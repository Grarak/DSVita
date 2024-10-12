use crate::bitset::Bitset;
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::assembler::basic_block::BasicBlock;
use crate::jit::assembler::block_inst::{BlockAluOp, BlockAluSetCond, BlockSystemRegOp, BlockTransferOp, BranchEncoding, GuestInstInfo};
use crate::jit::assembler::block_inst_list::BlockInstList;
use crate::jit::assembler::block_reg_allocator::ALLOCATION_REGS;
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BlockAsmBuf, BlockInst, BlockLabel, BlockOperand, BlockOperandShift, BlockReg, ANY_REG_LIMIT};
use crate::jit::inst_info::InstInfo;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, ShiftType};
use crate::utils::{NoHashMap, NoHashSet};
use crate::IS_DEBUG;
use std::intrinsics::unlikely;
use std::slice;

pub static mut BLOCK_LOG: bool = false;

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

    insts_link: BlockInstList,

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

    cond_block_end_label_stack: Vec<BlockLabel>,

    is_common_fun: bool,
    host_sp_ptr: *mut usize,
    block_start: usize,
    inst_insert_index: Option<usize>,
}

impl<'a> BlockAsm<'a> {
    pub fn new(is_common_fun: bool, guest_regs_ptr: *mut u32, host_sp_ptr: *mut usize, buf: &'a mut BlockAsmBuf) -> Self {
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

            insts_link: BlockInstList::new(),

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

            cond_block_end_label_stack: Vec::new(),

            is_common_fun,
            host_sp_ptr,
            block_start: 0,
            inst_insert_index: None,
        };

        instance.buf.insts.push(BlockInst::Prologue);

        // First argument is store_host_sp: bool
        if !is_common_fun {
            instance.cmp(BlockReg::Fixed(Reg::R0), 0);
            instance.start_cond_block(Cond::NE);
            let host_sp_addr_reg = thread_regs_addr_reg;
            instance.mov(host_sp_addr_reg, host_sp_ptr as u32);
            instance.store_u32(BlockReg::Fixed(Reg::SP), host_sp_addr_reg, 0);
            instance.end_cond_block();
        }

        instance.sub(BlockReg::Fixed(Reg::SP), BlockReg::Fixed(Reg::SP), ANY_REG_LIMIT as u32 * 4); // Reserve for spilled registers

        if !is_common_fun {
            instance.mov(thread_regs_addr_reg, guest_regs_ptr as u32);
            instance.restore_reg(Reg::CPSR);
        }

