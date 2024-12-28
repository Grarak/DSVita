use crate::jit::assembler::arm::alu_assembler::AluShiftImm;
use crate::jit::assembler::block_asm::{BlockAsm, BLOCK_LOG};
use crate::jit::assembler::block_inst::{BlockAluOp, BlockAluSetCond, BlockSystemRegOp, BlockTransferOp};
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockAsmBuf, BlockInstKind, BlockLabel};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{MemoryAmount, ShiftType};
use crate::IS_DEBUG;
use std::fmt::{Debug, Formatter};

pub struct BasicBlock {
    block_asm_buf_ptr: *const BlockAsmBuf,

    pub block_entry_start: usize,
    pub block_entry_end: usize,

    pub pad_label: Option<(BlockLabel, i32)>,
    pub pad_size: usize,

    pub guest_regs_resolved: bool,
    pub guest_regs_input_dirty: RegReserve,
    pub guest_regs_output_dirty: RegReserve,

    pub io_resolved: bool,
    pub regs_live_ranges: Vec<BlockRegSet>,

    pub enter_blocks: Vec<usize>,
    pub exit_blocks: Vec<usize>,

    pub inst_indices: Vec<u16>,

    pub start_pc: u32,

    pre_opcodes_indices: Vec<u16>,
    pub opcodes: Vec<u32>,
}

impl BasicBlock {
    pub fn new(asm: &mut BlockAsm, block_entry_start: usize, block_entry_end: usize) -> Self {
        BasicBlock {
            block_asm_buf_ptr: asm.buf as *mut _,

            block_entry_start,
            block_entry_end,

            pad_label: None,
            pad_size: 0,

            guest_regs_resolved: false,
            guest_regs_input_dirty: RegReserve::new(),
            guest_regs_output_dirty: RegReserve::new(),

            io_resolved: false,
            regs_live_ranges: Vec::new(),

            enter_blocks: Vec::new(),
            exit_blocks: Vec::with_capacity(2),

            inst_indices: Vec::new(),

            start_pc: 0,

            pre_opcodes_indices: Vec::new(),
            opcodes: Vec::new(),
        }
    }

    pub fn init_guest_regs(&mut self, asm: &mut BlockAsm) {
        self.guest_regs_output_dirty = self.guest_regs_input_dirty;

        for i in self.block_entry_start..=self.block_entry_end {
            match &mut asm.buf.insts[i].kind {
                BlockInstKind::SaveContext { guest_regs, .. } => {
                    *guest_regs = self.guest_regs_output_dirty;
                    self.guest_regs_output_dirty.clear();
                }
                BlockInstKind::SaveReg { guest_reg, .. } | BlockInstKind::RestoreReg { guest_reg, .. } | BlockInstKind::MarkRegDirty { guest_reg, dirty: false } => {
                    self.guest_regs_output_dirty -= *guest_reg
                }
                BlockInstKind::MarkRegDirty { guest_reg, dirty: true } => self.guest_regs_output_dirty += *guest_reg,
                _ => {
                    let inst = &asm.buf.insts[i];
                    let (_, outputs) = inst.get_io();
                    self.guest_regs_output_dirty += outputs.get_guests();
                }
            }
        }
    }

