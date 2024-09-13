use crate::core::thread_regs::ThreadRegs;
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::assembler::basic_block::BasicBlock;
use crate::jit::assembler::block_inst::{BlockAluOp, BlockAluSetCond, BlockSystemRegOp, BlockTransferOp, GuestInstInfo, GuestPcInfo};
use crate::jit::assembler::block_reg_allocator::BlockRegAllocator;
use crate::jit::assembler::block_reg_set::{block_reg_set, BlockRegSet};
use crate::jit::assembler::{BlockAsmBuf, BlockInst, BlockLabel, BlockOperand, BlockOperandShift, BlockReg, ANY_REG_LIMIT};
use crate::jit::inst_info::InstInfo;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount};
use crate::utils::{NoHashMap, NoHashSet};

// pub static mut BLOCK_LOG: bool = false;

macro_rules! alu3 {
    ($name:ident, $inst:ident, $set_cond:ident, $thumb_pc_aligned:expr) => {
        pub fn $name(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
            self.add_op3(BlockAluOp::$inst, op0.into(), op1.into(), op2.into(), BlockAluSetCond::$set_cond, $thumb_pc_aligned)
        }
    };
}

macro_rules! alu2_op1 {
    ($name:ident, $inst:ident, $set_cond:ident, $thumb_pc_aligned:expr) => {
        pub fn $name(&mut self, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
            self.add_op2_op1(BlockAluOp::$inst, op1.into(), op2.into(), BlockAluSetCond::$set_cond, $thumb_pc_aligned)
        }
    };
}

macro_rules! alu2_op0 {
    ($name:ident, $inst:ident, $set_cond:ident, $thumb_pc_aligned:expr) => {
        pub fn $name(&mut self, op0: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
            self.add_op2_op0(BlockAluOp::$inst, op0.into(), op2.into(), BlockAluSetCond::$set_cond, $thumb_pc_aligned)
        }
    };
}

pub struct BlockAsm<'a> {
    pub buf: &'a mut BlockAsmBuf,
    any_reg_count: u16,
    freed_any_regs: NoHashSet<u16>,
    label_count: u16,
    used_labels: NoHashSet<u16>,
    pub thread_regs_addr_reg: BlockReg,
    pub tmp_guest_cpsr_reg: BlockReg,
    pub tmp_adjusted_pc_reg: BlockReg,
    tmp_operand_imm_reg: BlockReg,
    tmp_shift_imm_reg: BlockReg,
    tmp_func_call_reg: BlockReg,
    cond_block_end_label: Option<BlockLabel>,
    block_start: usize,
}

impl<'a> BlockAsm<'a> {
    pub fn new(thread_regs: &ThreadRegs, buf: &'a mut BlockAsmBuf) -> Self {
        buf.insts.clear();
        buf.guest_branches_mapping.clear();
        let thread_regs_addr_reg = BlockReg::Any(Reg::SPSR as u16 + 1);
        let tmp_guest_cpsr_reg = BlockReg::Any(Reg::SPSR as u16 + 2);
        let tmp_adjusted_pc_reg = BlockReg::Any(Reg::SPSR as u16 + 3);
        let tmp_operand_imm_reg = BlockReg::Any(Reg::SPSR as u16 + 4);
        let tmp_shift_imm_reg = BlockReg::Any(Reg::SPSR as u16 + 5);
        let tmp_func_call_reg = BlockReg::Any(Reg::SPSR as u16 + 6);
        let mut instance = BlockAsm {
            buf,
            // First couple any regs are reserved for guest mapping
            any_reg_count: Reg::SPSR as u16 + 7,
            freed_any_regs: NoHashSet::default(),
            label_count: 0,
            used_labels: NoHashSet::default(),
            thread_regs_addr_reg,
            tmp_guest_cpsr_reg,
            tmp_adjusted_pc_reg,
            tmp_operand_imm_reg,
            tmp_shift_imm_reg,
            tmp_func_call_reg,
            cond_block_end_label: None,
            block_start: 0,
        };
        instance.transfer_push(BlockReg::Fixed(Reg::SP), reg_reserve!(Reg::LR));
        instance.sub(BlockReg::Fixed(Reg::SP), BlockReg::Fixed(Reg::SP), ANY_REG_LIMIT as u32 * 4); // Reserve for spilled registers
        instance.mov(thread_regs_addr_reg, thread_regs.get_reg_start_addr() as *const _ as u32);
        instance.block_start = instance.buf.insts.len();
        instance
    }

