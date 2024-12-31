use crate::jit::assembler::arm::alu_assembler::AluShiftImm;
use crate::jit::assembler::block_asm::{BlockTmpRegs, BLOCK_LOG};
use crate::jit::assembler::block_inst::{Alu, AluOp, AluSetCond, BlockInstType, SaveReg, SystemReg, SystemRegOp, Transfer, TransferOp};
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockAsmBuf, BlockLabel, BlockReg};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, ShiftType};
use crate::IS_DEBUG;
use std::fmt::{Debug, Formatter};
use std::hint::assert_unchecked;
use std::ptr;

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

    pub inst_indices: Vec<usize>,

    pub start_pc: u32,

    pre_opcodes_indices: Vec<usize>,
    pub opcodes: Vec<u32>,
    opcodes_emitted: bool,
}

impl BasicBlock {
    pub fn new() -> Self {
        BasicBlock {
            block_asm_buf_ptr: ptr::null(),

            block_entry_start: 0,
            block_entry_end: 0,

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
            opcodes_emitted: false,
        }
    }

    pub fn init(&mut self, buf: &mut BlockAsmBuf, block_entry_start: usize, block_entry_end: usize) {
        self.block_asm_buf_ptr = buf as _;

        self.block_entry_start = block_entry_start;
        self.block_entry_end = block_entry_end;

        self.pad_label = None;
        self.pad_size = 0;

        self.guest_regs_resolved = false;
        self.guest_regs_input_dirty = RegReserve::new();
        self.guest_regs_output_dirty = RegReserve::new();

        self.io_resolved = false;

        self.enter_blocks.clear();
        self.exit_blocks.clear();

        self.inst_indices.clear();

        self.opcodes.clear();
        self.opcodes_emitted = false;
    }

    pub fn init_guest_regs(&mut self, buf: &mut BlockAsmBuf) {
        self.guest_regs_output_dirty = self.guest_regs_input_dirty;

        for i in self.block_entry_start..=self.block_entry_end {
            match &mut buf.get_inst_mut(i).inst_type {
                BlockInstType::SaveContext(inner) => {
                    inner.guest_regs = self.guest_regs_output_dirty;
                    self.guest_regs_output_dirty.clear();
                }
                BlockInstType::SaveReg(inner) => self.guest_regs_output_dirty -= inner.guest_reg,
                BlockInstType::RestoreReg(inner) => self.guest_regs_output_dirty -= inner.guest_reg,
                BlockInstType::MarkRegDirty(inner) => {
                    if inner.dirty {
                        self.guest_regs_output_dirty += inner.guest_reg;
                    } else {
                        self.guest_regs_output_dirty -= inner.guest_reg;
                    }
                }
                _ => {
                    let inst = buf.get_inst(i);
                    let (_, outputs) = inst.get_io();
                    self.guest_regs_output_dirty += outputs.get_guests();
                }
            }
        }
    }

