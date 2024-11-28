use crate::jit::assembler::arm::alu_assembler::AluShiftImm;
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::assembler::basic_block::BasicBlock;
use crate::jit::assembler::block_inst::{BlockAluOp, BlockAluSetCond, BlockInst, BlockSystemRegOp, BlockTransferOp, BranchEncoding, GuestInstInfo};
use crate::jit::assembler::block_inst_list::BlockInstList;
use crate::jit::assembler::block_reg_allocator::ALLOCATION_REGS;
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{block_inst_list, BlockAsmBuf, BlockInstKind, BlockLabel, BlockOperand, BlockOperandShift, BlockReg, BlockShift, ANY_REG_LIMIT};
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
    pub tmp_operand_imm_reg: BlockReg,
    tmp_shift_imm_reg: BlockReg,
    tmp_func_call_reg: BlockReg,

    cond_block_end_label_stack: Vec<(BlockLabel, Cond, usize)>,

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
        let tmp_operand_imm_reg = BlockReg::Any(Reg::SPSR as u16 + 3);
        let tmp_shift_imm_reg = BlockReg::Any(Reg::SPSR as u16 + 4);
        let tmp_func_call_reg = BlockReg::Any(Reg::SPSR as u16 + 5);
        let mut instance = BlockAsm {
            buf,

            insts_link: BlockInstList::new(),

            // First couple any regs are reserved for guest mapping
            any_reg_count: Reg::SPSR as u16 + 6,
            freed_any_regs: NoHashSet::default(),
            label_count: 0,
            used_labels: NoHashSet::default(),

            thread_regs_addr_reg,
            tmp_guest_cpsr_reg,
            tmp_operand_imm_reg,
            tmp_shift_imm_reg,
            tmp_func_call_reg,

            cond_block_end_label_stack: Vec::new(),

            is_common_fun,
            host_sp_ptr,
            block_start: 0,
            inst_insert_index: None,
        };

        instance.insert_inst(BlockInstKind::Prologue);

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
            for guest_reg in RegReserve::gp() + Reg::SP + Reg::LR {
                instance.restore_reg(guest_reg);
            }
            instance.restore_reg(Reg::CPSR);
        }

        instance.block_start = instance.buf.insts.len();
        instance
    }

    fn insert_inst(&mut self, inst: impl Into<BlockInst>) {
        match self.inst_insert_index {
            None => self.buf.insts.push(inst.into()),
            Some(index) => {
                self.buf.insts.insert(index, inst.into());
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

    fn check_alu_imm_limit(&mut self, op2: &mut BlockOperandShift, emit_mov: bool) {
        if op2.operand.needs_reg_for_imm(0xFF) {
            debug_assert_eq!(op2.shift, BlockShift::default());
            let imm = op2.operand.as_imm();

            let msb_ones = (imm.leading_ones() + 0x1) & !0x1;
            if msb_ones != 0 {
                let ror_imm = (imm << msb_ones) | (imm >> (32 - msb_ones));
                if ror_imm & !0xFF == 0 {
                    *op2 = (ror_imm, ShiftType::Ror, msb_ones >> 1).into();
                    return;
                }
            }

            let lsb_zeros = imm.trailing_zeros() & !0x1;
            if (imm >> lsb_zeros) & !0xFF == 0 {
                *op2 = (imm >> lsb_zeros, ShiftType::Ror, (32 - lsb_zeros) >> 1).into();
            } else if emit_mov {
                self.mov(self.tmp_operand_imm_reg, op2.operand);
                *op2 = self.tmp_operand_imm_reg.into();
            }
        }
    }

    fn add_op3(&mut self, op: BlockAluOp, op0: BlockReg, op1: BlockReg, mut op2: BlockOperandShift, set_cond: BlockAluSetCond, thumb_pc_aligned: bool) {
        self.check_alu_imm_limit(&mut op2, true);
        self.check_imm_shift_limit(&mut op2);
        self.insert_inst(BlockInstKind::Alu3 {
            op,
            operands: [op0.into(), op1.into(), op2],
            set_cond,
            thumb_pc_aligned,
        })
    }

    fn add_op2_op1(&mut self, op: BlockAluOp, op1: BlockReg, mut op2: BlockOperandShift, set_cond: BlockAluSetCond, thumb_pc_aligned: bool) {
        self.check_alu_imm_limit(&mut op2, true);
        self.check_imm_shift_limit(&mut op2);
        self.insert_inst(BlockInstKind::Alu2Op1 {
            op,
            operands: [op1.into(), op2],
            set_cond,
            thumb_pc_aligned,
        })
    }

    fn add_op2_op0(&mut self, op: BlockAluOp, op0: BlockReg, mut op2: BlockOperandShift, set_cond: BlockAluSetCond, thumb_pc_aligned: bool) {
        self.check_alu_imm_limit(&mut op2, op != BlockAluOp::Mov);
        self.check_imm_shift_limit(&mut op2);
        self.insert_inst(BlockInstKind::Alu2Op0 {
            op,
            operands: [op0.into(), op2],
            set_cond,
            thumb_pc_aligned,
        })
    }

    pub fn load_u8(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.transfer_read(op0, op1, op2, false, MemoryAmount::Byte)
    }

    pub fn store_u8(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.transfer_write(op0, op1, op2, false, MemoryAmount::Byte)
    }

    pub fn load_u16(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.transfer_read(op0, op1, op2, false, MemoryAmount::Half)
    }

    pub fn store_u16(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.transfer_write(op0, op1, op2, false, MemoryAmount::Half)
    }

    pub fn load_u32(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.transfer_read(op0, op1, op2, false, MemoryAmount::Word)
    }

    pub fn store_u32(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.transfer_write(op0, op1, op2, false, MemoryAmount::Word)
    }

    pub fn transfer_read(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>, signed: bool, amount: MemoryAmount) {
        self.transfer(BlockTransferOp::Read, op0, op1, op2, signed, amount)
    }

    pub fn transfer_write(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>, signed: bool, amount: MemoryAmount) {
        self.transfer(BlockTransferOp::Write, op0, op1, op2, signed, amount)
    }

    pub fn transfer(&mut self, op: BlockTransferOp, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>, signed: bool, amount: MemoryAmount) {
        let mut op2 = op2.into();
        if op2.operand.needs_reg_for_imm(0xFFF) {
            self.mov(self.tmp_operand_imm_reg, op2.operand);
            op2 = self.tmp_operand_imm_reg.into();
        }
        self.check_imm_shift_limit(&mut op2);
        self.insert_inst(BlockInstKind::Transfer {
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
        self.insert_inst(BlockInstKind::TransferMultiple {
            op: BlockTransferOp::Read,
            operand: operand.into(),
            regs,
            write_back,
            pre,
            add_to_base,
        })
    }

    pub fn transfer_write_multiple(&mut self, operand: impl Into<BlockReg>, regs: RegReserve, write_back: bool, pre: bool, add_to_base: bool) {
        self.insert_inst(BlockInstKind::TransferMultiple {
            op: BlockTransferOp::Write,
            operand: operand.into(),
            regs,
            write_back,
            pre,
            add_to_base,
        })
    }

    pub fn guest_transfer_read_multiple(
        &mut self,
        addr_reg: impl Into<BlockReg>,
        addr_out_reg: impl Into<BlockReg>,
        gp_regs: RegReserve,
        fixed_regs: RegReserve,
        write_back: bool,
        pre: bool,
        add_to_base: bool,
    ) {
        self.insert_inst(BlockInstKind::GuestTransferMultiple {
            op: BlockTransferOp::Read,
            addr_reg: addr_reg.into(),
            addr_out_reg: addr_out_reg.into(),
            gp_regs,
            fixed_regs,
            write_back,
            pre,
            add_to_base,
        })
    }

    pub fn guest_transfer_write_multiple(
        &mut self,
        addr_reg: impl Into<BlockReg>,
        addr_out_reg: impl Into<BlockReg>,
        gp_regs: RegReserve,
        fixed_regs: RegReserve,
        write_back: bool,
        pre: bool,
        add_to_base: bool,
    ) {
        self.insert_inst(BlockInstKind::GuestTransferMultiple {
            op: BlockTransferOp::Write,
            addr_reg: addr_reg.into(),
            addr_out_reg: addr_out_reg.into(),
            gp_regs,
            fixed_regs,
            write_back,
            pre,
            add_to_base,
        })
    }

    pub fn mrs_cpsr(&mut self, operand: impl Into<BlockReg>) {
        self.insert_inst(BlockInstKind::SystemReg {
            op: BlockSystemRegOp::Mrs,
            operand: operand.into().into(),
        })
    }

    pub fn msr_cpsr(&mut self, operand: impl Into<BlockOperand>) {
        self.insert_inst(BlockInstKind::SystemReg {
            op: BlockSystemRegOp::Msr,
            operand: operand.into(),
        })
    }

    pub fn bfc(&mut self, operand: impl Into<BlockReg>, lsb: u8, width: u8) {
        self.insert_inst(BlockInstKind::Bfc { operand: operand.into(), lsb, width })
    }

    pub fn bfi(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, lsb: u8, width: u8) {
        self.insert_inst(BlockInstKind::Bfi {
            operands: [op0.into(), op1.into()],
            lsb,
            width,
        });
    }

    pub fn ubfx(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, lsb: u8, width: u8) {
        self.insert_inst(BlockInstKind::Ubfx {
            operands: [op0.into(), op1.into()],
            lsb,
            width,
        });
    }

    pub fn muls_guest_thumb_pc_aligned(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.insert_inst(BlockInstKind::Mul {
            operands: [op0.into().into(), op1.into().into(), op2.into()],
            set_cond: BlockAluSetCond::HostGuest,
            thumb_pc_aligned: true,
        });
    }

    pub fn label(&mut self, label: BlockLabel) {
        if !self.used_labels.insert(label.0) {
            panic!("{label:?} was already added");
        }
        self.insert_inst(BlockInstKind::Label { label, guest_pc: None })
    }

    pub fn branch(&mut self, label: BlockLabel, cond: Cond) {
        self.insert_inst(BlockInst::new(
            cond,
            BlockInstKind::Branch {
                label,
                block_index: 0,
                fallthrough: false,
            },
        ))
    }

    pub fn branch_fallthrough(&mut self, label: BlockLabel, cond: Cond) {
        self.insert_inst(BlockInst::new(
            cond,
            BlockInstKind::Branch {
                label,
                block_index: 0,
                fallthrough: true,
            },
        ))
    }

    pub fn save_context(&mut self) {
        self.insert_inst(BlockInstKind::SaveContext {
            guest_regs: RegReserve::new(),
            thread_regs_addr_reg: self.thread_regs_addr_reg,
        });
    }

    pub fn save_reg(&mut self, guest_reg: Reg) {
        self.insert_inst(BlockInstKind::SaveReg {
            guest_reg,
            reg_mapped: BlockReg::from(guest_reg),
            thread_regs_addr_reg: self.thread_regs_addr_reg,
        });
    }

    pub fn restore_reg(&mut self, guest_reg: Reg) {
        self.insert_inst(BlockInstKind::RestoreReg {
            guest_reg,
            reg_mapped: BlockReg::from(guest_reg),
            thread_regs_addr_reg: self.thread_regs_addr_reg,
            tmp_guest_cpsr_reg: self.tmp_guest_cpsr_reg,
        });
    }

    pub fn mark_reg_dirty(&mut self, guest_reg: Reg, dirty: bool) {
        self.insert_inst(BlockInstKind::MarkRegDirty { guest_reg, dirty });
    }

    pub fn epilogue(&mut self) {
        let host_sp_addr_reg = self.thread_regs_addr_reg;
        self.mov(host_sp_addr_reg, self.host_sp_ptr as u32);
        self.load_u32(BlockReg::Fixed(Reg::SP), host_sp_addr_reg, 0);
        self.insert_inst(BlockInstKind::Epilogue { restore_all_regs: true });
    }

    pub fn epilogue_previous_block(&mut self) {
        self.add(BlockReg::Fixed(Reg::SP), BlockReg::Fixed(Reg::SP), ANY_REG_LIMIT as u32 * 4);
        self.insert_inst(BlockInstKind::Epilogue { restore_all_regs: false });
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
        self.insert_inst(BlockInstKind::Call {
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
        self.insert_inst(BlockInstKind::CallCommon {
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
        self.insert_inst(BlockInstKind::Bkpt(id));
    }

    pub fn nop(&mut self) {
        self.insert_inst(BlockInstKind::Nop);
    }

    pub fn find_guest_pc_inst_index(&self, guest_pc_to_find: u32) -> Option<usize> {
        for (i, inst) in self.buf.insts.iter().enumerate().rev() {
            if let BlockInstKind::GuestPc(guest_pc) = &inst.kind {
                if *guest_pc == guest_pc_to_find {
                    return Some(i);
                }
            }
        }
        None
    }

    pub fn guest_pc(&mut self, pc: u32) {
        self.insert_inst(BlockInstKind::GuestPc(pc));
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
        self.insert_inst(BlockInst::new(
            cond,
            BlockInstKind::Branch {
                label,
                block_index: 0,
                fallthrough: false,
            },
        ));
    }

    pub fn generic_guest_inst(&mut self, inst_info: &mut InstInfo) {
        self.insert_inst(BlockInstKind::GenericGuestInst {
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
            self.cond_block_end_label_stack.push((label, cond, self.buf.insts.len()));
        }
    }

    pub fn end_cond_block(&mut self) {
        if let Some((label, cond, start_index)) = self.cond_block_end_label_stack.pop() {
            let cond_block_size = self.buf.insts.len() - start_index;
            let (_, outputs) = self.buf.insts[start_index].get_io();
            if cond_block_size == 1 && self.buf.insts[start_index].cond == Cond::AL && !outputs.contains(BlockReg::Fixed(Reg::CPSR)) && !outputs.contains(Reg::CPSR.into()) {
                self.buf.insts[start_index].cond = cond;
                self.buf.insts[start_index].invalidate_io_cache();
                // Remove the branch
                self.buf.insts.remove(start_index - 1);
            } else {
                self.label(label);
            }
        }
    }

    // Convert guest pc with labels into labels
    fn resolve_labels(&mut self, label_aliases: &mut NoHashMap<u16, u16>) {
        let mut previous_label: Option<(BlockLabel, Option<u32>)> = None;

        let mut current_node = self.insts_link.root;
        while !current_node.is_null() {
            let i = BlockInstList::deref(current_node).value;
            if let BlockInstKind::GuestPc(pc) = &self.buf.insts[i].kind {
                if let Some(guest_label) = self.buf.guest_branches_mapping.get(pc) {
                    self.buf.insts[i] = BlockInstKind::Label {
                        label: *guest_label,
                        guest_pc: Some(*pc),
                    }
                    .into();
                }
            }

            if let BlockInstKind::Label { label, guest_pc } = &self.buf.insts[i].kind {
                if let Some((p_label, p_guest_pc)) = previous_label {
                    let replace_guest_pc = p_guest_pc.or(*guest_pc);
                    previous_label = Some((p_label, replace_guest_pc));
                    let previous_node = BlockInstList::deref(current_node).previous;
                    let previous_i = BlockInstList::deref(previous_node).value;
                    self.insts_link.remove_entry(current_node);
                    label_aliases.insert(label.0, p_label.0);
                    current_node = previous_node;
                    if let BlockInstKind::Label { guest_pc, .. } = &mut self.buf.insts[previous_i].kind {
                        *guest_pc = replace_guest_pc;
                    }
                } else {
                    previous_label = Some((*label, *guest_pc));
                }
            } else {
                previous_label = None
            }

            current_node = BlockInstList::deref(current_node).next;
        }
    }

    fn resolve_guest_regs(&mut self, basic_blocks: &mut [BasicBlock], guest_regs_dirty: RegReserve, block_indices: &[usize], reachable_blocks: &mut NoHashSet<usize>) {
        for &i in block_indices {
            let basic_block = &mut basic_blocks[i];
            let sum_guest_regs_input_dirty = basic_block.guest_regs_input_dirty + guest_regs_dirty;
            if sum_guest_regs_input_dirty != basic_block.guest_regs_input_dirty || !basic_block.guest_regs_resolved {
                reachable_blocks.insert(i);
                basic_block.guest_regs_resolved = true;
                basic_block.guest_regs_input_dirty = sum_guest_regs_input_dirty;
                basic_block.init_guest_regs(self);
                let exit_blocks = unsafe { slice::from_raw_parts(basic_block.exit_blocks.as_ptr(), basic_block.exit_blocks.len()) };
                let guest_regs_dirty = basic_block.guest_regs_output_dirty;
                self.resolve_guest_regs(basic_blocks, guest_regs_dirty, exit_blocks, reachable_blocks);
            }
        }
    }

    fn resolve_io(&self, basic_blocks: &mut [BasicBlock], required_outputs: BlockRegSet, block_indices: &[usize], reachable_blocks: &NoHashSet<usize>) {
        for &i in block_indices {
            if !reachable_blocks.contains(&i) {
                continue;
            }

            let basic_block = &mut basic_blocks[i];
            let sum_required_outputs = *basic_block.get_required_outputs() + required_outputs;
            if sum_required_outputs != *basic_block.get_required_outputs() || !basic_block.io_resolved {
                basic_block.io_resolved = true;
                basic_block.set_required_outputs(sum_required_outputs);
                basic_block.init_resolve_io(self);
                let enter_blocks = unsafe { slice::from_raw_parts(basic_block.enter_blocks.as_ptr(), basic_block.enter_blocks.len()) };
                let required_inputs = *basic_block.get_required_inputs();
                self.resolve_io(basic_blocks, required_inputs, enter_blocks, reachable_blocks);
            }
        }
    }

    fn remove_guest_input_regs(basic_blocks: &mut [BasicBlock], guest_input_regs: RegReserve, block_indices: &[usize]) {
        for &i in block_indices {
            let basic_block = &mut basic_blocks[i];
            for regs_live_range in &mut basic_block.regs_live_ranges {
                regs_live_range.remove_guests(guest_input_regs);
            }
            let enter_blocks = unsafe { slice::from_raw_parts(basic_block.enter_blocks.as_ptr(), basic_block.enter_blocks.len()) };
            Self::remove_guest_input_regs(basic_blocks, guest_input_regs, enter_blocks);
        }
    }

    fn assemble_basic_blocks(&mut self, block_start_pc: u32, thumb: bool) -> (Vec<BasicBlock>, NoHashSet<usize>) {
        unsafe { block_inst_list::reset_inst_list_entries() };
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

            match &mut self.buf.insts[i].kind {
                BlockInstKind::Label { label, .. } => {
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
                BlockInstKind::Branch { label, .. } => {
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
                BlockInstKind::Call { has_return: false, .. } | BlockInstKind::CallCommon { has_return: false, .. } | BlockInstKind::Epilogue { .. } => {
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
            let cond = self.buf.insts[last_inst_index].cond;
            match &mut self.buf.insts[last_inst_index].kind {
                BlockInstKind::Branch { label, block_index, fallthrough } => {
                    let labelled_block_index = basic_block_label_mapping.get(&label.0).unwrap();
                    basic_block.exit_blocks.push(*labelled_block_index);
                    *block_index = *labelled_block_index;
                    if (*fallthrough || cond != Cond::AL) && i + 1 < basic_blocks_len {
                        basic_block.exit_blocks.push(i + 1);
                    }
                }
                // Don't add exit when last command in basic block is a breakout
                BlockInstKind::Call { has_return: false, .. } | BlockInstKind::CallCommon { has_return: false, .. } | BlockInstKind::Epilogue { .. } => {
                    if cond != Cond::AL && i + 1 < basic_blocks_len {
                        basic_block.exit_blocks.push(i + 1);
                    }
                }
                _ if i + 1 < basic_blocks_len => basic_block.exit_blocks.push(i + 1),
                _ => {}
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

        let mut reachable_blocks = NoHashSet::default();
        self.resolve_guest_regs(&mut basic_blocks, RegReserve::new(), &[0], &mut reachable_blocks);

        for (i, basic_block) in basic_blocks.iter_mut().enumerate() {
            if !reachable_blocks.contains(&i) {
                continue;
            }

            let mut basic_block_start_pc = block_start_pc;
            let mut current_node = basic_block.block_entry_start;
            while !current_node.is_null() {
                match &self.buf.insts[BlockInstList::deref(current_node).value].kind {
                    BlockInstKind::Label { guest_pc: Some(pc), .. } => {
                        basic_block_start_pc = *pc;
                        break;
                    }
                    BlockInstKind::GuestPc(pc) => {
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
            if !reachable_blocks.contains(&i) {
                continue;
            }

            if basic_blocks[i].exit_blocks.is_empty() {
                self.resolve_io(&mut basic_blocks, BlockRegSet::new(), &[i], &reachable_blocks);
            }
        }

        for (i, basic_block) in basic_blocks.iter_mut().enumerate() {
            if !reachable_blocks.contains(&i) {
                continue;
            }

            basic_block.remove_dead_code(self);
        }

        (basic_blocks, reachable_blocks)
    }

    pub fn emit_opcodes(&mut self, block_start_pc: u32, thumb: bool) -> usize {
        let (mut basic_blocks, reachable_blocks) = self.assemble_basic_blocks(block_start_pc, thumb);

        if IS_DEBUG && !basic_blocks[0].get_required_inputs().get_guests().is_empty() {
            println!("inputs as requirement {:?}", basic_blocks[0].get_required_inputs().get_guests());
            unsafe { BLOCK_LOG = true };
        }

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

        self.buf.reg_allocator.dirty_regs.clear();
        self.buf.reg_allocator.global_mapping.fill(Reg::None);
        let mut input_regs = BlockRegSet::new();
        for (i, basic_block) in basic_blocks.iter().enumerate() {
            if !reachable_blocks.contains(&i) {
                continue;
            }

            input_regs += *basic_block.get_required_inputs();
        }
        let gp_guest_regs = input_regs.get_guests().get_gp_regs();
        for guest_reg in gp_guest_regs {
            self.buf.reg_allocator.global_mapping[guest_reg as usize] = guest_reg;
        }

        let mut non_input_guest_regs = input_regs;
        non_input_guest_regs.remove_guests(gp_guest_regs);
        let mut free_input_regs = (!input_regs.get_guests()).get_gp_regs();
        for reg in non_input_guest_regs.iter_any() {
            let free_input_reg = free_input_regs.pop().unwrap_or(Reg::None);
            self.buf.reg_allocator.global_mapping[reg as usize] = free_input_reg;
        }

        if IS_DEBUG && unsafe { BLOCK_LOG } {
            println!("global input regs {input_regs:?} {:?}", input_regs.get_guests().get_gp_regs());
            println!("global not guests {non_input_guest_regs:?}");
        }

        for (i, basic_block) in basic_blocks.iter_mut().enumerate() {
            if !reachable_blocks.contains(&i) {
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

            if !reachable_blocks.contains(&i) {
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
            if Cond::from(u8::from(encoding.cond())) == Cond::NV {
                self.buf.opcodes[branch_placeholder] = AluShiftImm::mov_al(Reg::R0, Reg::R0);
                continue;
            }

            let diff = if encoding.is_call_common() {
                let opcode_index = (jit_mem_offset >> 2) + branch_placeholder;
                let branch_to = u32::from(encoding.index()) >> 2;
                branch_to as i32 - opcode_index as i32
            } else {
                let block_index = u32::from(encoding.index());
                let branch_to = self.buf.block_opcode_offsets[block_index as usize];
                branch_to as i32 - branch_placeholder as i32
            };
            if diff == 1 && !encoding.has_return() {
                self.buf.opcodes[branch_placeholder] = AluShiftImm::mov_al(Reg::R0, Reg::R0);
            } else {
                self.buf.opcodes[branch_placeholder] = if encoding.has_return() { B::bl } else { B::b }(diff - 2, Cond::from(u8::from(encoding.cond())));
            }
        }

        &self.buf.opcodes
    }
}