    pub fn new_reg(&mut self) -> BlockReg {
        match self.freed_any_regs.iter().next() {
            None => {
                assert!(self.any_reg_count < ANY_REG_LIMIT);
                let id = self.any_reg_count;
                self.any_reg_count += 1;
                BlockReg::Any(id)
            }
            Some(any_reg) => {
                let any_reg = *any_reg;
                self.freed_any_regs.remove(&any_reg);
                BlockReg::Any(any_reg)
            }
        }
    }

    pub fn free_reg(&mut self, reg: BlockReg) {
        self.freed_any_regs.insert(reg.as_any());
    }

    pub fn new_label(&mut self) -> BlockLabel {
        assert!(self.label_count < u16::MAX);
        let id = self.label_count;
        self.label_count += 1;
        BlockLabel(id)
    }

    alu3!(sub, Sub, None, false);
    alu3!(add, Add, None, false);
    alu3!(bic, Bic, None, false);
    alu3!(orr, Orr, None, false);

    alu2_op1!(cmp, Cmp, Host, false);

    alu2_op0!(mov, Mov, None, false);

    alu3!(ands_guest_thumb_pc_aligned, And, HostGuest, true);
    alu3!(eors_guest_thumb_pc_aligned, Eor, HostGuest, true);
    alu3!(subs_guest_thumb_pc_aligned, Sub, HostGuest, true);
    alu3!(rsbs_guest_thumb_pc_aligned, Rsb, HostGuest, true);
    alu3!(add_guest_thumb_pc_aligned, Add, None, true);
    alu3!(adds_guest_thumb_pc_aligned, Add, HostGuest, true);
    alu3!(adcs_guest_thumb_pc_aligned, Adc, HostGuest, true);
    alu3!(sbcs_guest_thumb_pc_aligned, Sbc, HostGuest, true);
    alu3!(bics_guest_thumb_pc_aligned, Bic, HostGuest, true);
    alu3!(orrs_guest_thumb_pc_aligned, Orr, HostGuest, true);

    alu2_op1!(tst_guest_thumb_pc_aligned, Tst, HostGuest, true);
    alu2_op1!(cmp_guest_thumb_pc_aligned, Cmp, HostGuest, true);
    alu2_op1!(cmp_guest, Cmp, HostGuest, false);
    alu2_op1!(cmn_guest_thumb_pc_aligned, Cmn, HostGuest, true);

    alu2_op0!(movs_guest_thumb_pc_aligned, Mov, HostGuest, true);
    alu2_op0!(mvns_guest_thumb_pc_aligned, Mvn, HostGuest, true);

    fn check_imm_shift_limit(&mut self, operand: &mut BlockOperandShift) {
        if operand.shift.value.needs_reg_for_imm(0x1F) {
            self.mov(self.tmp_shift_imm_reg, operand.shift.value);
            operand.shift.value = self.tmp_shift_imm_reg.into();
        }
    }

    fn add_op3(&mut self, op: BlockAluOp, op0: BlockReg, op1: BlockReg, mut op2: BlockOperandShift, set_cond: BlockAluSetCond, thumb_pc_aligned: bool) {
        if op2.operand.needs_reg_for_imm(0xFF) {
            self.mov(self.tmp_operand_imm_reg, op2.operand);
            op2 = self.tmp_operand_imm_reg.into();
        }
        self.check_imm_shift_limit(&mut op2);
        self.buf.insts.push(BlockInst::Alu3 {
            op,
            operands: [op0.into(), op1.into(), op2],
            set_cond,
            thumb_pc_aligned,
        })
    }

    fn add_op2_op1(&mut self, op: BlockAluOp, op1: BlockReg, mut op2: BlockOperandShift, set_cond: BlockAluSetCond, thumb_pc_aligned: bool) {
        if op2.operand.needs_reg_for_imm(0xFF) {
            self.mov(self.tmp_operand_imm_reg, op2.operand);
            op2 = self.tmp_operand_imm_reg.into();
        }
        self.check_imm_shift_limit(&mut op2);
        self.buf.insts.push(BlockInst::Alu2Op1 {
            op,
            operands: [op1.into(), op2],
            set_cond,
            thumb_pc_aligned,
        })
    }