        instance.block_start = instance.buf.insts.len();
        instance
    }

    fn insert_inst(&mut self, inst: BlockInst) {
        match self.inst_insert_index {
            None => self.buf.insts.push(inst),
            Some(index) => {
                self.buf.insts.insert(index, inst);
                self.inst_insert_index = Some(index + 1);
            }
        }
    }

    pub fn set_insert_inst_index(&mut self, index: usize) {
        self.inst_insert_index = Some(index);
    }

    pub fn reset_insert_inst_index(&mut self) {
        self.inst_insert_index = None;
    }

    pub fn new_reg(&mut self) -> BlockReg {
        match self.freed_any_regs.iter().next() {
            None => self.new_reg_no_reserve(),
            Some(any_reg) => {
                let any_reg = *any_reg;
                self.freed_any_regs.remove(&any_reg);
                BlockReg::Any(any_reg)
            }
        }
    }

    pub fn new_reg_no_reserve(&mut self) -> BlockReg {
        debug_assert!(self.any_reg_count < ANY_REG_LIMIT);
        let id = self.any_reg_count;
        self.any_reg_count += 1;
        BlockReg::Any(id)
    }

    pub fn free_reg(&mut self, reg: BlockReg) {
        self.freed_any_regs.insert(reg.as_any());
    }

    pub fn new_label(&mut self) -> BlockLabel {
        debug_assert!(self.label_count < u16::MAX);
        let id = self.label_count;
        self.label_count += 1;
        BlockLabel(id)
    }

    alu3!(and, And, None, false);
    alu3!(sub, Sub, None, false);
    alu3!(add, Add, None, false);
    alu3!(bic, Bic, None, false);
    alu3!(orr, Orr, None, false);

    alu2_op1!(cmp, Cmp, Host, false);
    alu2_op1!(tst, Tst, Host, false);

    alu2_op0!(mov, Mov, None, false);
    alu2_op0!(mvn, Mvn, None, false);

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

    fn check_alu_imm_limit(&mut self, op2: &mut BlockOperandShift) {
        if op2.operand.needs_reg_for_imm(0xFF) {
            let imm = op2.operand.as_imm();
            let lsb_zeros = imm.trailing_zeros() & !0x1;
            if (imm >> lsb_zeros) & !0xFF == 0 {
                *op2 = (imm >> lsb_zeros, ShiftType::Ror, (32 - lsb_zeros) >> 1).into();
            } else {
                self.mov(self.tmp_operand_imm_reg, op2.operand);
                *op2 = self.tmp_operand_imm_reg.into();
            }
        }
    }

    fn add_op3(&mut self, op: BlockAluOp, op0: BlockReg, op1: BlockReg, mut op2: BlockOperandShift, set_cond: BlockAluSetCond, thumb_pc_aligned: bool) {
        self.check_alu_imm_limit(&mut op2);
        self.check_imm_shift_limit(&mut op2);
        self.insert_inst(BlockInst::Alu3 {
            op,
            operands: [op0.into(), op1.into(), op2],
            set_cond,
            thumb_pc_aligned,
        })
    }

    fn add_op2_op1(&mut self, op: BlockAluOp, op1: BlockReg, mut op2: BlockOperandShift, set_cond: BlockAluSetCond, thumb_pc_aligned: bool) {
        self.check_alu_imm_limit(&mut op2);
        self.check_imm_shift_limit(&mut op2);
        self.insert_inst(BlockInst::Alu2Op1 {
            op,
            operands: [op1.into(), op2],
            set_cond,
            thumb_pc_aligned,
        })
    }

    fn add_op2_op0(&mut self, op: BlockAluOp, op0: BlockReg, mut op2: BlockOperandShift, set_cond: BlockAluSetCond, thumb_pc_aligned: bool) {
        if op != BlockAluOp::Mov {
            self.check_alu_imm_limit(&mut op2);
        }
        self.check_imm_shift_limit(&mut op2);
        self.insert_inst(BlockInst::Alu2Op0 {
            op,
            operands: [op0.into(), op2],
            set_cond,
            thumb_pc_aligned,
        })
    }

    pub fn load_u8(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.transfer(BlockTransferOp::Read, op0, op1, op2, false, MemoryAmount::Byte)
    }

    pub fn store_u8(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.transfer(BlockTransferOp::Write, op0, op1, op2, false, MemoryAmount::Byte)
    }

    pub fn load_u16(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.transfer(BlockTransferOp::Read, op0, op1, op2, false, MemoryAmount::Half)
    }

    pub fn store_u16(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.transfer(BlockTransferOp::Write, op0, op1, op2, false, MemoryAmount::Half)
    }

    pub fn load_u32(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.transfer(BlockTransferOp::Read, op0, op1, op2, false, MemoryAmount::Word)
    }

    pub fn store_u32(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.transfer(BlockTransferOp::Write, op0, op1, op2, false, MemoryAmount::Word)
    }

    fn transfer(&mut self, op: BlockTransferOp, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>, signed: bool, amount: MemoryAmount) {
        let mut op2 = op2.into();
        if op2.operand.needs_reg_for_imm(0xFFF) {
            self.mov(self.tmp_operand_imm_reg, op2.operand);
            op2 = self.tmp_operand_imm_reg.into();
        }
        self.check_imm_shift_limit(&mut op2);
        self.insert_inst(BlockInst::Transfer {
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
        self.insert_inst(BlockInst::TransferMultiple {
            op: BlockTransferOp::Read,
            operand: operand.into(),
            regs,
            write_back,
            pre,
            add_to_base,
        })
    }

    pub fn transfer_write_multiple(&mut self, operand: impl Into<BlockReg>, regs: RegReserve, write_back: bool, pre: bool, add_to_base: bool) {
        self.insert_inst(BlockInst::TransferMultiple {
            op: BlockTransferOp::Write,
            operand: operand.into(),
            regs,
            write_back,
            pre,
            add_to_base,
        })
    }

    pub fn mrs_cpsr(&mut self, operand: impl Into<BlockReg>) {
        self.insert_inst(BlockInst::SystemReg {
            op: BlockSystemRegOp::Mrs,
            operand: operand.into().into(),
        })
    }

    pub fn msr_cpsr(&mut self, operand: impl Into<BlockOperand>) {
        self.insert_inst(BlockInst::SystemReg {
            op: BlockSystemRegOp::Msr,
            operand: operand.into(),
        })
    }

    pub fn bfc(&mut self, operand: impl Into<BlockReg>, lsb: u8, width: u8) {
        self.insert_inst(BlockInst::Bfc { operand: operand.into(), lsb, width })
    }

    pub fn bfi(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, lsb: u8, width: u8) {
        self.insert_inst(BlockInst::Bfi {
            operands: [op0.into(), op1.into()],
            lsb,
            width,
        })
    }

    pub fn muls_guest_thumb_pc_aligned(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.insert_inst(BlockInst::Mul {
            operands: [op0.into().into(), op1.into().into(), op2.into()],
            set_cond: BlockAluSetCond::HostGuest,
            thumb_pc_aligned: true,
        });
    }

    pub fn label(&mut self, label: BlockLabel) {
        if !self.used_labels.insert(label.0) {
            panic!("{label:?} was already added");
        }
        self.insert_inst(BlockInst::Label { label, guest_pc: None })
    }

    pub fn branch(&mut self, label: BlockLabel, cond: Cond) {
        self.insert_inst(BlockInst::Branch { label, cond, block_index: 0 })
    }

    pub fn save_context(&mut self) {
        self.insert_inst(BlockInst::SaveContext {
            thread_regs_addr_reg: self.thread_regs_addr_reg,
            tmp_guest_cpsr_reg: self.tmp_guest_cpsr_reg,
        });
    }

    pub fn save_reg(&mut self, guest_reg: Reg) {
        self.insert_inst(BlockInst::SaveReg {
            guest_reg,
            reg_mapped: BlockReg::from(guest_reg),
            thread_regs_addr_reg: self.thread_regs_addr_reg,
            tmp_guest_cpsr_reg: self.tmp_guest_cpsr_reg,
        });
    }

    pub fn restore_reg(&mut self, guest_reg: Reg) {
        self.insert_inst(BlockInst::RestoreReg {
            guest_reg,
            reg_mapped: BlockReg::from(guest_reg),
            thread_regs_addr_reg: self.thread_regs_addr_reg,
            tmp_guest_cpsr_reg: self.tmp_guest_cpsr_reg,
        });
    }

    pub fn epilogue(&mut self) {
        let host_sp_addr_reg = self.thread_regs_addr_reg;
        self.mov(host_sp_addr_reg, self.host_sp_ptr as u32);
        self.load_u32(BlockReg::Fixed(Reg::SP), host_sp_addr_reg, 0);
        self.buf.insts.push(BlockInst::Epilogue { restore_all_regs: true });
    }

    pub fn epilogue_previous_block(&mut self) {
        self.add(BlockReg::Fixed(Reg::SP), BlockReg::Fixed(Reg::SP), ANY_REG_LIMIT as u32 * 4);
        self.buf.insts.push(BlockInst::Epilogue { restore_all_regs: false });
    }

    pub fn call(&mut self, func: impl Into<BlockOperand>) {
        self.call_internal(func, None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>, true)
    }

    pub fn call1(&mut self, func: impl Into<BlockOperand>, arg0: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>, true)
    }

    pub fn call2(&mut self, func: impl Into<BlockOperand>, arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), Some(arg1.into()), None::<BlockOperand>, None::<BlockOperand>, true)
    }

    pub fn call3(&mut self, func: impl Into<BlockOperand>, arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>, arg2: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), Some(arg1.into()), Some(arg2.into()), None::<BlockOperand>, true)
    }

    pub fn call4(&mut self, func: impl Into<BlockOperand>, arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>, arg2: impl Into<BlockOperand>, arg3: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), Some(arg1.into()), Some(arg2.into()), Some(arg3.into()), true)
    }

    pub fn call_no_return(&mut self, func: impl Into<BlockOperand>) {
        self.call_internal(func, None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>, false)
    }

    pub fn call1_no_return(&mut self, func: impl Into<BlockOperand>, arg0: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>, false)
    }

    pub fn call2_no_return(&mut self, func: impl Into<BlockOperand>, arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), Some(arg1.into()), None::<BlockOperand>, None::<BlockOperand>, false)
    }

    pub fn call3_no_return(&mut self, func: impl Into<BlockOperand>, arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>, arg2: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), Some(arg1.into()), Some(arg2.into()), None::<BlockOperand>, false)
    }

    pub fn call4_no_return(&mut self, func: impl Into<BlockOperand>, arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>, arg2: impl Into<BlockOperand>, arg3: impl Into<BlockOperand>) {
        self.call_internal(func, Some(arg0.into()), Some(arg1.into()), Some(arg2.into()), Some(arg3.into()), false)
    }

    fn handle_call_args(
        &mut self,
        arg0: Option<impl Into<BlockOperand>>,
        arg1: Option<impl Into<BlockOperand>>,
        arg2: Option<impl Into<BlockOperand>>,
        arg3: Option<impl Into<BlockOperand>>,
    ) -> [Option<BlockOperand>; 4] {
        let mut args = [arg0.map(|arg| arg.into()), arg1.map(|arg| arg.into()), arg2.map(|arg| arg.into()), arg3.map(|arg| arg.into())];
        for (i, arg) in args.iter_mut().enumerate() {
            if let Some(arg) = arg {
                let reg = BlockReg::Fixed(Reg::from(i as u8));
                self.mov(reg, *arg);
                *arg = reg.into();
            }
        }
        args
    }

    fn call_internal(
        &mut self,
        func: impl Into<BlockOperand>,
        arg0: Option<impl Into<BlockOperand>>,
        arg1: Option<impl Into<BlockOperand>>,
        arg2: Option<impl Into<BlockOperand>>,
        arg3: Option<impl Into<BlockOperand>>,
        has_return: bool,
    ) {
        let args = self.handle_call_args(arg0, arg1, arg2, arg3);
        self.mov(self.tmp_func_call_reg, func.into());
        self.insert_inst(BlockInst::Call {
            func_reg: self.tmp_func_call_reg,
            args: [
                args[0].map(|_| BlockReg::Fixed(Reg::R0)),
                args[1].map(|_| BlockReg::Fixed(Reg::R1)),
                args[2].map(|_| BlockReg::Fixed(Reg::R2)),
                args[3].map(|_| BlockReg::Fixed(Reg::R3)),
            ],
            has_return,
        });
    }

    pub fn call_common(&mut self, offset: usize) {
        self.call_common_internal(offset, None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>, true)
    }

    pub fn call1_common(&mut self, offset: usize, arg0: impl Into<BlockOperand>) {
        self.call_common_internal(offset, Some(arg0.into()), None::<BlockOperand>, None::<BlockOperand>, None::<BlockOperand>, true)
    }

    pub fn call2_common(&mut self, offset: usize, arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>) {
        self.call_common_internal(offset, Some(arg0.into()), Some(arg1.into()), None::<BlockOperand>, None::<BlockOperand>, true)
    }

    pub fn call3_common(&mut self, offset: usize, arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>, arg2: impl Into<BlockOperand>) {
        self.call_common_internal(offset, Some(arg0.into()), Some(arg1.into()), Some(arg2.into()), None::<BlockOperand>, true)
    }

    pub fn call4_common(&mut self, offset: usize, arg0: impl Into<BlockOperand>, arg1: impl Into<BlockOperand>, arg2: impl Into<BlockOperand>, arg3: impl Into<BlockOperand>) {
        self.call_common_internal(offset, Some(arg0.into()), Some(arg1.into()), Some(arg2.into()), Some(arg3.into()), true)
    }

    fn call_common_internal(
        &mut self,
        offset: usize,
        arg0: Option<impl Into<BlockOperand>>,
        arg1: Option<impl Into<BlockOperand>>,
        arg2: Option<impl Into<BlockOperand>>,
        arg3: Option<impl Into<BlockOperand>>,
        has_return: bool,
    ) {
        let args = self.handle_call_args(arg0, arg1, arg2, arg3);
        self.insert_inst(BlockInst::CallCommon {
            mem_offset: offset,
            args: [
                args[0].map(|_| BlockReg::Fixed(Reg::R0)),
                args[1].map(|_| BlockReg::Fixed(Reg::R1)),
                args[2].map(|_| BlockReg::Fixed(Reg::R2)),
                args[3].map(|_| BlockReg::Fixed(Reg::R3)),
            ],
            has_return,
        });
    }

    pub fn bkpt(&mut self, id: u16) {
        self.insert_inst(BlockInst::Bkpt(id));
    }

    pub fn find_guest_pc_inst_index(&self, guest_pc_to_find: u32) -> Option<usize> {
        for (i, inst) in self.buf.insts.iter().enumerate().rev() {
            if let BlockInst::GuestPc(guest_pc) = inst {
                if *guest_pc == guest_pc_to_find {
                    return Some(i);
                }
            }
        }
        None
    }

    pub fn guest_pc(&mut self, pc: u32) {
        self.insert_inst(BlockInst::GuestPc(pc));
    }

    pub fn guest_branch(&mut self, cond: Cond, target_pc: u32) {
        let label = match self.buf.guest_branches_mapping.get(&target_pc) {
            None => {
                let label = self.new_label();
                self.buf.guest_branches_mapping.insert(target_pc, label);
                label
            }
            Some(label) => *label,
        };
        self.insert_inst(BlockInst::Branch { label, cond, block_index: 0 });
    }

    pub fn generic_guest_inst(&mut self, inst_info: &mut InstInfo) {
        self.insert_inst(BlockInst::GenericGuestInst {
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
            self.cond_block_end_label_stack.push(label);
        }
    }

    pub fn end_cond_block(&mut self) {
        if let Some(label) = self.cond_block_end_label_stack.pop() {
            self.label(label);
        }
    }

    // Convert guest pc with labels into labels
    fn resolve_labels(&mut self, label_aliases: &mut NoHashMap<u16, u16>) {
        let mut previous_label: Option<(BlockLabel, Option<u32>)> = None;

        let mut current_node = self.insts_link.root;
        while !current_node.is_null() {
            let i = BlockInstList::deref(current_node).value;
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
                    let replace_guest_pc = p_guest_pc.or(guest_pc);
                    previous_label = Some((p_label, replace_guest_pc));
                    let previous_node = BlockInstList::deref(current_node).previous;
                    let previous_i = BlockInstList::deref(previous_node).value;
                    if let BlockInst::Label { guest_pc, .. } = &mut self.buf.insts[previous_i] {
                        *guest_pc = replace_guest_pc;
                    }
                    self.insts_link.remove_entry(current_node);
                    label_aliases.insert(label.0, p_label.0);
                    current_node = previous_node;
                } else {
                    previous_label = Some((label, guest_pc));
                }
            } else {
                previous_label = None
            }

            current_node = BlockInstList::deref(current_node).next;
        }
    }

    fn resolve_io(&self, basic_blocks: &mut [BasicBlock], required_outputs: BlockRegSet, block_indices: &[usize]) {
        for i in block_indices {
            let basic_block = &mut basic_blocks[*i];
            let sum_required_outputs = *basic_block.get_required_outputs() + required_outputs;
            if sum_required_outputs != *basic_block.get_required_outputs() || !basic_block.io_resolved {
                basic_block.io_resolved = true;
                basic_block.set_required_outputs(sum_required_outputs);
                basic_block.init_resolve_io(self);
                let enter_blocks = unsafe { slice::from_raw_parts(basic_block.enter_blocks.as_ptr(), basic_block.enter_blocks.len()) };
                let required_inputs = *basic_block.get_required_inputs();
                self.resolve_io(basic_blocks, required_inputs, enter_blocks);
            }
        }
    }

    fn resolve_df_ordering(
        already_processed: &mut Bitset<{ 1024 / 32 }>,
        completed: &mut Bitset<{ 1024 / 32 }>,
        basic_blocks: &mut [BasicBlock],
        block_i: usize,
        ordering: &mut [usize],
        ordering_start: &mut usize,
        ordering_end: &mut usize,
    ) {
        *already_processed += block_i;
        let mut cycle = false;
        for &exit_i in &basic_blocks[block_i].exit_blocks {
            if already_processed.contains(exit_i) && !completed.contains(exit_i) {
                cycle = true;
                break;
            }
        }
        if cycle {
            ordering[*ordering_end] = block_i;
            *ordering_end -= 1;
        } else {
            ordering[*ordering_start] = block_i;
            *ordering_start += 1;
        }
        for i in 0..basic_blocks[block_i].exit_blocks.len() {
            let exit_i = basic_blocks[block_i].exit_blocks[i];
            if !already_processed.contains(exit_i) {
                Self::resolve_df_ordering(already_processed, completed, basic_blocks, exit_i, ordering, ordering_start, ordering_end);
            }
        }
        *completed += block_i;
    }

    fn assemble_basic_blocks(&mut self, block_start_pc: u32, thumb: bool) -> (Vec<BasicBlock>, Vec<usize>) {
        for i in 0..self.buf.insts.len() {
            self.insts_link.insert_end(i);
        }
        let mut label_aliases = NoHashMap::default();
        self.resolve_labels(&mut label_aliases);

        let mut basic_blocks = Vec::new();
        let mut basic_block_label_mapping = NoHashMap::<u16, usize>::default();
        let mut last_label = None::<BlockLabel>;
        let mut basic_block_start = self.insts_link.root;

        let mut current_node = basic_block_start;
        while !current_node.is_null() {
            let i = BlockInstList::deref(current_node).value;

            match &mut self.buf.insts[i] {
                BlockInst::Label { label, .. } => {
                    let label = *label;
                    // block_start_entry can be the same as current_node, when the previous iteration ended with a branch
                    if basic_block_start != current_node {
                        basic_blocks.push(BasicBlock::new(self, basic_block_start, BlockInstList::deref(current_node).previous));
                        basic_block_start = current_node;
                        if let Some(last_label) = last_label {
                            basic_block_label_mapping.insert(last_label.0, basic_blocks.len() - 1);
                        }
                    }
                    last_label = Some(label);
                }
                BlockInst::Branch { label, .. } => {
                    if let Some(alias) = label_aliases.get(&label.0) {
                        label.0 = *alias;
                    }
                    basic_blocks.push(BasicBlock::new(self, basic_block_start, current_node));
                    basic_block_start = BlockInstList::deref(current_node).next;
                    if let Some(last_label) = last_label {
                        basic_block_label_mapping.insert(last_label.0, basic_blocks.len() - 1);
                    }
                    last_label = None;
                }
                BlockInst::Call { has_return: false, .. } | BlockInst::Epilogue { .. } => {
                    basic_blocks.push(BasicBlock::new(self, basic_block_start, current_node));
                    basic_block_start = BlockInstList::deref(current_node).next;
                    if let Some(last_label) = last_label {
                        basic_block_label_mapping.insert(last_label.0, basic_blocks.len() - 1);
                    }
                    last_label = None;
                }
                _ => {}
            }

            current_node = BlockInstList::deref(current_node).next;
        }

        if !basic_block_start.is_null() {
            basic_blocks.push(BasicBlock::new(self, basic_block_start, self.insts_link.end));
            if let Some(last_label) = last_label {
                basic_block_label_mapping.insert(last_label.0, basic_blocks.len() - 1);
            }
        }

        let basic_blocks_len = basic_blocks.len();
        // Link blocks
        for (i, basic_block) in basic_blocks.iter_mut().enumerate() {
            let last_inst_index = BlockInstList::deref(basic_block.block_entry_end).value;
            match &mut self.buf.insts[last_inst_index] {
                BlockInst::Branch { label, cond, block_index, .. } => {
                    let labelled_block_index = basic_block_label_mapping.get(&label.0).unwrap();
                    basic_block.exit_blocks.push(*labelled_block_index);
                    *block_index = *labelled_block_index;
                    if *cond != Cond::AL && i + 1 < basic_blocks_len {
                        basic_block.exit_blocks.push(i + 1);
                    }
                }
                // Don't add exit when last command in basic block is a breakout
                BlockInst::Call { has_return: false, .. } | BlockInst::Epilogue { .. } => {
                    if i + 1 < basic_blocks_len {
                        basic_block.exit_blocks.push(i + 1);
                    }
                }
                _ => {
                    if i + 1 < basic_blocks_len {
                        basic_block.exit_blocks.push(i + 1);
                    }
                }
            }
        }

        for i in 0..basic_blocks.len() {
            let exit_blocks = &basic_blocks[i].exit_blocks;
            debug_assert!(exit_blocks.len() <= 1 || (exit_blocks.len() == 2 && exit_blocks[0] != exit_blocks[1]));
            for j in 0..exit_blocks.len() {
                let exit_block = basic_blocks[i].exit_blocks[j];
                if !basic_blocks[exit_block].enter_blocks.contains(&i) {
                    basic_blocks[exit_block].enter_blocks.push(i);
                }
            }
        }

        for basic_block in &mut basic_blocks {
            let mut basic_block_start_pc = block_start_pc;
            let mut current_node = basic_block.block_entry_start;
            while !current_node.is_null() {
                match &self.buf.insts[BlockInstList::deref(current_node).value] {
                    BlockInst::Label { guest_pc: Some(pc), .. } => {
                        basic_block_start_pc = *pc;
                        break;
                    }
                    BlockInst::GuestPc(pc) => {
                        basic_block_start_pc = *pc;
                        break;
                    }
                    _ => {}
                }

                current_node = BlockInstList::deref(current_node).previous;
            }
            basic_block.init_insts(self, basic_block_start_pc, thumb);
        }

        for i in (0..basic_blocks_len).rev() {
            if basic_blocks[i].exit_blocks.is_empty() {
                self.resolve_io(&mut basic_blocks, BlockRegSet::new(), &[i]);
            }
        }

        for basic_block in &mut basic_blocks {
            basic_block.remove_dead_code(self);
        }

        let mut df_already_processed = Bitset::<{ 1024 / 32 }>::new();
        let mut df_completed = Bitset::<{ 1024 / 32 }>::new();
        let mut df_ordering = vec![0; basic_blocks_len];
        let mut df_ordering_start = 0;
        let mut df_ordering_end = basic_blocks_len - 1;
        Self::resolve_df_ordering(
            &mut df_already_processed,
            &mut df_completed,
            &mut basic_blocks,
            0,
            &mut df_ordering,
            &mut df_ordering_start,
            &mut df_ordering_end,
        );

        (basic_blocks, df_ordering)
    }

    fn assemble_intervals(basic_blocks: &[BasicBlock], ordering: &[usize]) -> NoHashMap<u16, (usize, usize)> {
        let mut intervals = NoHashMap::default();

        let mut processed_regs = BlockRegSet::new();
        for i in 0..ordering.len() {
            let block_i = ordering[i];
            let outputs = *basic_blocks[block_i].get_required_outputs();
            for reg in (outputs - processed_regs).iter_any() {
                let mut end_j = 0;
                for j in (i + 1..ordering.len()).rev() {
                    let block_j = ordering[j];
                    if basic_blocks[block_j].get_required_inputs().contains(BlockReg::Any(reg)) {
                        end_j = j;
                        break;
                    }
                }
                debug_assert!(i < end_j);
                intervals.insert(reg, (i, end_j));
            }
            processed_regs += outputs;
        }

        intervals
    }

    pub fn emit_opcodes(&mut self, block_start_pc: u32, thumb: bool) -> usize {
        let (mut basic_blocks, basic_blocks_order) = self.assemble_basic_blocks(block_start_pc, thumb);

        if IS_DEBUG && unsafe { BLOCK_LOG } {
            for (i, basic_block) in basic_blocks.iter().enumerate() {
                println!("{i}[{i} {:x}]", basic_block.start_pc);
            }

            for (i, basic_block) in basic_blocks.iter().enumerate() {
                for &exit_i in &basic_block.exit_blocks {
                    println!("{i} --> {exit_i}");
                }
            }

            for (i, basic_block) in basic_blocks.iter().enumerate() {
                println!("{i}: {basic_block:?}");
            }
        }

        let mut reg_intervals = Self::assemble_intervals(&basic_blocks, &basic_blocks_order);

        if IS_DEBUG && unsafe { BLOCK_LOG } {
            println!("reg intervals {reg_intervals:?} ");
            println!("block ordering {basic_blocks_order:?}");
        }

        self.buf.reg_allocator.dirty_regs.clear();
        self.buf.reg_allocator.global_mapping.clear();
        let mut free_regs = ALLOCATION_REGS;
        while !free_regs.is_empty() {
            let mut longest_interval = 0;
            let mut longest_interval_reg = 0;
            for (&reg, &(start, end)) in &reg_intervals {
                let range = end - start;
                if range > longest_interval {
                    longest_interval = range;
                    longest_interval_reg = reg;
                }
            }
            reg_intervals.remove(&longest_interval_reg);
            self.buf.reg_allocator.global_mapping.insert(longest_interval_reg, free_regs.pop().unwrap());
        }

        for (reg, _) in reg_intervals {
            self.buf.reg_allocator.global_mapping.insert(reg, Reg::None);
        }

        for (i, basic_block) in basic_blocks.iter_mut().enumerate() {
            if unlikely(i != 0 && basic_block.enter_blocks.is_empty()) {
                continue;
            }
            self.buf.reg_allocator.init_inputs(*basic_block.get_required_inputs());
            basic_block.allocate_regs(self);
        }

        // Used to determine what regs to push and pop for prologue and epilogue
        let mut used_host_regs = if unlikely(self.is_common_fun) {
            self.buf.reg_allocator.dirty_regs & ALLOCATION_REGS
        } else {
            ALLOCATION_REGS
        };
        // We need an even amount of registers for 8 byte alignment, in case the compiler decides to use neon instructions, epilogue adds Reg::LR
        if used_host_regs.len() & 1 == 0 {
            used_host_regs += Reg::R12;
        }

        self.buf.opcodes.clear();
        self.buf.block_opcode_offsets.clear();
        self.buf.branch_placeholders.clear();

        for (i, basic_block) in basic_blocks.iter().enumerate() {
            let opcodes_len = self.buf.opcodes.len();
            self.buf.block_opcode_offsets.push(opcodes_len);
            if unlikely(i != 0 && basic_block.enter_blocks.is_empty()) {
                continue;
            }
            let opcodes = basic_block.emit_opcodes(self, opcodes_len, used_host_regs);
            self.buf.opcodes.extend(opcodes);
        }

        self.buf.opcodes.len()
    }

    pub fn finalize(&mut self, jit_mem_offset: usize) -> &Vec<u32> {
        for &branch_placeholder in &self.buf.branch_placeholders {
            let encoding = BranchEncoding::from(self.buf.opcodes[branch_placeholder]);
            let diff = if encoding.is_call_common() {
                let opcode_index = (jit_mem_offset >> 2) + branch_placeholder;
                let branch_to = u32::from(encoding.index()) >> 2;
                branch_to as i32 - opcode_index as i32
            } else {
                let block_index = u32::from(encoding.index());
                let branch_to = self.buf.block_opcode_offsets[block_index as usize];
                branch_to as i32 - branch_placeholder as i32
            };
            self.buf.opcodes[branch_placeholder] = if encoding.has_return() { B::bl } else { B::b }(diff - 2, Cond::from(u8::from(encoding.cond())));
        }

        &self.buf.opcodes
    }
}
