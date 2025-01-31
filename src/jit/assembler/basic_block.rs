use crate::jit::assembler::block_asm::{BlockTmpRegs, BLOCK_LOG};
use crate::jit::assembler::block_inst::{Alu, AluOp, AluSetCond, BlockInst, BlockInstType, GuestTransferMultiple, SaveReg, TransferOp};
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockAsmBuf, BlockReg};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::Cond;
use crate::IS_DEBUG;
use std::fmt::{Debug, Formatter};
use std::hint::assert_unchecked;
use std::ptr;

pub struct BasicBlock {
    block_asm_buf_ptr: *const BlockAsmBuf,

    pub block_entry_start: usize,
    pub block_entry_end: usize,

    pub guest_regs_resolved: bool,
    pub guest_regs_input_dirty: RegReserve,
    pub guest_regs_output_dirty: RegReserve,

    pub io_resolved: bool,
    pub regs_live_ranges: Vec<BlockRegSet>,

    pub enter_blocks: Vec<usize>,
    pub exit_blocks: Vec<usize>,

    pub start_pc: u32,
}

impl BasicBlock {
    pub fn new() -> Self {
        BasicBlock {
            block_asm_buf_ptr: ptr::null(),

            block_entry_start: 0,
            block_entry_end: 0,

            guest_regs_resolved: false,
            guest_regs_input_dirty: RegReserve::new(),
            guest_regs_output_dirty: RegReserve::new(),

            io_resolved: false,
            regs_live_ranges: Vec::new(),

            enter_blocks: Vec::new(),
            exit_blocks: Vec::with_capacity(2),

            start_pc: 0,
        }
    }

    pub fn init(&mut self, buf: &mut BlockAsmBuf, block_entry_start: usize, block_entry_end: usize) {
        self.block_asm_buf_ptr = buf as _;

        self.block_entry_start = block_entry_start;
        self.block_entry_end = block_entry_end;

        self.guest_regs_resolved = false;
        self.guest_regs_input_dirty = RegReserve::new();
        self.guest_regs_output_dirty = RegReserve::new();

        self.io_resolved = false;

        self.enter_blocks.clear();
        self.exit_blocks.clear();
    }