    fn add_op2_op0(&mut self, op: BlockAluOp, op0: BlockReg, mut op2: BlockOperandShift, set_cond: BlockAluSetCond, thumb_pc_aligned: bool) {
        if op != BlockAluOp::Mov && op2.operand.needs_reg_for_imm(0xFF) {
            self.mov(self.tmp_operand_imm_reg, op2.operand);
            op2 = self.tmp_operand_imm_reg.into();
        }
        self.check_imm_shift_limit(&mut op2);
        self.buf.insts.push(BlockInst::Alu2Op0 {
            op,
            operands: [op0.into(), op2],
            set_cond,
            thumb_pc_aligned,
        })
    }

    pub fn transfer_read(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>, signed: bool, amount: MemoryAmount) {
        self.transfer(BlockTransferOp::Read, op0, op1, op2, signed, amount)
    }

    pub fn transfer_write(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>, signed: bool, amount: MemoryAmount) {
        self.transfer(BlockTransferOp::Write, op0, op1, op2, signed, amount)
    }

    fn transfer(&mut self, op: BlockTransferOp, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>, signed: bool, amount: MemoryAmount) {
        let mut op2 = op2.into();
        if op2.operand.needs_reg_for_imm(0xFFF) {
            self.mov(self.tmp_operand_imm_reg, op2.operand);
            op2 = self.tmp_operand_imm_reg.into();
        }
        self.check_imm_shift_limit(&mut op2);
        self.buf.insts.push(BlockInst::Transfer {
            op,
            operands: [op0.into().into(), op1.into().into(), op2],
            signed,
            amount,
            add_to_base: true,
        });
    }

    pub fn transfer_push(&mut self, operand: impl Into<BlockReg>, regs: RegReserve) {
        self.transfer_write_multiple(operand, regs, true, true, false)
    }

    pub fn transfer_pop(&mut self, operand: impl Into<BlockReg>, regs: RegReserve) {
        self.transfer_read_multiple(operand, regs, true, false, true)
    }

    pub fn transfer_read_multiple(&mut self, operand: impl Into<BlockReg>, regs: RegReserve, write_back: bool, pre: bool, add_to_base: bool) {
        self.buf.insts.push(BlockInst::TransferMultiple {
            op: BlockTransferOp::Read,
            operand: operand.into(),
            regs,
            write_back,
            pre,
            add_to_base,
        })
    }

    pub fn transfer_write_multiple(&mut self, operand: impl Into<BlockReg>, regs: RegReserve, write_back: bool, pre: bool, add_to_base: bool) {
        self.buf.insts.push(BlockInst::TransferMultiple {
            op: BlockTransferOp::Write,
            operand: operand.into(),
            regs,
            write_back,
            pre,
            add_to_base,
        })
    }

    pub fn mrs_cpsr(&mut self, operand: impl Into<BlockReg>) {
        self.buf.insts.push(BlockInst::SystemReg {
            op: BlockSystemRegOp::Mrs,
            operand: operand.into().into(),
        })
    }

    pub fn msr_cpsr(&mut self, operand: impl Into<BlockOperand>) {
        self.buf.insts.push(BlockInst::SystemReg {
            op: BlockSystemRegOp::Msr,
            operand: operand.into(),
        })
    }

    pub fn bfc(&mut self, operand: impl Into<BlockReg>, lsb: u8, width: u8) {
        self.buf.insts.push(BlockInst::Bfc { operand: operand.into(), lsb, width })
    }

    pub fn muls_guest_thumb_pc_aligned(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.buf.insts.push(BlockInst::Mul {
            operands: [op0.into().into(), op1.into().into(), op2.into()],
            set_cond: BlockAluSetCond::HostGuest,
            thumb_pc_aligned: true,
        });
    }

    pub fn label(&mut self, label: BlockLabel) {
        if !self.used_labels.insert(label.0) {
            panic!("{label:?} was already added");
        }
        self.buf.insts.push(BlockInst::Label { label, guest_pc: None })
    }

