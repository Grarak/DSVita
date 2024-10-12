use crate::jit::assembler::block_asm::{BlockAsm, BLOCK_LOG};
use crate::jit::assembler::block_inst::{BlockAluOp, BlockAluSetCond, BlockSystemRegOp, BlockTransferOp};
use crate::jit::assembler::block_inst_list::{BlockInstList, BlockInstListEntry};
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockAsmBuf, BlockInst};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{MemoryAmount, ShiftType};
use crate::IS_DEBUG;
use std::fmt::{Debug, Formatter};

pub struct BasicBlock {
    block_asm_buf_ptr: *const BlockAsmBuf,

    pub block_entry_start: *mut BlockInstListEntry,
    pub block_entry_end: *mut BlockInstListEntry,

    pub io_resolved: bool,
    pub regs_live_ranges: Vec<BlockRegSet>,
    pub used_regs: Vec<BlockRegSet>,

    pub enter_blocks: Vec<usize>,
    pub exit_blocks: Vec<usize>,

    pub insts_link: BlockInstList,

    pub start_pc: u32,
}

impl BasicBlock {
    pub fn new(asm: &mut BlockAsm, block_entry_start: *mut BlockInstListEntry, block_entry_end: *mut BlockInstListEntry) -> Self {
        BasicBlock {
            block_asm_buf_ptr: asm.buf as *mut _,

            block_entry_start,
            block_entry_end,

            io_resolved: false,
            regs_live_ranges: Vec::new(),
            used_regs: Vec::new(),

            enter_blocks: Vec::new(),
            exit_blocks: Vec::with_capacity(2),

            insts_link: BlockInstList::new(),

            start_pc: 0,
        }
    }