    pub fn init_guest_regs(&mut self, buf: &mut BlockAsmBuf) {
        self.guest_regs_output_dirty = self.guest_regs_input_dirty;

        let mut i = self.block_entry_start;
        while i <= self.block_entry_end {
            debug_assert!(!buf.get_inst(i).skip);
            match &mut buf.get_inst_mut(i).inst_type {
                BlockInstType::SaveContext(inner) => {
                    inner.guest_regs = self.guest_regs_output_dirty;
                    self.guest_regs_output_dirty.clear();
                    i += Reg::SPSR as usize - 1;
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
            i += 1;
        }
    }

    pub fn init_insts(&mut self, buf: &mut BlockAsmBuf, tmp_regs: &BlockTmpRegs, basic_block_start_pc: u32) {
        self.start_pc = basic_block_start_pc;

        let mut i = self.block_entry_start;
        while i <= self.block_entry_end {
            if let BlockInstType::SaveContext(inner) = &buf.get_inst(i).inst_type {
                let guest_regs = inner.guest_regs;
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
                    if guest_regs.is_reserved(Reg::from(j)) {
                        buf.get_inst_mut(i + j as usize).skip = false;
                    }
                }
                i += Reg::SPSR as usize - 1;
            }
            i += 1;
        }

        let size = self.block_entry_end - self.block_entry_start + 1;
        self.regs_live_ranges.resize(size + 1, BlockRegSet::new());
        self.regs_live_ranges.last_mut().unwrap().clear();
    }

    pub fn init_resolve_io(&mut self, buf: &BlockAsmBuf) {
        for inst_index in (self.block_entry_start..=self.block_entry_end).rev() {
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
        for (i, inst_index) in (self.block_entry_start..=self.block_entry_end).enumerate() {
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

    fn flush_reg_io_consolidation(&mut self, buf: &mut BlockAsmBuf, tmp_regs: &BlockTmpRegs, from_reg: Reg, to_reg: Reg, save: bool, start_i: usize, end_i: usize) {
        for i in start_i..=end_i {
            let inst = buf.get_inst_mut(i);
            if !inst.skip {
                inst.skip = true;
            }
        }
        self.regs_live_ranges[end_i - self.block_entry_start] = self.regs_live_ranges[start_i - self.block_entry_start];
        let mut thread_regs_addr_reg = tmp_regs.thread_regs_addr_reg;
        if from_reg as u8 > 0 {
            thread_regs_addr_reg = tmp_regs.operand_imm_reg;
            let previous_inst = buf.get_inst_mut(end_i - 1);
            *previous_inst = Alu::alu3(
                AluOp::Add,
                [thread_regs_addr_reg.into(), tmp_regs.thread_regs_addr_reg.into(), (from_reg as u32 * 4).into()],
                AluSetCond::None,
                false,
            )
            .into();
            self.regs_live_ranges[end_i - self.block_entry_start - 1] = self.regs_live_ranges[start_i - self.block_entry_start];
            self.regs_live_ranges[end_i - self.block_entry_start] += thread_regs_addr_reg;
        }
        let end_inst = buf.get_inst_mut(end_i);
        let op = if save { TransferOp::Write } else { TransferOp::Read };
        let mut guest_regs = RegReserve::new();
        for reg in from_reg as u8..=to_reg as u8 {
            guest_regs += Reg::from(reg);
        }
        *end_inst = GuestTransferMultiple {
            op,
            addr_reg: thread_regs_addr_reg,
            addr_out_reg: thread_regs_addr_reg,
            gp_regs: guest_regs,
            fixed_regs: RegReserve::new(),
            write_back: false,
            pre: false,
            add_to_base: true,
        }
        .into();
    }

    pub fn consolidate_reg_io(&mut self, buf: &mut BlockAsmBuf, tmp_regs: &BlockTmpRegs) {
        let mut count = 0;
        let mut target_reg = Reg::None;
        let mut target_save = false;
        let mut last_reg = Reg::None;
        let mut was_save = None;
        let mut start_i = 0;

        for i in self.block_entry_start..=self.block_entry_end {
            let inst = buf.get_inst_mut(i);
            if !inst.skip {
                let mut flush = true;
                match &inst.inst_type {
                    BlockInstType::SaveReg(inner) => {
                        if was_save == Some(true) && inner.guest_reg <= Reg::R12 && last_reg as u8 + 1 == inner.guest_reg as u8 {
                            count += 1;
                            flush = false;
                            target_reg = inner.guest_reg;
                            target_save = true;
                        }
                        last_reg = inner.guest_reg;
                        was_save = Some(true);
                    }
                    BlockInstType::RestoreReg(inner) => {
                        if was_save == Some(false) && inner.guest_reg <= Reg::R12 && last_reg as u8 + 1 == inner.guest_reg as u8 {
                            count += 1;
                            flush = false;
                            target_reg = inner.guest_reg;
                            target_save = false;
                        }
                        last_reg = inner.guest_reg;
                        was_save = Some(false);
                    }
                    _ => {
                        last_reg = Reg::None;
                        was_save = None;
                    }
                }
                if flush && count > 0 {
                    self.flush_reg_io_consolidation(buf, tmp_regs, Reg::from(target_reg as u8 - count), target_reg, target_save, start_i, i - 1);
                    count = 0;
                }
                if count == 0 {
                    start_i = i;
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

    pub fn emit_opcodes(&mut self, buf: &mut BlockAsmBuf, block_index: usize) {
        unsafe { assert_unchecked(block_index < buf.placeholders.len()) }
        buf.clear_placeholders_block(block_index);

        buf.reg_allocator.init_inputs(self.get_required_inputs());

        let opcodes_start_len = buf.opcodes.len();

        for (i, inst_index) in (self.block_entry_start..=self.block_entry_end).enumerate() {
            let inst = unsafe { buf.insts.get_unchecked_mut(inst_index) };
            if inst.skip {
                continue;
            }

            buf.reg_allocator.inst_allocate(inst, &self.regs_live_ranges[i..], &mut buf.opcodes);

            let start_len = buf.opcodes.len();

            if IS_DEBUG && unsafe { BLOCK_LOG } {
                match &inst.inst_type {
                    BlockInstType::GuestPc(inner) => {
                        println!("(0x{start_len:x}, 0x{:x}),", inner.0);
                    }
                    BlockInstType::Label(inner) => {
                        if let Some(pc) = inner.guest_pc {
                            println!("(0x{start_len:x}, 0x{pc:x}),");
                        }
                    }
                    _ => {}
                }
            }

            inst.emit_opcode(&buf.reg_allocator, &mut buf.opcodes, start_len - opcodes_start_len, &mut buf.placeholders[block_index]);
            if inst.cond != Cond::AL {
                for opcode in &mut buf.opcodes[start_len..] {
                    *opcode = (*opcode & !(0xF << 28)) | ((inst.cond as u32) << 28);
                }
            }
        }

        if !self.exit_blocks.is_empty() {
            let required_outputs = *self.get_required_outputs();

            // Make sure to restore mapping before a branch
            if let BlockInstType::Branch(_) = &buf.get_inst(self.block_entry_end).inst_type {
                buf.opcodes.pop().unwrap();
                buf.reg_allocator.ensure_global_mappings(required_outputs, &mut buf.opcodes);
                buf.placeholders[block_index].branch.pop().unwrap();

                let inst = unsafe { buf.insts.get_unchecked_mut(self.block_entry_end) };
                let opcode_index = buf.opcodes.len() - opcodes_start_len;
                inst.emit_opcode(&buf.reg_allocator, &mut buf.opcodes, opcode_index, &mut buf.placeholders[block_index]);
                if inst.cond != Cond::AL {
                    let branch_opcode = buf.opcodes.last_mut().unwrap();
                    *branch_opcode = (*branch_opcode & !(0xF << 28)) | ((inst.cond as u32) << 28);
                }
            } else {
                buf.reg_allocator.ensure_global_mappings(required_outputs, &mut buf.opcodes);
            }
        }
    }
}

impl Debug for BasicBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if !self.io_resolved {
            writeln!(f, "BasicBlock: uninitialized enter blocks: {:?}", self.enter_blocks)?;
            for i in self.block_entry_start..=self.block_entry_end {
                let inst = unsafe { self.block_asm_buf_ptr.as_ref().unwrap().get_inst(i) };
                writeln!(f, "\t{inst:?}")?;
                let (inputs, outputs) = inst.get_io();
                writeln!(f, "\t\tinputs: {inputs:?}, outputs: {outputs:?}")?;
            }
            write!(f, "BasicBlock end exit blocks: {:?}", self.exit_blocks)
        } else {
            writeln!(f, "BasicBlock: inputs: {:?} enter blocks: {:?}", self.regs_live_ranges.first().unwrap(), self.enter_blocks,)?;
            for (i, inst_index) in (self.block_entry_start..=self.block_entry_end).enumerate() {
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