    pub fn init_insts(&mut self, buf: &mut BlockAsmBuf, tmp_regs: &BlockTmpRegs, basic_block_start_pc: u32, thumb: bool) {
        self.start_pc = basic_block_start_pc;

        let mut last_pc = basic_block_start_pc;

        for i in self.block_entry_start..=self.block_entry_end {
            match &buf.get_inst(i).inst_type {
                BlockInstType::Label(inner) => {
                    if let Some(pc) = inner.guest_pc {
                        last_pc = pc;
                    }
                }
                BlockInstType::GuestPc(inner) => last_pc = inner.0,
                BlockInstType::SaveContext(inner) => {
                    // Unroll regs to save into individual save regs, easier on reg allocator later on
                    for guest_reg in inner.guest_regs {
                        self.inst_indices.push(buf.insts.len());
                        buf.insts.push(
                            SaveReg {
                                guest_reg,
                                reg_mapped: BlockReg::from(guest_reg),
                                thread_regs_addr_reg: tmp_regs.thread_regs_addr_reg,
                                tmp_host_cpsr_reg: tmp_regs.host_cpsr_reg,
                            }
                            .into(),
                        );
                    }
                    continue;
                }
                BlockInstType::SaveReg(_) | BlockInstType::MarkRegDirty(_) => {}
                BlockInstType::PadBlock(inner) => {
                    self.pad_label = Some((inner.label, inner.correction));
                }
                inst_type => {
                    let (inputs, _) = buf.get_inst(i).get_io();
                    let inputs = inputs.get_guests();

                    if inputs.is_reserved(Reg::PC) {
                        let mut last_pc = last_pc + if thumb { 4 } else { 8 };
                        if thumb {
                            if let BlockInstType::Alu(inner) = inst_type {
                                if inner.thumb_pc_aligned {
                                    last_pc &= !0x3;
                                }
                            }
                        } else if let BlockInstType::GenericGuest(inner) = inst_type {
                            // PC + 12 when ALU shift by register
                            if inner.inst_info.op.is_alu_reg_shift() && *inner.inst_info.operands().last().unwrap().as_reg().unwrap().0 == Reg::PC {
                                last_pc += 4;
                            }
                        }

                        self.inst_indices.push(buf.insts.len());
                        buf.insts.push(Alu::alu2(AluOp::Mov, [Reg::PC.into(), last_pc.into()], AluSetCond::None, false).into());
                    }

                    if inputs.is_reserved(Reg::CPSR) {
                        self.inst_indices.push(buf.insts.len());
                        buf.insts.push(
                            SystemReg {
                                op: SystemRegOp::Mrs,
                                operand: tmp_regs.host_cpsr_reg.into(),
                            }
                            .into(),
                        );

                        self.inst_indices.push(buf.insts.len());
                        buf.insts.push(
                            Transfer {
                                op: TransferOp::Read,
                                operands: [tmp_regs.guest_cpsr_reg.into(), tmp_regs.thread_regs_addr_reg.into(), (Reg::CPSR as u32 * 4).into()],
                                signed: false,
                                amount: MemoryAmount::Half,
                                add_to_base: true,
                            }
                            .into(),
                        );

                        self.inst_indices.push(buf.insts.len());
                        buf.insts.push(
                            Alu::alu3(
                                AluOp::And,
                                [tmp_regs.host_cpsr_reg.into(), tmp_regs.host_cpsr_reg.into(), (0xF8, ShiftType::Ror, 4).into()],
                                AluSetCond::None,
                                false,
                            )
                            .into(),
                        );

                        self.inst_indices.push(buf.insts.len());
                        buf.insts.push(
                            Alu::alu3(
                                AluOp::Orr,
                                [tmp_regs.host_cpsr_reg.into(), tmp_regs.host_cpsr_reg.into(), tmp_regs.guest_cpsr_reg.into()],
                                AluSetCond::None,
                                false,
                            )
                            .into(),
                        );

                        buf.get_inst_mut(i).replace_input_regs(Reg::CPSR.into(), tmp_regs.host_cpsr_reg.into());
                    }

                    if inputs.is_reserved(Reg::SPSR) {
                        self.inst_indices.push(buf.insts.len());
                        buf.insts.push(
                            Transfer {
                                op: TransferOp::Read,
                                operands: [Reg::SPSR.into(), tmp_regs.thread_regs_addr_reg.into(), (Reg::SPSR as u32 * 4).into()],
                                signed: false,
                                amount: MemoryAmount::Word,
                                add_to_base: true,
                            }
                            .into(),
                        );
                    }
                }
            }

            self.inst_indices.push(i);
        }

        self.regs_live_ranges.resize(self.inst_indices.len() + 1, BlockRegSet::new());
        self.regs_live_ranges.last_mut().unwrap().clear();
        self.pre_opcodes_indices.resize(self.inst_indices.len() + 2, 0);
    }

    pub fn init_resolve_io(&mut self, buf: &BlockAsmBuf) {
        for (i, &inst_index) in self.inst_indices.iter().enumerate().rev() {
            unsafe { assert_unchecked(i + 1 < self.regs_live_ranges.len()) };
            let (inputs, outputs) = buf.get_inst(inst_index).get_io();
            let mut previous_ranges = self.regs_live_ranges[i + 1];
            previous_ranges -= outputs;
            self.regs_live_ranges[i] = previous_ranges + inputs;
        }
    }

    pub fn remove_dead_code(&mut self, buf: &mut BlockAsmBuf) {
        for (i, &inst_index) in self.inst_indices.iter().enumerate() {
            let inst = buf.get_inst_mut(inst_index);
            if let BlockInstType::RestoreReg(inner) = &inst.inst_type {
                if inner.guest_reg != Reg::CPSR {
                    let (_, outputs) = inst.get_io();
                    unsafe { assert_unchecked(i + 1 < self.regs_live_ranges.len()) };
                    if (self.regs_live_ranges[i + 1] - outputs) == self.regs_live_ranges[i + 1] {
                        inst.skip = true;
                    }
                }
            }
        }
    }

    pub fn set_required_outputs(&mut self, required_outputs: BlockRegSet) {
        let last = self.regs_live_ranges.len() - 1;
        unsafe { *self.regs_live_ranges.get_unchecked_mut(last) = required_outputs }
    }

    pub fn get_required_inputs(&self) -> &BlockRegSet {
        unsafe { self.regs_live_ranges.get_unchecked(0) }
    }

    pub fn get_required_outputs(&self) -> &BlockRegSet {
        unsafe { self.regs_live_ranges.get_unchecked(self.regs_live_ranges.len() - 1) }
    }

    pub fn allocate_regs(&mut self, buf: &mut BlockAsmBuf) {
        for (i, &inst_index) in self.inst_indices.iter().enumerate() {
            let inst = unsafe { buf.insts.get_unchecked_mut(inst_index) };
            self.pre_opcodes_indices[i] = buf.reg_allocator.pre_allocate_insts.len();
            if !inst.skip {
                buf.reg_allocator.inst_allocate(inst, &self.regs_live_ranges[i..]);
            }
        }

        let last_index = self.inst_indices.len() - 1;
        let pre_opcodes_start = buf.reg_allocator.pre_allocate_insts.len();
        self.pre_opcodes_indices[last_index + 1] = pre_opcodes_start;

        if !self.exit_blocks.is_empty() {
            buf.reg_allocator.ensure_global_mappings(*self.get_required_outputs());

            let end_entry = *self.inst_indices.last().unwrap();
            // Make sure to restore mapping before a branch
            if let BlockInstType::Branch(_) = &buf.get_inst(end_entry).inst_type {
                self.pre_opcodes_indices[last_index] = pre_opcodes_start;
                self.pre_opcodes_indices[last_index + 1] = buf.reg_allocator.pre_allocate_insts.len();
            }
        }

        self.pre_opcodes_indices[last_index + 2] = buf.reg_allocator.pre_allocate_insts.len();
    }