    pub fn init_insts(&mut self, asm: &mut BlockAsm, basic_block_start_pc: u32, thumb: bool) {
        self.start_pc = basic_block_start_pc;

        let mut last_pc = basic_block_start_pc;

        for i in self.block_entry_start..=self.block_entry_end {
            let mut add_inst = true;

            match &mut asm.buf.insts[i].kind {
                BlockInstKind::Label { guest_pc: Some(pc), .. } => last_pc = *pc,
                BlockInstKind::GuestPc(pc) => last_pc = *pc,
                BlockInstKind::SaveContext { guest_regs, thread_regs_addr_reg } => {
                    let thread_regs_addr_reg = *thread_regs_addr_reg;
                    // Unroll regs to save into individual save regs, easier on reg allocator later on
                    for guest_reg in *guest_regs {
                        self.inst_indices.push(asm.buf.insts.len() as u16);
                        asm.buf.insts.push(
                            BlockInstKind::SaveReg {
                                guest_reg,
                                reg_mapped: guest_reg.into(),
                                thread_regs_addr_reg,
                            }
                            .into(),
                        );
                    }
                    add_inst = false;
                }
                BlockInstKind::SaveReg { .. } | BlockInstKind::MarkRegDirty { .. } => {}
                BlockInstKind::PadBlock { label, correction } => self.pad_label = Some((*label, *correction)),
                _ => {
                    let (inputs, _) = asm.buf.insts[i].get_io();
                    for guest_reg in inputs.get_guests() {
                        match guest_reg {
                            Reg::PC => {
                                let mut last_pc = last_pc + if thumb { 4 } else { 8 };
                                if thumb {
                                    match &asm.buf.insts[i].kind {
                                        BlockInstKind::Alu3 { thumb_pc_aligned, .. }
                                        | BlockInstKind::Alu2Op1 { thumb_pc_aligned, .. }
                                        | BlockInstKind::Alu2Op0 { thumb_pc_aligned, .. }
                                        | BlockInstKind::Mul { thumb_pc_aligned, .. } => {
                                            if *thumb_pc_aligned {
                                                last_pc &= !0x3;
                                            }
                                        }
                                        _ => {}
                                    }
                                } else if let BlockInstKind::GenericGuestInst { inst, .. } = &asm.buf.insts[i].kind {
                                    // PC + 12 when ALU shift by register
                                    if inst.op.is_alu_reg_shift() && *inst.operands().last().unwrap().as_reg().unwrap().0 == Reg::PC {
                                        last_pc += 4;
                                    }
                                }

                                self.inst_indices.push(asm.buf.insts.len() as u16);
                                asm.buf.insts.push(
                                    BlockInstKind::Alu2Op0 {
                                        op: BlockAluOp::Mov,
                                        operands: [Reg::PC.into(), last_pc.into()],
                                        set_cond: BlockAluSetCond::None,
                                        thumb_pc_aligned: false,
                                    }
                                    .into(),
                                );
                            }
                            Reg::CPSR => {
                                self.inst_indices.push(asm.buf.insts.len() as u16);
                                asm.buf.insts.push(
                                    BlockInstKind::SystemReg {
                                        op: BlockSystemRegOp::Mrs,
                                        operand: Reg::CPSR.into(),
                                    }
                                    .into(),
                                );

                                self.inst_indices.push(asm.buf.insts.len() as u16);
                                asm.buf.insts.push(
                                    BlockInstKind::Transfer {
                                        op: BlockTransferOp::Read,
                                        operands: [asm.tmp_guest_cpsr_reg.into(), asm.thread_regs_addr_reg.into(), (Reg::CPSR as u32 * 4).into()],
                                        signed: false,
                                        amount: MemoryAmount::Half,
                                        add_to_base: true,
                                    }
                                    .into(),
                                );

                                self.inst_indices.push(asm.buf.insts.len() as u16);
                                asm.buf.insts.push(
                                    BlockInstKind::Alu3 {
                                        op: BlockAluOp::And,
                                        operands: [Reg::CPSR.into(), Reg::CPSR.into(), (0xF8, ShiftType::Ror, 4).into()],
                                        set_cond: BlockAluSetCond::None,
                                        thumb_pc_aligned: false,
                                    }
                                    .into(),
                                );

                                self.inst_indices.push(asm.buf.insts.len() as u16);
                                asm.buf.insts.push(
                                    BlockInstKind::Alu3 {
                                        op: BlockAluOp::Orr,
                                        operands: [Reg::CPSR.into(), Reg::CPSR.into(), asm.tmp_guest_cpsr_reg.into()],
                                        set_cond: BlockAluSetCond::None,
                                        thumb_pc_aligned: false,
                                    }
                                    .into(),
                                );
                            }
                            Reg::SPSR => {
                                self.inst_indices.push(asm.buf.insts.len() as u16);
                                asm.buf.insts.push(
                                    BlockInstKind::Transfer {
                                        op: BlockTransferOp::Read,
                                        operands: [Reg::SPSR.into(), asm.thread_regs_addr_reg.into(), (Reg::SPSR as u32 * 4).into()],
                                        signed: false,
                                        amount: MemoryAmount::Word,
                                        add_to_base: true,
                                    }
                                    .into(),
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }

            if add_inst {
                self.inst_indices.push(i as u16);
            }
        }

        self.regs_live_ranges.resize(self.inst_indices.len() + 1, BlockRegSet::new());
        self.pre_opcodes_indices.resize(self.inst_indices.len() + 2, 0);
    }

    pub fn init_resolve_io(&mut self, asm: &BlockAsm) {
        for (i, &inst_index) in self.inst_indices.iter().enumerate().rev() {
            let (inputs, outputs) = asm.buf.insts[inst_index as usize].get_io();
            let mut previous_ranges = self.regs_live_ranges[i + 1];
            previous_ranges -= outputs;
            self.regs_live_ranges[i] = previous_ranges + inputs;
        }
    }

    pub fn remove_dead_code(&mut self, asm: &mut BlockAsm) {
        for (i, &inst_index) in self.inst_indices.iter().enumerate() {
            let inst = &mut asm.buf.insts[inst_index as usize];
            if let BlockInstKind::RestoreReg { guest_reg, .. } = &inst.kind {
                if *guest_reg != Reg::CPSR {
                    let (_, outputs) = inst.get_io();
                    if (self.regs_live_ranges[i + 1] - outputs) == self.regs_live_ranges[i + 1] {
                        inst.skip = true;
                    }
                }
            }
        }
    }

    pub fn set_required_outputs(&mut self, required_outputs: BlockRegSet) {
        *self.regs_live_ranges.last_mut().unwrap() = required_outputs;
    }

    pub fn get_required_inputs(&self) -> &BlockRegSet {
        self.regs_live_ranges.first().unwrap()
    }

    pub fn get_required_outputs(&self) -> &BlockRegSet {
        self.regs_live_ranges.last().unwrap()
    }

    pub fn allocate_regs(&mut self, asm: &mut BlockAsm) {
        for (i, &inst_index) in self.inst_indices.iter().enumerate() {
            let inst = &mut asm.buf.insts[inst_index as usize];
            self.pre_opcodes_indices[i] = asm.buf.reg_allocator.pre_allocate_insts.len() as u16;
            if !inst.skip {
                asm.buf.reg_allocator.inst_allocate(inst, &self.regs_live_ranges[i..]);
            }
        }

        let last_index = self.inst_indices.len() - 1;
        let pre_opcodes_start = asm.buf.reg_allocator.pre_allocate_insts.len();
        self.pre_opcodes_indices[last_index + 1] = pre_opcodes_start as u16;

        if !self.exit_blocks.is_empty() {
            asm.buf.reg_allocator.ensure_global_mappings(*self.get_required_outputs());

            let end_entry = *self.inst_indices.last().unwrap() as usize;
            // Make sure to restore mapping before a branch
            if let BlockInstKind::Branch { .. } = &asm.buf.insts[end_entry].kind {
                self.pre_opcodes_indices[last_index] = pre_opcodes_start as u16;
                self.pre_opcodes_indices[last_index + 1] = asm.buf.reg_allocator.pre_allocate_insts.len() as u16;
            }
        }

        self.pre_opcodes_indices[last_index + 2] = asm.buf.reg_allocator.pre_allocate_insts.len() as u16;
    }

    pub fn emit_opcodes(&mut self, asm: &mut BlockAsm, opcodes_offset: usize, block_index: usize, used_host_regs: RegReserve) {
        // if IS_DEBUG && unsafe { BLOCK_LOG } && opcodes_offset != 0 {
        //     self.opcodes.clear();
        // }

        if !self.opcodes.is_empty() {
            return;
        }

        asm.buf.branch_placeholders[block_index].clear();

        for (i, &inst_index) in self.inst_indices.iter().enumerate() {
            let inst = &mut asm.buf.insts[inst_index as usize];
            if inst.skip {
                continue;
            }

            self.opcodes
                .extend(&asm.buf.reg_allocator.pre_allocate_insts[self.pre_opcodes_indices[i] as usize..self.pre_opcodes_indices[i + 1] as usize]);

            let start_len = self.opcodes.len();

            if IS_DEBUG && unsafe { BLOCK_LOG } && opcodes_offset != 0 {
                match &inst.kind {
                    BlockInstKind::GuestPc(pc) => {
                        println!("(0x{:x}, 0x{pc:x}),", start_len + opcodes_offset);
                    }
                    BlockInstKind::Label { guest_pc: Some(pc), .. } => {
                        println!("(0x{:x}, 0x{pc:x}),", start_len + opcodes_offset);
                    }
                    _ => {}
                }
            }

            inst.kind.emit_opcode(&mut self.opcodes, start_len, &mut asm.buf.branch_placeholders[block_index], used_host_regs);
            if !inst.kind.unconditional() {
                for i in start_len..self.opcodes.len() {
                    self.opcodes[i] = (self.opcodes[i] & !(0xF << 28)) | ((inst.cond as u32) << 28);
                }
            }
        }

        self.opcodes.extend(
            &asm.buf.reg_allocator.pre_allocate_insts[self.pre_opcodes_indices[self.pre_opcodes_indices.len() - 2] as usize..self.pre_opcodes_indices[self.pre_opcodes_indices.len() - 1] as usize],
        );

        for _ in self.opcodes.len()..self.pad_size {
            self.opcodes.push(AluShiftImm::mov_al(Reg::R0, Reg::R0));
        }
    }
}

impl Debug for BasicBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.inst_indices.is_empty() {
            writeln!(f, "BasicBlock: uninitialized enter blocks: {:?}", self.enter_blocks)?;
            for i in self.block_entry_start..self.block_entry_end {
                let inst = unsafe { &self.block_asm_buf_ptr.as_ref().unwrap().insts[i] };
                writeln!(f, "\t{inst:?}")?;
                let (inputs, outputs) = inst.get_io();
                writeln!(f, "\t\tinputs: {inputs:?}, outputs: {outputs:?}")?;
            }
            write!(f, "BasicBlock end exit blocks: {:?}", self.exit_blocks)
        } else {
            writeln!(f, "BasicBlock: inputs: {:?} enter blocks: {:?}", self.regs_live_ranges.first().unwrap(), self.enter_blocks,)?;
            for (i, &inst_index) in self.inst_indices.iter().enumerate() {
                let inst = unsafe { &self.block_asm_buf_ptr.as_ref().unwrap().insts[inst_index as usize] };
                writeln!(f, "\t{inst:?}")?;
                let (inputs, outputs) = inst.get_io();
                writeln!(f, "\t\tinputs: {inputs:?}, outputs: {outputs:?}")?;
                writeln!(f, "\t\tlive range: {:?}", self.regs_live_ranges[i])?;
            }
            write!(f, "BasicBlock end: outputs: {:?} exit blocks: {:?}", self.regs_live_ranges.last().unwrap(), self.exit_blocks,)
        }
    }
}
