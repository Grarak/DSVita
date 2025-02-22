use crate::jit::assembler::block_asm::BlockTmpRegs;
use crate::jit::assembler::block_inst::{Alu, AluOp, AluSetCond, BlockInst, BlockInstType, GuestTransferMultiple, RestoreReg, SaveReg, TransferOp};
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockAsmBuf, BlockReg};
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;
use std::fmt::{Debug, Formatter};
use std::hint::assert_unchecked;
use std::ptr;

pub struct BasicBlock {
    block_asm_buf_ptr: *const BlockAsmBuf,

    pub block_entry_start: usize,
    pub block_entry_end: usize,

    dirty_guest_regs: RegReserve,
    pub guest_regs_resolved: bool,

    pub io_resolved: bool,
    regs_live_ranges: Vec<BlockRegSet>,

    pub enter_blocks: Vec<usize>,
    pub exit_blocks: Vec<usize>,

    #[cfg(debug_assertions)]
    pub start_pc: u32,
}

impl BasicBlock {
    pub fn new() -> Self {
        BasicBlock {
            block_asm_buf_ptr: ptr::null(),

            block_entry_start: 0,
            block_entry_end: 0,

            dirty_guest_regs: RegReserve::new(),
            guest_regs_resolved: false,

            io_resolved: false,
            regs_live_ranges: Vec::new(),

            enter_blocks: Vec::new(),
            exit_blocks: Vec::with_capacity(2),

            #[cfg(debug_assertions)]
            start_pc: 0,
        }
    }

    pub fn init(&mut self, buf: &mut BlockAsmBuf, block_entry_start: usize, block_entry_end: usize) {
        self.block_asm_buf_ptr = buf as _;

        self.block_entry_start = block_entry_start;
        self.block_entry_end = block_entry_end;

        self.dirty_guest_regs.clear();
        self.guest_regs_resolved = false;

        self.io_resolved = false;

        self.enter_blocks.clear();
        self.exit_blocks.clear();
    }

    pub fn init_guest_regs(&mut self, tmp_regs: &BlockTmpRegs, buf: &mut BlockAsmBuf) {
        #[cfg(debug_assertions)]
        {
            for i in (0..self.block_entry_start + 1).rev() {
                match &buf.get_inst(i).inst_type {
                    BlockInstType::Label(inner) => {
                        if let Some(pc) = inner.guest_pc {
                            self.start_pc = pc;
                            break;
                        }
                    }
                    BlockInstType::GuestPc(inner) => {
                        self.start_pc = inner.0;
                        break;
                    }
                    _ => {}
                }
            }
        }

        let mut i = self.block_entry_start;
        while i <= self.block_entry_end {
            debug_assert!(!buf.get_inst(i).skip);
            match &mut buf.get_inst_mut(i).inst_type {
                BlockInstType::SaveContext(inner) => {
                    inner.guest_regs = self.dirty_guest_regs;

                    let mut inst: BlockInst = SaveReg {
                        guest_reg: Reg::R0,
                        reg_mapped: BlockReg::from(Reg::R0),
                        thread_regs_addr_reg: tmp_regs.thread_regs_addr_reg,
                        tmp_host_cpsr_reg: tmp_regs.host_cpsr_reg,
                    }
                    .into();
                    inst.skip = true;
                    *buf.get_inst_mut(i) = inst;
                    for j in Reg::R0 as u8..Reg::None as u8 {
                        if self.dirty_guest_regs.is_reserved(Reg::from(j)) {
                            buf.get_inst_mut(i + j as usize).skip = false;
                        }
                    }

                    self.dirty_guest_regs.clear();
                    i += Reg::SPSR as usize - 1;
                }
                BlockInstType::SaveReg(inner) => self.dirty_guest_regs -= inner.guest_reg,
                BlockInstType::RestoreReg(inner) => self.dirty_guest_regs -= inner.guest_reg,
                BlockInstType::MarkRegDirty(inner) => {
                    if inner.dirty {
                        self.dirty_guest_regs += inner.guest_reg;
                    } else {
                        self.dirty_guest_regs -= inner.guest_reg;
                    }
                }
                _ => {
                    let inst = buf.get_inst(i);
                    let (_, outputs) = inst.get_io();
                    self.dirty_guest_regs += outputs.get_guests();
                }
            }
            i += 1;
        }

        let size = self.block_entry_end - self.block_entry_start + 1;
        if size + 1 > self.regs_live_ranges.len() {
            self.regs_live_ranges.resize(size + 1, BlockRegSet::new());
        } else {
            self.regs_live_ranges[size].clear();
        }
        if !self.dirty_guest_regs.is_empty() {
            self.regs_live_ranges[size].add_guests(self.dirty_guest_regs);
            self.regs_live_ranges[size] += tmp_regs.thread_regs_addr_reg;
        }
    }