    pub fn branch(&mut self, label: BlockLabel, cond: Cond) {
        self.buf.insts.push(BlockInst::Branch {
            label,
            cond,
            block_index: 0,
            skip: false,
        })
    }

    pub fn save_context(&mut self) {
        self.buf.insts.push(BlockInst::SaveContext {
            thread_regs_addr_reg: self.thread_regs_addr_reg,
            tmp_guest_cpsr_reg: self.tmp_guest_cpsr_reg,
            regs_to_save: RegReserve::new(),
        });
    }

    pub fn save_reg(&mut self, guest_reg: Reg) {
        self.buf.insts.push(BlockInst::SaveReg {
            guest_reg,
            reg_mapped: BlockReg::from(guest_reg),
            thread_regs_addr_reg: self.thread_regs_addr_reg,
            tmp_guest_cpsr_reg: self.tmp_guest_cpsr_reg,
        });
    }

    pub fn restore_reg(&mut self, guest_reg: Reg) {
        self.buf.insts.push(BlockInst::RestoreReg {
            guest_reg,
            reg_mapped: BlockReg::from(guest_reg),
            thread_regs_addr_reg: self.thread_regs_addr_reg,
            tmp_guest_cpsr_reg: self.tmp_guest_cpsr_reg,
        });
    }

    pub fn breakout(&mut self) {
        self.add(BlockReg::Fixed(Reg::SP), BlockReg::Fixed(Reg::SP), ANY_REG_LIMIT as u32 * 4); // Remove reserve for spilled registers
        self.transfer_pop(BlockReg::Fixed(Reg::SP), reg_reserve!(Reg::PC));
    }

    pub fn call(&mut self, func: *const ()) {
        self.call_internal(func, None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>)
    }