    pub fn emit_opcodes(&mut self, buf: &mut BlockAsmBuf, emit_local: bool, block_index: usize, used_host_regs: RegReserve) {
        if self.opcodes_emitted {
            if !emit_local {
                buf.opcodes.extend(&self.opcodes);
            }
            return;
        }

        let (opcodes_start_len, opcodes) = if emit_local { (0, &mut self.opcodes) } else { (buf.opcodes.len(), &mut buf.opcodes) };
        self.opcodes_emitted = true;

        unsafe { assert_unchecked(block_index < buf.branch_placeholders.len()) }
        buf.branch_placeholders[block_index].clear();

        for (i, &inst_index) in self.inst_indices.iter().enumerate() {
            let inst = unsafe { buf.insts.get_unchecked_mut(inst_index) };
            if inst.skip {
                continue;
            }

            let pre_opcodes_start = unsafe { *self.pre_opcodes_indices.get_unchecked(i) };
            let pre_opcodes_end = unsafe { *self.pre_opcodes_indices.get_unchecked(i + 1) };
            if pre_opcodes_start < pre_opcodes_end {
                unsafe { assert_unchecked(pre_opcodes_end <= buf.reg_allocator.pre_allocate_insts.len()) };
                opcodes.extend(&buf.reg_allocator.pre_allocate_insts[pre_opcodes_start..pre_opcodes_end]);
            }

            let start_len = opcodes.len();

            if IS_DEBUG && unsafe { BLOCK_LOG } && !emit_local {
                match &inst.inst_type {
                    BlockInstType::GuestPc(inner) => {
                        println!("(0x{:x}, 0x{:x}),", start_len, inner.0);
                    }
                    BlockInstType::Label(inner) => {
                        if let Some(pc) = inner.guest_pc {
                            println!("(0x{:x}, 0x{pc:x}),", start_len);
                        }
                    }
                    _ => {}
                }
            }

            inst.emit_opcode(opcodes, start_len - opcodes_start_len, &mut buf.branch_placeholders[block_index], used_host_regs);
            if inst.cond != Cond::AL {
                for opcode in &mut opcodes[start_len..] {
                    *opcode = (*opcode & !(0xF << 28)) | ((inst.cond as u32) << 28);
                }
            }
        }

        let pre_opcodes_start = unsafe { *self.pre_opcodes_indices.get_unchecked(self.pre_opcodes_indices.len() - 2) };
        let pre_opcodes_end = unsafe { *self.pre_opcodes_indices.get_unchecked(self.pre_opcodes_indices.len() - 1) };
        if pre_opcodes_start < pre_opcodes_end {
            unsafe { assert_unchecked(pre_opcodes_end <= buf.reg_allocator.pre_allocate_insts.len()) };
            opcodes.extend(&buf.reg_allocator.pre_allocate_insts[pre_opcodes_start..pre_opcodes_end]);
        }

        for _ in opcodes.len() - opcodes_start_len..self.pad_size {
            opcodes.push(AluShiftImm::mov_al(Reg::R0, Reg::R0));
        }
    }
}

impl Debug for BasicBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.inst_indices.is_empty() {
            writeln!(f, "BasicBlock: uninitialized enter blocks: {:?}", self.enter_blocks)?;
            for i in self.block_entry_start..self.block_entry_end {
                let inst = unsafe { self.block_asm_buf_ptr.as_ref().unwrap().get_inst(i) };
                writeln!(f, "\t{inst:?}")?;
                let (inputs, outputs) = inst.get_io();
                writeln!(f, "\t\tinputs: {inputs:?}, outputs: {outputs:?}")?;
            }
            write!(f, "BasicBlock end exit blocks: {:?}", self.exit_blocks)
        } else {
            writeln!(f, "BasicBlock: inputs: {:?} enter blocks: {:?}", self.regs_live_ranges.first().unwrap(), self.enter_blocks,)?;
            for (i, &inst_index) in self.inst_indices.iter().enumerate() {
                let inst = unsafe { self.block_asm_buf_ptr.as_ref().unwrap().get_inst(inst_index) };
                writeln!(f, "\t{inst:?}")?;
                let (inputs, outputs) = inst.get_io();
                writeln!(f, "\t\tinputs: {inputs:?}, outputs: {outputs:?}")?;
                writeln!(f, "\t\tlive range: {:?}", self.regs_live_ranges[i])?;
            }
            write!(f, "BasicBlock end: outputs: {:?} exit blocks: {:?}", self.regs_live_ranges.last().unwrap(), self.exit_blocks,)
        }
    }
}