    pub fn init_insts(&mut self, asm: &mut BlockAsm, basic_block_start_pc: u32, thumb: bool) {
        self.start_pc = basic_block_start_pc;

        let mut initialized_guest_regs = RegReserve::new();
        let mut last_guest_regs_outputs = [None; Reg::SPSR as usize];

        let mut pc_initialized = false;
        let mut last_pc = basic_block_start_pc;
        let mut last_pc_reg = Reg::PC.into();

        let mut current_node = self.block_entry_start;
        loop {
            let i = BlockInstList::deref(current_node).value;
            let mut add_inst = true;
            let mut guest_regs_outputs = RegReserve::new();
            match &mut asm.buf.insts[i] {
                BlockInst::Label { guest_pc: Some(pc), .. } => last_pc = *pc,
                BlockInst::GuestPc(pc) => last_pc = *pc,
                _ => match asm.buf.insts[i] {
                    BlockInst::SaveContext {
                        thread_regs_addr_reg,
                        tmp_guest_cpsr_reg,
                    } => {
                        // Unroll regs to save into individual save regs, easier on reg allocator later on
                        for (i, node) in last_guest_regs_outputs.iter().enumerate() {
                            if node.is_some() {
                                let guest_reg = Reg::from(i as u8);
                                self.insts_link.insert_end(asm.buf.insts.len());
                                asm.buf.insts.push(BlockInst::SaveReg {
                                    guest_reg,
                                    reg_mapped: if guest_reg == Reg::PC { last_pc_reg } else { guest_reg.into() },
                                    thread_regs_addr_reg,
                                    tmp_guest_cpsr_reg,
                                });
                            }
                        }
                        last_guest_regs_outputs.fill(None);
                        add_inst = false;
                    }
                    BlockInst::SaveReg {
                        guest_reg,
                        thread_regs_addr_reg,
                        tmp_guest_cpsr_reg,
                        ..
                    } => {
                        if guest_reg == Reg::PC {
                            asm.buf.insts[i] = BlockInst::SaveReg {
                                guest_reg: Reg::PC,
                                reg_mapped: last_pc_reg,
                                thread_regs_addr_reg,
                                tmp_guest_cpsr_reg,
                            }
                        }
                        last_guest_regs_outputs[guest_reg as usize] = None;
                    }
                    BlockInst::RestoreReg {
                        guest_reg,
                        thread_regs_addr_reg,
                        tmp_guest_cpsr_reg,
                        ..
                    } => {
                        if guest_reg == Reg::PC {
                            asm.buf.insts[i] = BlockInst::RestoreReg {
                                guest_reg: Reg::PC,
                                reg_mapped: last_pc_reg,
                                thread_regs_addr_reg,
                                tmp_guest_cpsr_reg,
                            }
                        }
                        last_guest_regs_outputs[guest_reg as usize] = None;
                    }
                    _ => {
                        let (inputs, outputs) = asm.buf.insts[i].get_io();

                        for guest_reg in inputs.get_guests() {
                            match guest_reg {
                                Reg::PC => {
                                    if !pc_initialized {
                                        pc_initialized = true;
                                        last_pc_reg = Reg::PC.into();
                                        self.insts_link.insert_end(asm.buf.insts.len());
                                        asm.buf.insts.push(BlockInst::Alu2Op0 {
                                            op: BlockAluOp::Mov,
                                            operands: [Reg::PC.into(), basic_block_start_pc.into()],
                                            set_cond: BlockAluSetCond::None,
                                            thumb_pc_aligned: false,
                                        });
                                    }

                                    let mut last_pc = last_pc + if thumb { 4 } else { 8 };
                                    if thumb {
                                        match &asm.buf.insts[i] {
                                            BlockInst::Alu3 { thumb_pc_aligned, .. }
                                            | BlockInst::Alu2Op1 { thumb_pc_aligned, .. }
                                            | BlockInst::Alu2Op0 { thumb_pc_aligned, .. }
                                            | BlockInst::Mul { thumb_pc_aligned, .. } => {
                                                if *thumb_pc_aligned {
                                                    last_pc &= !0x3;
                                                }
                                            }
                                            _ => {}
                                        }
                                    } else if let BlockInst::GenericGuestInst { inst, .. } = &asm.buf.insts[i] {
                                        // PC + 12 when ALU shift by register
                                        if inst.op.is_alu_reg_shift() && *inst.operands().last().unwrap().as_reg().unwrap().0 == Reg::PC {
                                            last_pc += 4;
                                        }
                                    }

                                    let pc_diff = last_pc - basic_block_start_pc;
                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(if pc_diff & !0xFF != 0 {
                                        BlockInst::Alu2Op0 {
                                            op: BlockAluOp::Mov,
                                            operands: [asm.tmp_adjusted_pc_reg.into(), last_pc.into()],
                                            set_cond: BlockAluSetCond::None,
                                            thumb_pc_aligned: false,
                                        }
                                    } else {
                                        BlockInst::Alu3 {
                                            op: BlockAluOp::Add,
                                            operands: [asm.tmp_adjusted_pc_reg.into(), Reg::PC.into(), pc_diff.into()],
                                            set_cond: BlockAluSetCond::None,
                                            thumb_pc_aligned: false,
                                        }
                                    });
                                    asm.buf.insts[i].replace_regs(Reg::PC.into(), asm.tmp_adjusted_pc_reg);

                                    if outputs.contains(Reg::PC.into()) {
                                        last_pc_reg = asm.tmp_adjusted_pc_reg;
                                    }
                                }
                                Reg::CPSR => {
                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(BlockInst::SystemReg {
                                        op: BlockSystemRegOp::Mrs,
                                        operand: Reg::CPSR.into(),
                                    });
                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(BlockInst::Transfer {
                                        op: BlockTransferOp::Read,
                                        operands: [asm.tmp_guest_cpsr_reg.into(), asm.thread_regs_addr_reg.into(), (Reg::CPSR as u32 * 4).into()],
                                        signed: false,
                                        amount: MemoryAmount::Word,
                                        add_to_base: true,
                                    });
                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(BlockInst::Alu3 {
                                        op: BlockAluOp::And,
                                        operands: [Reg::CPSR.into(), Reg::CPSR.into(), (0xF8, ShiftType::Ror, 4).into()],
                                        set_cond: BlockAluSetCond::None,
                                        thumb_pc_aligned: false,
                                    });
                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(BlockInst::Alu3 {
                                        op: BlockAluOp::Bic,
                                        operands: [asm.tmp_guest_cpsr_reg.into(), asm.tmp_guest_cpsr_reg.into(), (0xF8, ShiftType::Ror, 4).into()],
                                        set_cond: BlockAluSetCond::None,
                                        thumb_pc_aligned: false,
                                    });
                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(BlockInst::Alu3 {
                                        op: BlockAluOp::Orr,
                                        operands: [Reg::CPSR.into(), Reg::CPSR.into(), asm.tmp_guest_cpsr_reg.into()],
                                        set_cond: BlockAluSetCond::None,
                                        thumb_pc_aligned: false,
                                    });
                                }
                                Reg::SPSR => {
                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(BlockInst::Transfer {
                                        op: BlockTransferOp::Read,
                                        operands: [Reg::SPSR.into(), asm.thread_regs_addr_reg.into(), (Reg::SPSR as u32 * 4).into()],
                                        signed: false,
                                        amount: MemoryAmount::Word,
                                        add_to_base: true,
                                    });
                                }
                                _ if !initialized_guest_regs.is_reserved(guest_reg) => {
                                    initialized_guest_regs += guest_reg;
                                    self.insts_link.insert_end(asm.buf.insts.len());
                                    asm.buf.insts.push(BlockInst::RestoreReg {
                                        guest_reg,
                                        reg_mapped: guest_reg.into(),
                                        thread_regs_addr_reg: asm.thread_regs_addr_reg,
                                        tmp_guest_cpsr_reg: asm.tmp_guest_cpsr_reg,
                                    });
                                }
                                _ => {}
                            }
                        }

                        if outputs.contains(Reg::PC.into()) {
                            pc_initialized = false;
                        }

                        guest_regs_outputs = outputs.get_guests();
                        initialized_guest_regs += outputs.get_guests();
                    }
                },
            }

            if add_inst {
                self.insts_link.insert_end(i);
                for reg in guest_regs_outputs {
                    last_guest_regs_outputs[reg as usize] = Some(self.insts_link.end);
                }
            }

            if current_node == self.block_entry_end {
                break;
            }

            current_node = BlockInstList::deref(current_node).next;
        }

        for (i, node) in last_guest_regs_outputs.iter().enumerate() {
            if let &Some(node) = node {
                let guest_reg = Reg::from(i as u8);
                if guest_reg == Reg::PC {
                    todo!()
                }

                if node.is_null() {
                    self.insts_link.insert_begin(asm.buf.insts.len());
                } else {
                    self.insts_link.insert_entry_end(node, asm.buf.insts.len());
                }
                asm.buf.insts.push(BlockInst::SaveReg {
                    guest_reg,
                    reg_mapped: guest_reg.into(),
                    thread_regs_addr_reg: asm.thread_regs_addr_reg,
                    tmp_guest_cpsr_reg: asm.tmp_guest_cpsr_reg,
                });
            }
        }

        self.regs_live_ranges.resize(self.insts_link.len() + 1, BlockRegSet::new());
        self.used_regs.resize(self.insts_link.len() + 1, BlockRegSet::new());
    }