    pub fn init_resolve_io(&mut self, buf: &BlockAsmBuf) {
        for inst_index in (self.block_entry_start..self.block_entry_end + 1).rev() {
            let i = inst_index - self.block_entry_start;
            unsafe { assert_unchecked(i + 1 < self.regs_live_ranges.len()) };
            let inst = buf.get_inst(inst_index);
            if inst.skip {
                self.regs_live_ranges[i] = self.regs_live_ranges[i + 1];
                continue;
            }
            let (inputs, outputs) = buf.get_inst(inst_index).get_io();
            let mut previous_ranges = self.regs_live_ranges[i + 1];
            previous_ranges -= outputs;
            self.regs_live_ranges[i] = previous_ranges + inputs;
        }
    }

    pub fn remove_dead_code(&mut self, buf: &mut BlockAsmBuf) {
        for (i, inst_index) in (self.block_entry_start..self.block_entry_end + 1).enumerate() {
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

    pub fn set_required_outputs(&mut self, required_outputs: &BlockRegSet) {
        let last = self.block_entry_end - self.block_entry_start + 1;
        unsafe { *self.regs_live_ranges.get_unchecked_mut(last) = *required_outputs }
    }

    pub fn get_required_inputs(&self) -> &BlockRegSet {
        unsafe { self.regs_live_ranges.get_unchecked(0) }
    }

    pub fn get_required_inputs_mut(&mut self) -> &mut BlockRegSet {
        unsafe { self.regs_live_ranges.get_unchecked_mut(0) }
    }

    pub fn get_required_outputs(&self) -> &BlockRegSet {
        let last = self.block_entry_end - self.block_entry_start + 1;
        unsafe { self.regs_live_ranges.get_unchecked(last) }
    }

    fn emit_guest_regs_io(&mut self, tmp_regs: &BlockTmpRegs, buf: &mut BlockAsmBuf, guest_regs: RegReserve, index: usize, save_reg: bool) {
        let mut current_reg = Reg::None;
        let mut reg_length = 0;

        let flush = |current_reg: Reg, reg_length: u8, buf: &mut BlockAsmBuf| {
            if reg_length > 0 && current_reg as u8 - reg_length == 0 {
                let gp_regs = RegReserve::from((1 << (current_reg as u8 + 1)) - 1);
                let mut inst = GuestTransferMultiple {
                    op: if save_reg { TransferOp::Write } else { TransferOp::Read },
                    addr_reg: tmp_regs.thread_regs_addr_reg,
                    addr_out_reg: tmp_regs.thread_regs_addr_reg,
                    gp_regs,
                    fixed_regs: RegReserve::new(),
                    write_back: false,
                    pre: false,
                    add_to_base: true,
                }
                .into();
                buf.reg_allocator.inst_allocate(&inst, &self.regs_live_ranges[index], &mut buf.opcodes);
                inst.emit_opcode(&buf.reg_allocator, &mut buf.opcodes, &mut buf.placeholders);
            } else if reg_length > 2 {
                let start_reg = current_reg as u8 - reg_length;
                let mut guest_regs_mask = (1 << (current_reg as u8 + 1)) - 1;
                guest_regs_mask &= !((1 << start_reg) - 1);
                let gp_regs = RegReserve::from(guest_regs_mask);

                let next_live_range = self.regs_live_ranges[index] + tmp_regs.io_offset_reg;
                let mut base_inst = Alu::alu3(
                    AluOp::Add,
                    [tmp_regs.io_offset_reg.into(), tmp_regs.thread_regs_addr_reg.into(), (start_reg as u32 * 4).into()],
                    AluSetCond::None,
                    false,
                )
                .into();
                buf.reg_allocator.inst_allocate(&base_inst, &next_live_range, &mut buf.opcodes);
                base_inst.emit_opcode(&buf.reg_allocator, &mut buf.opcodes, &mut buf.placeholders);

                let mut inst = GuestTransferMultiple {
                    op: if save_reg { TransferOp::Write } else { TransferOp::Read },
                    addr_reg: tmp_regs.io_offset_reg,
                    addr_out_reg: tmp_regs.io_offset_reg,
                    gp_regs,
                    fixed_regs: RegReserve::new(),
                    write_back: false,
                    pre: false,
                    add_to_base: true,
                }
                .into();
                buf.reg_allocator.inst_allocate(&inst, &self.regs_live_ranges[index], &mut buf.opcodes);
                inst.emit_opcode(&buf.reg_allocator, &mut buf.opcodes, &mut buf.placeholders);
            } else {
                let start_reg = current_reg as u8 - reg_length;
                for i in 0..reg_length + 1 {
                    let guest_reg = Reg::from(start_reg + i);
                    let mut inst = if save_reg {
                        SaveReg {
                            guest_reg,
                            reg_mapped: guest_reg.into(),
                            thread_regs_addr_reg: tmp_regs.thread_regs_addr_reg,
                            tmp_host_cpsr_reg: tmp_regs.host_cpsr_reg,
                        }
                        .into()
                    } else {
                        RestoreReg {
                            guest_reg,
                            reg_mapped: guest_reg.into(),
                            thread_regs_addr_reg: tmp_regs.thread_regs_addr_reg,
                            tmp_guest_cpsr_reg: tmp_regs.guest_cpsr_reg,
                        }
                        .into()
                    };
                    buf.reg_allocator.inst_allocate(&inst, &self.regs_live_ranges[index], &mut buf.opcodes);
                    inst.emit_opcode(&buf.reg_allocator, &mut buf.opcodes, &mut buf.placeholders);
                }
            }
        };

        for reg in guest_regs {
            if reg as u8 == current_reg as u8 + 1 {
                current_reg = reg;
                reg_length += 1;
            } else {
                if current_reg != Reg::None {
                    flush(current_reg, reg_length, buf);
                }
                current_reg = reg;
                reg_length = 0;
            }
        }
        if current_reg != Reg::None {
            flush(current_reg, reg_length, buf);
        }
    }

    fn emit_opcode(&self, inst_index: usize, buf: &mut BlockAsmBuf) {
        let i = inst_index - self.block_entry_start;
        let inst = unsafe { buf.insts.get_unchecked_mut(inst_index) };
        buf.reg_allocator.inst_allocate(inst, unsafe { self.regs_live_ranges.get_unchecked(i + 1) }, &mut buf.opcodes);

        let start_len = buf.opcodes.len();

        #[cfg(debug_assertions)]
        {
            if unsafe { crate::jit::assembler::block_asm::BLOCK_LOG } {
                match &inst.inst_type {
                    BlockInstType::GuestPc(inner) => buf.opcodes_guest_pc_mapping.push((inner.0, start_len)),
                    BlockInstType::Label(inner) => {
                        if let Some(pc) = inner.guest_pc {
                            buf.opcodes_guest_pc_mapping.push((pc, start_len));
                        }
                    }
                    _ => {}
                }
            }
        }

        inst.emit_opcode(&buf.reg_allocator, &mut buf.opcodes, &mut buf.placeholders);
        if inst.cond != Cond::AL {
            for opcode in &mut buf.opcodes[start_len..] {
                *opcode = (*opcode & !(0xF << 28)) | ((inst.cond as u32) << 28);
            }
        }
    }

    pub fn emit_opcodes<const FIRST_BLOCK: bool>(&mut self, tmp_regs: &BlockTmpRegs, buf: &mut BlockAsmBuf, mov_thread_regs_addr_reg_inst: &mut BlockInst) {
        let mut required_inputs = *self.get_required_inputs();
        let needs_thread_regs_addr_reg = required_inputs.contains(tmp_regs.thread_regs_addr_reg);
        let required_guest_regs = required_inputs.get_guests();
        required_inputs.remove_guests(required_guest_regs);
        required_inputs -= tmp_regs.thread_regs_addr_reg;
        buf.reg_allocator.init_inputs(&required_inputs);

        let initialize_guest_regs = |block: &mut Self, buf: &mut BlockAsmBuf, mov_thread_regs_addr_reg_inst: &mut BlockInst, index: usize| {
            if !required_guest_regs.is_empty() || needs_thread_regs_addr_reg {
                let regs_live_range = block.regs_live_ranges[index] + tmp_regs.thread_regs_addr_reg;
                buf.reg_allocator.inst_allocate(mov_thread_regs_addr_reg_inst, &regs_live_range, &mut buf.opcodes);
                mov_thread_regs_addr_reg_inst.emit_opcode(&buf.reg_allocator, &mut buf.opcodes, &mut buf.placeholders);

                debug_assert!(!required_guest_regs.is_reserved(Reg::PC) && !required_guest_regs.is_reserved(Reg::CPSR));
                block.emit_guest_regs_io(tmp_regs, buf, required_guest_regs.get_gp_regs(), index, false);
                for guest_reg in required_guest_regs & (reg_reserve!(Reg::SP, Reg::LR)) {
                    let mut inst = RestoreReg {
                        guest_reg,
                        reg_mapped: guest_reg.into(),
                        thread_regs_addr_reg: tmp_regs.thread_regs_addr_reg,
                        tmp_guest_cpsr_reg: tmp_regs.guest_cpsr_reg,
                    }
                    .into();
                    buf.reg_allocator.inst_allocate(&inst, &regs_live_range, &mut buf.opcodes);
                    inst.emit_opcode(&buf.reg_allocator, &mut buf.opcodes, &mut buf.placeholders);
                }
            }
        };

        if !FIRST_BLOCK {
            initialize_guest_regs(self, buf, mov_thread_regs_addr_reg_inst, 0);
        }

        let mut guest_initialized = false;
        for (i, inst_index) in (self.block_entry_start..self.block_entry_end).enumerate() {
            let inst = unsafe { buf.insts.get_unchecked_mut(inst_index) };
            if inst.skip {
                continue;
            }

            if FIRST_BLOCK && !guest_initialized {
                if let BlockInstType::RestoreReg(inner) = &inst.inst_type {
                    if inner.guest_reg == Reg::CPSR {
                        initialize_guest_regs(self, buf, mov_thread_regs_addr_reg_inst, i);
                        guest_initialized = true;
                    }
                }
            }

            self.emit_opcode(inst_index, buf);
        }

        let last_inst = buf.get_inst(self.block_entry_end);
        let mut last_inst_i = self.block_entry_end - self.block_entry_start;
        let last_inst_is_branch = matches!(&last_inst.inst_type, BlockInstType::Branch(_));
        if !last_inst_is_branch {
            debug_assert!(!last_inst.skip);

            if FIRST_BLOCK && !guest_initialized {
                if let BlockInstType::RestoreReg(inner) = &last_inst.inst_type {
                    if inner.guest_reg == Reg::CPSR {
                        initialize_guest_regs(self, buf, mov_thread_regs_addr_reg_inst, last_inst_i);
                        guest_initialized = true;
                    }
                }
            }

            self.emit_opcode(self.block_entry_end, buf);
            last_inst_i += 1;
        }

        debug_assert!(!FIRST_BLOCK || guest_initialized);

        if !self.dirty_guest_regs.is_empty() {
            self.emit_guest_regs_io(tmp_regs, buf, self.dirty_guest_regs.get_gp_regs(), last_inst_i, true);
            for guest_reg in self.dirty_guest_regs & (reg_reserve!(Reg::SP, Reg::LR, Reg::PC, Reg::CPSR)) {
                let mut inst = SaveReg {
                    guest_reg,
                    reg_mapped: guest_reg.into(),
                    thread_regs_addr_reg: tmp_regs.thread_regs_addr_reg,
                    tmp_host_cpsr_reg: tmp_regs.host_cpsr_reg,
                }
                .into();
                buf.reg_allocator.inst_allocate(&inst, &self.regs_live_ranges[last_inst_i], &mut buf.opcodes);
                inst.emit_opcode(&buf.reg_allocator, &mut buf.opcodes, &mut buf.placeholders);
            }
        }

        if !self.exit_blocks.is_empty() {
            let mut required_outputs = *self.get_required_outputs();
            required_outputs.remove_guests(RegReserve::gp() + Reg::SP + Reg::LR);
            required_outputs -= tmp_regs.thread_regs_addr_reg;

            buf.reg_allocator.ensure_global_mappings(required_outputs, &mut buf.opcodes);
        }

        if last_inst_is_branch {
            self.emit_opcode(self.block_entry_end, buf);
        }
    }
}

impl Debug for BasicBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if !self.io_resolved {
            writeln!(f, "BasicBlock: uninitialized enter blocks: {:?}", self.enter_blocks)?;
            for i in self.block_entry_start..self.block_entry_end + 1 {
                let inst = unsafe { self.block_asm_buf_ptr.as_ref().unwrap().get_inst(i) };
                writeln!(f, "\t{inst:?}")?;
                let (inputs, outputs) = inst.get_io();
                writeln!(f, "\t\tinputs: {inputs:?}, outputs: {outputs:?}")?;
            }
            write!(f, "BasicBlock end exit blocks: {:?}", self.exit_blocks)
        } else {
            writeln!(f, "BasicBlock: inputs: {:?} enter blocks: {:?}", self.regs_live_ranges.first().unwrap(), self.enter_blocks,)?;
            for (i, inst_index) in (self.block_entry_start..self.block_entry_end + 1).enumerate() {
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