    pub fn call1(&mut self, func: *const (), arg0: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>)
    }

    pub fn call2(&mut self, func: *const (), arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), Some(arg1.into()), None::<BlockOperand>, None::<BlockOperand>)
    }

    pub fn call3(&mut self, func: *const (), arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>, arg2: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), Some(arg1.into()), Some(arg2.into()), None::<BlockOperand>)
    }

    pub fn call4(&mut self, func: *const (), arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>, arg2: impl Into<BlockOperand>, arg3: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), Some(arg1.into()), Some(arg2.into()), Some(arg3.into()))
    }

    fn call_internal(
        &mut self,
        func: *const (),
        arg0: Option<impl Into<BlockOperand>>,
        arg1: Option<impl Into<BlockOperand>>,
        arg2: Option<impl Into<BlockOperand>>,
        arg3: Option<impl Into<BlockOperand>>,
    ) {
        self.mov(self.tmp_func_call_reg, func as u32);

        let mut args = [arg0.map(|arg| arg.into()), arg1.map(|arg| arg.into()), arg2.map(|arg| arg.into()), arg3.map(|arg| arg.into())];
        for (i, arg) in args.iter_mut().enumerate() {
            if let Some(arg) = arg {
                let reg = BlockReg::Fixed(Reg::from(i as u8));
                self.mov(reg, *arg);
                *arg = reg.into();
            }
        }
        self.buf.insts.push(BlockInst::Call {
            func_reg: self.tmp_func_call_reg,
            args: [
                args[0].map(|_| BlockReg::Fixed(Reg::R0)),
                args[1].map(|_| BlockReg::Fixed(Reg::R1)),
                args[2].map(|_| BlockReg::Fixed(Reg::R2)),
                args[3].map(|_| BlockReg::Fixed(Reg::R3)),
            ],
        });

        // self.free_reg(func_reg);
    }

    pub fn bkpt(&mut self, id: u16) {
        self.buf.insts.push(BlockInst::Bkpt(id));
    }

    pub fn guest_pc(&mut self, pc: u32) {
        self.buf.insts.push(BlockInst::GuestPc(GuestPcInfo(pc)));
    }

    pub fn guest_branch(&mut self, cond: Cond, target_pc: u32) {
        let label = self.new_label();
        self.buf.insts.push(BlockInst::Branch {
            label,
            cond,
            block_index: 0,
            skip: false,
        });
        self.buf.guest_branches_mapping.insert(target_pc, label);
    }

    pub fn generic_guest_inst(&mut self, inst_info: &mut InstInfo) {
        self.buf.insts.push(BlockInst::GenericGuestInst {
            inst: GuestInstInfo::new(inst_info),
            regs_mapping: [
                BlockReg::from(Reg::R0),
                BlockReg::from(Reg::R1),
                BlockReg::from(Reg::R2),
                BlockReg::from(Reg::R3),
                BlockReg::from(Reg::R4),
                BlockReg::from(Reg::R5),
                BlockReg::from(Reg::R6),
                BlockReg::from(Reg::R7),
                BlockReg::from(Reg::R8),
                BlockReg::from(Reg::R9),
                BlockReg::from(Reg::R10),
                BlockReg::from(Reg::R11),
                BlockReg::from(Reg::R12),
                BlockReg::from(Reg::SP),
                BlockReg::from(Reg::LR),
                BlockReg::from(Reg::PC),
                BlockReg::from(Reg::CPSR),
                BlockReg::from(Reg::SPSR),
            ],
        });
    }

    pub fn start_cond_block(&mut self, cond: Cond) {
        if cond != Cond::AL {
            let label = self.new_label();
            self.branch(label, !cond);
            self.cond_block_end_label = Some(label);
        }
    }

    pub fn end_cond_block(&mut self) {
        if let Some(label) = self.cond_block_end_label.take() {
            self.label(label);
        }
    }

    fn resolve_guest_regs(&mut self, basic_block_to_resolve: usize, basic_blocks: &mut [BasicBlock]) {
        basic_blocks[basic_block_to_resolve].init_resolve_guest_regs(self);
        let guest_regs_written_to = basic_blocks[basic_block_to_resolve].guest_regs_dirty;
        for exit_block in basic_blocks[basic_block_to_resolve].exit_blocks.clone() {
            basic_blocks[exit_block].guest_regs_dirty += guest_regs_written_to;
            basic_blocks[exit_block].enter_blocks_guest_resolved.insert(basic_block_to_resolve);
        }
    }

    fn resolve_io_in_basic_blocks(basic_block_to_resolve: usize, basic_blocks: &mut [BasicBlock]) {
        basic_blocks[basic_block_to_resolve].init_resolve_io();
        let required_inputs = *basic_blocks[basic_block_to_resolve].get_required_inputs();
        for enter_block in basic_blocks[basic_block_to_resolve].enter_blocks.clone() {
            basic_blocks[enter_block].add_required_outputs(required_inputs);
            basic_blocks[enter_block].exit_blocks_io_resolved.insert(basic_block_to_resolve);
        }
    }

    // Convert guest pc with labels into labels
    fn resolve_labels(&mut self) {
        let mut replace_labels_mapping = NoHashMap::<u16, (BlockLabel, Option<GuestPcInfo>)>::default();

        let mut i = 0;
        let mut previous_label: Option<(BlockLabel, Option<GuestPcInfo>)> = None;
        while i < self.buf.insts.len() {
            if let BlockInst::GuestPc(pc) = &self.buf.insts[i] {
                if let Some(guest_label) = self.buf.guest_branches_mapping.get(pc) {
                    self.buf.insts[i] = BlockInst::Label {
                        label: *guest_label,
                        guest_pc: Some(*pc),
                    };
                }
            }

            if let BlockInst::Label { label, guest_pc } = self.buf.insts[i] {
                if let Some((p_label, p_guest_pc)) = previous_label {
                    let replace_guest_pc = p_guest_pc.or_else(|| guest_pc);
                    replace_labels_mapping.insert(label.0, (p_label, replace_guest_pc));
                    previous_label = Some((p_label, replace_guest_pc));
                    self.buf.insts.remove(i);
                    continue;
                } else {
                    previous_label = Some((label, guest_pc));
                }
            } else {
                previous_label = None
            }
            i += 1;
        }
    }

    fn assemble_basic_blocks<const THUMB: bool>(&mut self, block_start_pc: u32) -> Vec<BasicBlock> {
        self.resolve_labels();

        let mut basic_blocks = Vec::new();
        // The first couple instructions (self.block_start) are for initialization
        // Manually push these to the first block later after initializing live ranges of variables
        let mut basic_block_start = self.block_start;
        let mut basic_block_label_mapping = NoHashMap::<u16, usize>::default();
        let mut last_label = None::<BlockLabel>;
        for i in self.block_start..self.buf.insts.len() {
            let inst = unsafe { self.buf.insts.get_unchecked(i) };
            match inst {
                BlockInst::Label { label, .. } => {
                    if basic_block_start < i {
                        basic_blocks.push(BasicBlock::new(basic_block_start, i - 1));
                        basic_block_start = i;
                        if let Some(last_label) = last_label {
                            basic_block_label_mapping.insert(last_label.0, basic_blocks.len() - 1);
                        }
                    }
                    last_label = Some(*label);
                }
                BlockInst::Branch { .. } => {
                    if basic_block_start <= i {
                        basic_blocks.push(BasicBlock::new(basic_block_start, i));
                        basic_block_start = i + 1;
                        if let Some(last_label) = last_label {
                            basic_block_label_mapping.insert(last_label.0, basic_blocks.len() - 1);
                        }
                        last_label = None;
                    }
                }
                _ => {}
            }
        }

        if basic_block_start < self.buf.insts.len() {
            basic_blocks.push(BasicBlock::new(basic_block_start, self.buf.insts.len() - 1));
            if let Some(last_label) = last_label {
                basic_block_label_mapping.insert(last_label.0, basic_blocks.len() - 1);
            }
        }

        let basic_blocks_len = basic_blocks.len();
        // Link blocks
        for (i, basic_block) in basic_blocks.iter_mut().enumerate() {
            if let BlockInst::Branch { label, cond, block_index, .. } = &mut self.buf.insts[basic_block.end_asm_inst] {
                let labelled_block_index = basic_block_label_mapping.get(&label.0).unwrap();
                basic_block.exit_blocks.insert(*labelled_block_index);
                *block_index = *labelled_block_index;
                if *cond != Cond::AL && i + 1 < basic_blocks_len {
                    basic_block.exit_blocks.insert(i + 1);
                }
            } else if i + 1 < basic_blocks_len {
                basic_block.exit_blocks.insert(i + 1);
            }
        }

        for i in 0..basic_blocks.len() {
            for exit_block in basic_blocks[i].exit_blocks.clone() {
                basic_blocks[exit_block].enter_blocks.insert(i);
            }
        }

        let mut processed_blocks = NoHashSet::default();
        while processed_blocks.len() != basic_blocks.len() {
            for i in 0..basic_blocks.len() {
                if basic_blocks[i].enter_blocks.len() == basic_blocks[i].enter_blocks_guest_resolved.len() && processed_blocks.insert(i) {
                    self.resolve_guest_regs(i, &mut basic_blocks);
                }
            }
        }

        // First block should contain initialization instructions
        let first_basic_block = basic_blocks.first_mut().unwrap();
        first_basic_block.insts.extend_from_slice(&self.buf.insts[..self.block_start]);

        for basic_block in &mut basic_blocks {
            let mut basic_block_start_pc = block_start_pc;
            for i in (0..=basic_block.start_asm_inst).rev() {
                match &self.buf.insts[i] {
                    BlockInst::Label { guest_pc, .. } => {
                        if let Some(pc) = guest_pc {
                            basic_block_start_pc = pc.0;
                            break;
                        }
                    }
                    BlockInst::GuestPc(pc) => {
                        basic_block_start_pc = pc.0;
                        break;
                    }
                    _ => {}
                }
            }
            basic_block.init_insts::<THUMB>(self, basic_block_start_pc);
        }

        processed_blocks.clear();
        while processed_blocks.len() != basic_blocks.len() {
            for i in (0..basic_blocks.len()).rev() {
                if basic_blocks[i].exit_blocks.len() == basic_blocks[i].exit_blocks_io_resolved.len() && processed_blocks.insert(i) {
                    Self::resolve_io_in_basic_blocks(i, &mut basic_blocks);
                }
            }
        }

        // Initialize all guest regs used in block
        let first_basic_block = basic_blocks.first_mut().unwrap();
        let mut loaded_guest_regs = BlockRegSet::new();
        let mut load_guest_regs_insts = Vec::new();
        let mut used_guest_regs = Vec::new();
        for guest_reg in first_basic_block.get_required_inputs().get_guests() {
            loaded_guest_regs += BlockReg::from(guest_reg);
            load_guest_regs_insts.push(BlockInst::Transfer {
                op: BlockTransferOp::Read,
                operands: [guest_reg.into(), self.thread_regs_addr_reg.into(), (guest_reg as u32 * 4).into()],
                signed: false,
                amount: MemoryAmount::Word,
                add_to_base: true,
            });
            used_guest_regs.push(block_reg_set!(Some(BlockReg::from(guest_reg)), Some(self.thread_regs_addr_reg)));
        }

        let regs_live_extensions = vec![first_basic_block.regs_live_ranges[self.block_start]; load_guest_regs_insts.len()];
        first_basic_block.regs_live_ranges.splice(self.block_start..self.block_start, regs_live_extensions);
        first_basic_block.used_regs.splice(self.block_start..self.block_start, used_guest_regs);

        first_basic_block.insts.splice(self.block_start..self.block_start, load_guest_regs_insts);
        for i in 0..self.block_start {
            first_basic_block.regs_live_ranges[i] -= loaded_guest_regs;
        }

        basic_blocks
    }

    pub fn finalize<const THUMB: bool>(mut self, block_start_pc: u32) -> Vec<u32> {
        let mut basic_blocks = self.assemble_basic_blocks::<THUMB>(block_start_pc);

        assert!(basic_blocks.last().unwrap().exit_blocks.is_empty());

        // if unsafe { BLOCK_LOG } {
        //     for basic_block in &basic_blocks {
        //         println!("{basic_block:?}");
        //     }
        // }

        // Extend reg live ranges over all blocks for reg allocation
        self.buf.reg_range_indicies.clear();
        for (i, basic_block) in basic_blocks.iter().enumerate() {
            for reg in basic_block.get_required_inputs().iter() {
                let indices = self.buf.reg_range_indicies.get_mut(&reg);
                match indices {
                    None => {
                        self.buf.reg_range_indicies.insert(reg, (i, 0));
                    }
                    Some((_, end)) => *end = i,
                }
            }
        }
        for (reg, (start, end)) in &self.buf.reg_range_indicies {
            for i in *start..*end {
                for range in &mut basic_blocks[i].regs_live_ranges {
                    *range += *reg;
                }
            }
        }
        for basic_block in &mut basic_blocks {
            basic_block.extend_reg_live_ranges(&mut self);
        }

        // Try to collapse cond blocks into cond opcodes
        for i in 1..basic_blocks.len() {
            if basic_blocks[i].insts.len() == 1 && basic_blocks[i].enter_blocks.len() == 1 && *basic_blocks[i].enter_blocks.iter().next().unwrap() == i - 1 {
                let override_cond = if let BlockInst::Branch { cond, block_index, skip, .. } = basic_blocks[i - 1].insts.last_mut().unwrap() {
                    if *block_index == i + 1 {
                        *skip = true;
                        !*cond
                    } else {
                        Cond::AL
                    }
                } else {
                    Cond::AL
                };
                // override_cond != AL when a block can be collapsed
                basic_blocks[i].cond_block = override_cond;
            }
        }

        let mut global_regs = BlockRegSet::new();
        for basic_block in &basic_blocks {
            global_regs += *basic_block.get_required_inputs();
        }
        let mut reg_allocator = BlockRegAllocator::new(global_regs);

        let mut opcodes = Vec::new();
        let mut branch_placeholders = Vec::new();
        let mut opcodes_offset = Vec::with_capacity(basic_blocks.len());
        for basic_block in basic_blocks {
            opcodes_offset.push(opcodes.len());
            opcodes.extend(basic_block.emit_opcodes(&mut reg_allocator, &mut branch_placeholders, opcodes.len()));
        }

        for branch_placeholder in branch_placeholders {
            let opcode = opcodes[branch_placeholder];
            let cond = Cond::from((opcode >> 28) as u8);
            let block_index = opcode & 0xFFFFFFF;
            let branch_to = opcodes_offset[block_index as usize];
            let diff = branch_to as i32 - branch_placeholder as i32;
            opcodes[branch_placeholder] = B::b(diff - 2, cond);
        }

        opcodes
    }
}