    pub fn init_resolve_io(&mut self, asm: &BlockAsm) {
        let mut i = self.insts_link.len() - 1;
        for entry in self.insts_link.iter_rev() {
            let (inputs, outputs) = asm.buf.insts[entry.value].get_io();
            let mut previous_ranges = self.regs_live_ranges[i + 1];
            previous_ranges -= outputs;
            self.regs_live_ranges[i] = previous_ranges + inputs;
            self.used_regs[i] = inputs + outputs;
            i = i.wrapping_sub(1);
        }
    }

    pub fn remove_dead_code(&mut self, asm: &BlockAsm) {
        let mut current_node = self.insts_link.root;
        let mut i = 0;
        while !current_node.is_null() {
            let inst_i = BlockInstList::deref(current_node).value;
            let inst = &asm.buf.insts[inst_i];
            if let BlockInst::RestoreReg { guest_reg, .. } = inst {
                if *guest_reg != Reg::CPSR {
                    let (_, outputs) = inst.get_io();
                    if (self.regs_live_ranges[i + 1] - outputs) == self.regs_live_ranges[i + 1] {
                        let next_node = BlockInstList::deref(current_node).next;
                        self.insts_link.remove_entry(current_node);
                        current_node = next_node;
                        self.regs_live_ranges.remove(i);
                        self.used_regs.remove(i);
                        continue;
                    }
                }
            }

            i += 1;
            current_node = BlockInstList::deref(current_node).next;
        }
    }

    pub fn add_required_outputs(&mut self, required_outputs: BlockRegSet) {
        *self.regs_live_ranges.last_mut().unwrap() += required_outputs;
        *self.used_regs.last_mut().unwrap() += required_outputs;
    }

    pub fn set_required_outputs(&mut self, required_outputs: BlockRegSet) {
        *self.regs_live_ranges.last_mut().unwrap() = required_outputs;
        *self.used_regs.last_mut().unwrap() = required_outputs;
    }

    pub fn get_required_inputs(&self) -> &BlockRegSet {
        self.regs_live_ranges.first().unwrap()
    }

    pub fn get_required_outputs(&self) -> &BlockRegSet {
        self.regs_live_ranges.last().unwrap()
    }

    pub fn allocate_regs(&mut self, asm: &mut BlockAsm) {
        let mut i = 0;
        let mut current_node = self.insts_link.root;
        while !current_node.is_null() {
            let inst_i = BlockInstList::deref(current_node).value;
            asm.buf.reg_allocator.inst_allocate(&mut asm.buf.insts[inst_i], &self.regs_live_ranges[i..], &self.used_regs[i..]);
            if !asm.buf.reg_allocator.pre_allocate_insts.is_empty() {
                for i in asm.buf.insts.len()..asm.buf.insts.len() + asm.buf.reg_allocator.pre_allocate_insts.len() {
                    self.insts_link.insert_entry_begin(current_node, i);
                }
                asm.buf.insts.extend_from_slice(&asm.buf.reg_allocator.pre_allocate_insts);
            }
            i += 1;
            current_node = BlockInstList::deref(current_node).next;
        }

        if !self.exit_blocks.is_empty() {
            asm.buf.reg_allocator.ensure_global_mappings(*self.get_required_outputs());

            let end_entry = self.insts_link.end;
            // Make sure to restore mapping before a branch
            if let BlockInst::Branch { .. } = asm.buf.insts[BlockInstList::deref(end_entry).value] {
                for i in asm.buf.insts.len()..asm.buf.insts.len() + asm.buf.reg_allocator.pre_allocate_insts.len() {
                    self.insts_link.insert_entry_begin(end_entry, i);
                }
            } else {
                for i in asm.buf.insts.len()..asm.buf.insts.len() + asm.buf.reg_allocator.pre_allocate_insts.len() {
                    self.insts_link.insert_end(i);
                }
            }
            asm.buf.insts.extend_from_slice(&asm.buf.reg_allocator.pre_allocate_insts);
        }
    }

    pub fn emit_opcodes(&self, asm: &mut BlockAsm, opcodes_offset: usize, used_host_regs: RegReserve) -> Vec<u32> {
        let mut opcodes = Vec::new();
        let mut inst_opcodes = Vec::new();
        for entry in self.insts_link.iter() {
            let inst = &mut asm.buf.insts[entry.value];

            if IS_DEBUG && unsafe { BLOCK_LOG } {
                match inst {
                    BlockInst::GuestPc(pc) => {
                        println!("(0x{:x}, 0x{pc:x}),", opcodes.len() + opcodes_offset);
                    }
                    BlockInst::Label { guest_pc, .. } => {
                        if let Some(pc) = guest_pc {
                            println!("(0x{:x}, 0x{pc:x}),", opcodes.len() + opcodes_offset);
                        }
                    }
                    _ => {}
                }
            }

            inst_opcodes.clear();
            inst.emit_opcode(&mut inst_opcodes, opcodes.len(), &mut asm.buf.branch_placeholders, opcodes_offset, used_host_regs);
            opcodes.extend(&inst_opcodes);
        }
        opcodes
    }
}

impl Debug for BasicBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.insts_link.is_empty() {
            writeln!(f, "BasicBlock: uninitialized enter blocks: {:?}", self.enter_blocks)?;
            let mut current_node = self.block_entry_start;
            loop {
                let inst = unsafe { &self.block_asm_buf_ptr.as_ref().unwrap().insts[BlockInstList::deref(current_node).value] };
                writeln!(f, "\t{inst:?}")?;
                let (inputs, outputs) = inst.get_io();
                writeln!(f, "\t\tinputs: {inputs:?}, outputs: {outputs:?}")?;
                if current_node == self.block_entry_end {
                    break;
                }
                current_node = BlockInstList::deref(current_node).next;
            }
            write!(f, "BasicBlock end exit blocks: {:?}", self.exit_blocks)
        } else {
            writeln!(f, "BasicBlock: inputs: {:?} enter blocks: {:?}", self.regs_live_ranges.first().unwrap(), self.enter_blocks,)?;
            for (i, entry) in self.insts_link.iter().enumerate() {
                let inst = unsafe { &self.block_asm_buf_ptr.as_ref().unwrap().insts[entry.value] };
                writeln!(f, "\t{inst:?}")?;
                let (inputs, outputs) = inst.get_io();
                writeln!(f, "\t\tinputs: {inputs:?}, outputs: {outputs:?}")?;
                writeln!(f, "\t\tlive range: {:?}", self.regs_live_ranges[i])?;
            }
            write!(f, "BasicBlock end: outputs: {:?} exit blocks: {:?}", self.regs_live_ranges.last().unwrap(), self.exit_blocks,)
        }
    }
}
