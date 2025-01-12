use crate::jit::assembler::arm::alu_assembler::AluShiftImm;
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::assembler::arm::transfer_assembler::LdmStm;
use crate::jit::assembler::basic_block::BasicBlock;
use crate::jit::assembler::block_inst::{
    Alu, AluOp, AluSetCond, BitField, BlockInst, BlockInstType, Branch, BranchEncoding, Call, Epilogue, Generic, GenericGuest, GuestPc, GuestTransferMultiple, Label, MarkRegDirty, PadBlock, Preload,
    RestoreReg, SaveContext, SaveReg, SystemReg, SystemRegOp, Transfer, TransferMultiple, TransferOp,
};
use crate::jit::assembler::block_reg_allocator::ALLOCATION_REGS;
use crate::jit::assembler::block_reg_set::BlockRegSet;
use crate::jit::assembler::{BasicBlocksCache, BlockAsmBuf, BlockLabel, BlockOperand, BlockOperandShift, BlockReg, BlockShift, ANY_REG_LIMIT};
use crate::jit::inst_info::InstInfo;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, ShiftType};
use crate::utils::{NoHashMap, NoHashSet};
use crate::IS_DEBUG;
use std::hint::assert_unchecked;
use std::intrinsics::unlikely;
use std::slice;

pub static mut BLOCK_LOG: bool = false;

macro_rules! alu3 {
    ($name:ident, $inst:ident, $set_cond:ident, $thumb_pc_aligned:expr) => {
        pub fn $name(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
            self.add_op3(AluOp::$inst, op0.into(), op1.into(), op2.into(), AluSetCond::$set_cond, $thumb_pc_aligned)
        }
    };
}

macro_rules! alu2_op1 {
    ($name:ident, $inst:ident, $set_cond:ident, $thumb_pc_aligned:expr) => {
        pub fn $name(&mut self, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
            self.add_op2_op1(AluOp::$inst, op1.into(), op2.into(), AluSetCond::$set_cond, $thumb_pc_aligned)
        }
    };
}

macro_rules! alu2_op0 {
    ($name:ident, $inst:ident, $set_cond:ident, $thumb_pc_aligned:expr) => {
        pub fn $name(&mut self, op0: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
            self.add_op2_op0(AluOp::$inst, op0.into(), op2.into(), AluSetCond::$set_cond, $thumb_pc_aligned)
        }
    };
}

pub struct BlockTmpRegs {
    pub thread_regs_addr_reg: BlockReg,
    pub host_cpsr_reg: BlockReg,
    pub guest_cpsr_reg: BlockReg,
    pub operand_imm_reg: BlockReg,
    shift_imm_reg: BlockReg,
    func_call_reg: BlockReg,
}

pub struct BlockAsm {
    cache: &'static mut BasicBlocksCache,
    buf: &'static mut BlockAsmBuf,

    is_thumb: bool,

    any_reg_count: u16,
    freed_any_regs: NoHashSet<u16>,
    label_count: u16,
    used_labels: NoHashSet<u16>,

    pub tmp_regs: BlockTmpRegs,

    cond_block_end_label_stack: Vec<(BlockLabel, Cond, usize)>,

    is_common_fun: bool,
    host_sp_ptr: *mut usize,

    last_pc: u32,
    last_pc_thumb_aligned: bool,
    last_pc_alu_shift: bool,
}

impl BlockAsm {
    pub fn new(is_common_fun: bool, guest_regs_ptr: *mut u32, host_sp_ptr: *mut usize, cache: &'static mut BasicBlocksCache, buf: &'static mut BlockAsmBuf, is_thumb: bool) -> Self {
        buf.insts.clear();
        buf.guest_branches_mapping.clear();

        let thread_regs_addr_reg = BlockReg::Any(Reg::SPSR as u16 + 1);
        let tmp_host_cpsr_reg = BlockReg::Any(Reg::SPSR as u16 + 2);
        let tmp_guest_cpsr_reg = BlockReg::Any(Reg::SPSR as u16 + 3);
        let tmp_operand_imm_reg = BlockReg::Any(Reg::SPSR as u16 + 4);
        let tmp_shift_imm_reg = BlockReg::Any(Reg::SPSR as u16 + 5);
        let tmp_func_call_reg = BlockReg::Any(Reg::SPSR as u16 + 6);
        let mut instance = BlockAsm {
            cache,
            buf,

            is_thumb,

            // First couple any regs are reserved for guest mapping
            any_reg_count: Reg::SPSR as u16 + 7,
            freed_any_regs: NoHashSet::default(),
            label_count: 0,
            used_labels: NoHashSet::default(),

            tmp_regs: BlockTmpRegs {
                thread_regs_addr_reg,
                host_cpsr_reg: tmp_host_cpsr_reg,
                guest_cpsr_reg: tmp_guest_cpsr_reg,
                operand_imm_reg: tmp_operand_imm_reg,
                shift_imm_reg: tmp_shift_imm_reg,
                func_call_reg: tmp_func_call_reg,
            },

            cond_block_end_label_stack: Vec::new(),

            is_common_fun,
            host_sp_ptr,

            last_pc: 0,
            last_pc_thumb_aligned: false,
            last_pc_alu_shift: false,
        };

        instance.insert_inst(Generic::Prologue);

        instance.sub(BlockReg::Fixed(Reg::SP), BlockReg::Fixed(Reg::SP), ANY_REG_LIMIT as u32 * 4); // Reserve for spilled registers

        if !is_common_fun {
            instance.mov(thread_regs_addr_reg, guest_regs_ptr as u32);
            for guest_reg in RegReserve::gp() + Reg::SP + Reg::LR {
                instance.restore_reg(guest_reg);
            }
            instance.restore_reg(Reg::CPSR);
        }

        instance
    }

    fn insert_inst(&mut self, inst: impl Into<BlockInst>) {
        let mut inst = inst.into();
        let (inputs, outputs) = inst.get_io();

        let guest_inputs = inputs.get_guests();
        let guest_outputs = outputs.get_guests();

        if guest_inputs.is_reserved(Reg::PC) {
            let mut last_pc = self.last_pc + if self.is_thumb { 4 } else { 8 };
            if self.last_pc_thumb_aligned {
                last_pc &= !0x3;
            } else if self.last_pc_alu_shift {
                last_pc += 4;
            }
            self.buf.insts.push(Alu::alu2(AluOp::Mov, [Reg::PC.into(), last_pc.into()], AluSetCond::None, false).into());
        }
        self.last_pc_alu_shift = false;
        self.last_pc_thumb_aligned = false;

        if unlikely(guest_inputs.is_reserved(Reg::CPSR)) {
            self.buf.insts.push(
                SystemReg {
                    op: SystemRegOp::Mrs,
                    operand: self.tmp_regs.host_cpsr_reg.into(),
                }
                .into(),
            );

            self.buf.insts.push(
                Transfer {
                    op: TransferOp::Read,
                    operands: [self.tmp_regs.guest_cpsr_reg.into(), self.tmp_regs.thread_regs_addr_reg.into(), (Reg::CPSR as u32 * 4).into()],
                    signed: false,
                    amount: MemoryAmount::Half,
                    add_to_base: true,
                }
                .into(),
            );

            self.buf.insts.push(
                Alu::alu3(
                    AluOp::And,
                    [self.tmp_regs.host_cpsr_reg.into(), self.tmp_regs.host_cpsr_reg.into(), (0xF8, ShiftType::Ror, 4).into()],
                    AluSetCond::None,
                    false,
                )
                .into(),
            );

            self.buf.insts.push(
                Alu::alu3(
                    AluOp::Orr,
                    [self.tmp_regs.host_cpsr_reg.into(), self.tmp_regs.host_cpsr_reg.into(), self.tmp_regs.guest_cpsr_reg.into()],
                    AluSetCond::None,
                    false,
                )
                .into(),
            );

            inst.replace_input_regs(Reg::CPSR.into(), self.tmp_regs.host_cpsr_reg);
        }

        if unlikely(guest_inputs.is_reserved(Reg::SPSR)) {
            self.buf.insts.push(
                Transfer {
                    op: TransferOp::Read,
                    operands: [Reg::SPSR.into(), self.tmp_regs.thread_regs_addr_reg.into(), (Reg::SPSR as u32 * 4).into()],
                    signed: false,
                    amount: MemoryAmount::Word,
                    add_to_base: true,
                }
                .into(),
            );
        }

        self.buf.insts.push(inst);

        if guest_inputs.is_reserved(Reg::PC) && !guest_outputs.is_reserved(Reg::PC) {
            self.buf.insts.push(MarkRegDirty { guest_reg: Reg::PC, dirty: false }.into());
        }

        if unlikely(guest_inputs.is_reserved(Reg::SPSR) && !guest_outputs.is_reserved(Reg::SPSR)) {
            self.buf.insts.push(MarkRegDirty { guest_reg: Reg::SPSR, dirty: false }.into());
        }
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
            self.mov(self.tmp_regs.shift_imm_reg, operand.shift.value);
            operand.shift.value = self.tmp_regs.shift_imm_reg.into();
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
                self.mov(self.tmp_regs.operand_imm_reg, op2.operand);
                *op2 = self.tmp_regs.operand_imm_reg.into();
            }
        }
    }

    fn add_op3(&mut self, op: AluOp, op0: BlockReg, op1: BlockReg, mut op2: BlockOperandShift, set_cond: AluSetCond, thumb_pc_aligned: bool) {
        self.check_alu_imm_limit(&mut op2, true);
        self.check_imm_shift_limit(&mut op2);
        self.last_pc_thumb_aligned = thumb_pc_aligned;
        self.insert_inst(Alu::alu3(op, [op0.into(), op1.into(), op2], set_cond, thumb_pc_aligned))
    }

    fn add_op2_op1(&mut self, op: AluOp, op1: BlockReg, mut op2: BlockOperandShift, set_cond: AluSetCond, thumb_pc_aligned: bool) {
        self.check_alu_imm_limit(&mut op2, true);
        self.check_imm_shift_limit(&mut op2);
        self.last_pc_thumb_aligned = thumb_pc_aligned;
        self.insert_inst(Alu::alu2(op, [op1.into(), op2], set_cond, thumb_pc_aligned))
    }

    fn add_op2_op0(&mut self, op: AluOp, op0: BlockReg, mut op2: BlockOperandShift, set_cond: AluSetCond, thumb_pc_aligned: bool) {
        self.check_alu_imm_limit(&mut op2, op != AluOp::Mov);
        self.check_imm_shift_limit(&mut op2);
        self.last_pc_thumb_aligned = thumb_pc_aligned;
        self.insert_inst(Alu::alu2(op, [op0.into(), op2], set_cond, thumb_pc_aligned))
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
        self.transfer(TransferOp::Read, op0, op1, op2, signed, amount)
    }

    pub fn transfer_write(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>, signed: bool, amount: MemoryAmount) {
        self.transfer(TransferOp::Write, op0, op1, op2, signed, amount)
    }

    pub fn transfer(&mut self, op: TransferOp, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>, signed: bool, amount: MemoryAmount) {
        let mut op2 = op2.into();
        if op2.operand.needs_reg_for_imm(0xFFF) {
            self.mov(self.tmp_regs.operand_imm_reg, op2.operand);
            op2 = self.tmp_regs.operand_imm_reg.into();
        }
        self.check_imm_shift_limit(&mut op2);
        self.insert_inst(Transfer {
            op,
            operands: [op0.into().into(), op1.into().into(), op2],
            signed,
            amount,
            add_to_base: true,
        })
    }

    pub fn transfer_push(&mut self, operand: impl Into<BlockReg>, regs: RegReserve) {
        self.transfer_write_multiple(operand, regs, true, true, false)
    }

    pub fn transfer_pop(&mut self, operand: impl Into<BlockReg>, regs: RegReserve) {
        self.transfer_read_multiple(operand, regs, true, false, true)
    }

    pub fn transfer_read_multiple(&mut self, operand: impl Into<BlockReg>, regs: RegReserve, write_back: bool, pre: bool, add_to_base: bool) {
        self.insert_inst(TransferMultiple {
            op: TransferOp::Read,
            operand: operand.into(),
            regs,
            write_back,
            pre,
            add_to_base,
        })
    }

    pub fn transfer_write_multiple(&mut self, operand: impl Into<BlockReg>, regs: RegReserve, write_back: bool, pre: bool, add_to_base: bool) {
        self.insert_inst(TransferMultiple {
            op: TransferOp::Write,
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
        self.insert_inst(GuestTransferMultiple {
            op: TransferOp::Read,
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
        self.insert_inst(GuestTransferMultiple {
            op: TransferOp::Write,
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
        self.insert_inst(SystemReg {
            op: SystemRegOp::Mrs,
            operand: operand.into().into(),
        })
    }

    pub fn msr_cpsr(&mut self, operand: impl Into<BlockOperand>) {
        self.insert_inst(SystemReg {
            op: SystemRegOp::Msr,
            operand: operand.into(),
        })
    }

    pub fn bfc(&mut self, operand: impl Into<BlockReg>, lsb: u8, width: u8) {
        self.insert_inst(BitField::bfc(operand.into(), lsb, width))
    }

    pub fn bfi(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, lsb: u8, width: u8) {
        self.insert_inst(BitField::bfi([op0.into(), op1.into()], lsb, width));
    }

    pub fn ubfx(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, lsb: u8, width: u8) {
        self.insert_inst(BitField::ubfx([op0.into(), op1.into()], lsb, width));
    }

    pub fn muls_guest_thumb_pc_aligned(&mut self, op0: impl Into<BlockReg>, op1: impl Into<BlockReg>, op2: impl Into<BlockOperandShift>) {
        self.insert_inst(Alu::mul([op0.into().into(), op1.into().into(), op2.into()], AluSetCond::HostGuest, true));
    }

    fn label_internal(&mut self, label: BlockLabel, unlikely: bool) {
        if !self.used_labels.insert(label.0) {
            panic!("{label:?} was already added");
        }
        self.insert_inst(Label { label, guest_pc: None, unlikely })
    }

    pub fn label(&mut self, label: BlockLabel) {
        self.label_internal(label, false);
    }

    pub fn label_unlikely(&mut self, label: BlockLabel) {
        self.label_internal(label, true);
    }

    pub fn branch(&mut self, label: BlockLabel, cond: Cond) {
        self.insert_inst(BlockInst::new(
            cond,
            Branch {
                label,
                block_index: 0,
                fallthrough: false,
            }
            .into(),
        ))
    }

    pub fn branch_fallthrough(&mut self, label: BlockLabel, cond: Cond) {
        self.insert_inst(BlockInst::new(
            cond,
            Branch {
                label,
                block_index: 0,
                fallthrough: true,
            }
            .into(),
        ))
    }

    pub fn save_context(&mut self) {
        self.insert_inst(SaveContext { guest_regs: RegReserve::new() });
        for reg in Reg::R1 as u8..Reg::SPSR as u8 {
            let guest_reg = Reg::from(reg);
            let mut inst: BlockInst = SaveReg {
                guest_reg,
                reg_mapped: BlockReg::from(guest_reg),
                thread_regs_addr_reg: self.tmp_regs.thread_regs_addr_reg,
                tmp_host_cpsr_reg: self.tmp_regs.host_cpsr_reg,
            }
            .into();
            inst.skip = true;
            self.buf.insts.push(inst);
        }
    }

    pub fn save_reg(&mut self, guest_reg: Reg) {
        self.buf.insts.push(
            SaveReg {
                guest_reg,
                reg_mapped: BlockReg::from(guest_reg),
                thread_regs_addr_reg: self.tmp_regs.thread_regs_addr_reg,
                tmp_host_cpsr_reg: self.tmp_regs.host_cpsr_reg,
            }
            .into(),
        );
    }

    pub fn restore_reg(&mut self, guest_reg: Reg) {
        self.insert_inst(RestoreReg {
            guest_reg,
            reg_mapped: BlockReg::from(guest_reg),
            thread_regs_addr_reg: self.tmp_regs.thread_regs_addr_reg,
            tmp_guest_cpsr_reg: self.tmp_regs.guest_cpsr_reg,
        });
    }

    pub fn mark_reg_dirty(&mut self, guest_reg: Reg, dirty: bool) {
        self.buf.insts.push(MarkRegDirty { guest_reg, dirty }.into());
    }

    pub fn epilogue(&mut self) {
        let host_sp_addr_reg = self.tmp_regs.thread_regs_addr_reg;
        self.mov(host_sp_addr_reg, self.host_sp_ptr as u32);
        self.load_u32(BlockReg::Fixed(Reg::SP), host_sp_addr_reg, 0);
        self.insert_inst(Epilogue { restore_all_regs: true });
    }

    pub fn epilogue_previous_block(&mut self) {
        self.add(BlockReg::Fixed(Reg::SP), BlockReg::Fixed(Reg::SP), ANY_REG_LIMIT as u32 * 4);
        self.insert_inst(Epilogue { restore_all_regs: false });
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
        self.mov(self.tmp_regs.func_call_reg, func.into());
        self.insert_inst(Call::reg(
            self.tmp_regs.func_call_reg,
            [
                args[0].map(|_| BlockReg::Fixed(Reg::R0)),
                args[1].map(|_| BlockReg::Fixed(Reg::R1)),
                args[2].map(|_| BlockReg::Fixed(Reg::R2)),
                args[3].map(|_| BlockReg::Fixed(Reg::R3)),
            ],
            has_return,
        ));
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
        self.insert_inst(Call::offset(
            offset,
            [
                args[0].map(|_| BlockReg::Fixed(Reg::R0)),
                args[1].map(|_| BlockReg::Fixed(Reg::R1)),
                args[2].map(|_| BlockReg::Fixed(Reg::R2)),
                args[3].map(|_| BlockReg::Fixed(Reg::R3)),
            ],
            has_return,
        ));
    }

    pub fn bkpt(&mut self, id: u16) {
        self.insert_inst(Generic::Bkpt(id));
    }

    pub fn pli(&mut self, op0: impl Into<BlockReg>, offset: u16, add: bool) {
        self.insert_inst(Preload { operand: op0.into(), offset, add })
    }

    pub fn nop(&mut self) {
        self.insert_inst(Generic::Nop);
    }

    pub fn guest_pc(&mut self, pc: u32) {
        self.last_pc = pc;
        self.insert_inst(GuestPc(pc));
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
            Branch {
                label,
                block_index: 0,
                fallthrough: false,
            }
            .into(),
        ));
    }

    pub fn generic_guest_inst(&mut self, inst_info: &mut InstInfo) {
        // PC + 12 when ALU shift by register
        if inst_info.op.is_alu_reg_shift() && *inst_info.operands().last().unwrap().as_reg().unwrap().0 == Reg::PC {
            self.last_pc_alu_shift = true;
        }
        self.insert_inst(GenericGuest::new(inst_info));
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
            let (_, outputs) = self.buf.get_inst(start_index).get_io();
            if cond_block_size == 1 && self.buf.get_inst(start_index).cond == Cond::AL && !outputs.contains(BlockReg::Fixed(Reg::CPSR)) && !outputs.contains(Reg::CPSR.into()) {
                self.buf.get_inst_mut(start_index).cond = cond;
                self.buf.get_inst_mut(start_index).invalidate_io_cache();
                // Remove the branch
                self.buf.insts.remove(start_index - 1);
            } else {
                self.label(label);
            }
        }
    }

    pub fn pad_block(&mut self, label: BlockLabel, correction: i32) {
        self.insert_inst(PadBlock { label, correction });
    }

    fn resolve_guest_regs(&mut self, guest_regs_dirty: RegReserve, block_indices: &[usize]) {
        for &i in block_indices {
            let basic_block = &mut self.cache.basic_blocks[i];
            let sum_guest_regs_input_dirty = basic_block.guest_regs_input_dirty + guest_regs_dirty;
            if sum_guest_regs_input_dirty != basic_block.guest_regs_input_dirty || !basic_block.guest_regs_resolved {
                self.buf.reachable_blocks.insert(i);
                basic_block.guest_regs_resolved = true;
                basic_block.guest_regs_input_dirty = sum_guest_regs_input_dirty;
                basic_block.init_guest_regs(self.buf);
                let exit_blocks = unsafe { slice::from_raw_parts(basic_block.exit_blocks.as_ptr(), basic_block.exit_blocks.len()) };
                let guest_regs_dirty = basic_block.guest_regs_output_dirty;
                self.resolve_guest_regs(guest_regs_dirty, exit_blocks);
            }
        }
    }

    fn resolve_io(&mut self, required_outputs: &BlockRegSet, block_indices: &[usize]) {
        for &i in block_indices {
            if !self.buf.reachable_blocks.contains(i) {
                continue;
            }

            let basic_block = unsafe { self.cache.basic_blocks.get_unchecked_mut(i) };
            let sum_required_outputs = *basic_block.get_required_outputs() + required_outputs;
            if sum_required_outputs != basic_block.get_required_outputs() || !basic_block.io_resolved {
                basic_block.io_resolved = true;
                basic_block.set_required_outputs(sum_required_outputs);
                basic_block.init_resolve_io(self.buf);
                let enter_blocks = unsafe { slice::from_raw_parts(basic_block.enter_blocks.as_ptr(), basic_block.enter_blocks.len()) };
                let required_inputs = unsafe { (basic_block.get_required_inputs() as *const BlockRegSet).as_ref_unchecked() };
                self.resolve_io(required_inputs, enter_blocks);
            }
        }
    }

    fn assemble_basic_blocks(&mut self, block_start_pc: u32) -> usize {
        #[derive(Default)]
        struct BasicBlockData {
            start: u16,
            last_label: Option<BlockLabel>,
            last_label_unlikely: bool,
        }
        let mut basic_block_data = BasicBlockData::default();

        let mut basic_blocks_label_mapping = NoHashMap::<u16, usize>::default();
        let mut basic_blocks_unlikely_label_mapping = NoHashMap::<u16, usize>::default();
        let mut basic_blocks_len = 0;
        let mut basic_blocks_unlikely_len = 0;

        for i in 0..self.buf.insts.len() {
            let (basic_blocks, basic_blocks_len, basic_block_label_mapping) = if basic_block_data.last_label_unlikely {
                (&mut self.cache.basic_blocks_unlikely, &mut basic_blocks_unlikely_len, &mut basic_blocks_unlikely_label_mapping)
            } else {
                (&mut self.cache.basic_blocks, &mut basic_blocks_len, &mut basic_blocks_label_mapping)
            };

            let handle_label = |buf,
                                basic_blocks: &mut Vec<BasicBlock>,
                                label: BlockLabel,
                                unlikely: bool,
                                basic_blocks_len: &mut u16,
                                basic_block_label_mapping: &mut NoHashMap<u16, usize>,
                                data: &mut BasicBlockData| {
                // block_start_entry can be the same as current_node, when the previous iteration ended with a branch
                if data.start as usize != i {
                    *basic_blocks_len += 1;
                    if *basic_blocks_len as usize > basic_blocks.len() {
                        basic_blocks.push(BasicBlock::new());
                    }
                    basic_blocks[*basic_blocks_len as usize - 1].init(buf, data.start as usize, i - 1);
                    data.start = i as u16;
                    if let Some(last_label) = data.last_label {
                        basic_block_label_mapping.insert(last_label.0, *basic_blocks_len as usize - 1);
                    }
                }
                data.last_label_unlikely = unlikely;
                data.last_label = Some(label);
            };

            let handle_branch = |buf, basic_blocks: &mut Vec<BasicBlock>, basic_blocks_len: &mut u16, basic_block_label_mapping: &mut NoHashMap<u16, usize>, data: &mut BasicBlockData| {
                *basic_blocks_len += 1;
                if *basic_blocks_len as usize > basic_blocks.len() {
                    basic_blocks.push(BasicBlock::new());
                }
                basic_blocks[*basic_blocks_len as usize - 1].init(buf, data.start as usize, i);
                data.start = i as u16 + 1;
                if let Some(last_label) = data.last_label {
                    basic_block_label_mapping.insert(last_label.0, *basic_blocks_len as usize - 1);
                }
                data.last_label = None;
            };

            match &self.buf.get_inst(i).inst_type {
                BlockInstType::GuestPc(inner) => {
                    let guest_pc = inner.0;
                    if let Some(&guest_label) = self.buf.guest_branches_mapping.get(&guest_pc) {
                        *self.buf.get_inst_mut(i) = Label {
                            label: guest_label,
                            guest_pc: Some(guest_pc),
                            unlikely: false,
                        }
                        .into();
                        handle_label(self.buf, basic_blocks, guest_label, false, basic_blocks_len, basic_block_label_mapping, &mut basic_block_data);
                    }
                }
                BlockInstType::Label(inner) => {
                    handle_label(self.buf, basic_blocks, inner.label, inner.unlikely, basic_blocks_len, basic_block_label_mapping, &mut basic_block_data);
                }
                BlockInstType::Branch(_) | BlockInstType::Epilogue(_) => handle_branch(self.buf, basic_blocks, basic_blocks_len, basic_block_label_mapping, &mut basic_block_data),
                BlockInstType::Call(inner) => {
                    if !inner.has_return {
                        handle_branch(self.buf, basic_blocks, basic_blocks_len, basic_block_label_mapping, &mut basic_block_data);
                    }
                }
                _ => {}
            }
        }

        if (basic_block_data.start as usize) < self.buf.insts.len() {
            let (basic_blocks, basic_blocks_len, basic_block_label_mapping) = if basic_block_data.last_label_unlikely {
                (&mut self.cache.basic_blocks_unlikely, &mut basic_blocks_unlikely_len, &mut basic_blocks_unlikely_label_mapping)
            } else {
                (&mut self.cache.basic_blocks, &mut basic_blocks_len, &mut basic_blocks_label_mapping)
            };
            *basic_blocks_len += 1;
            if *basic_blocks_len as usize > basic_blocks.len() {
                basic_blocks.push(BasicBlock::new());
            }
            basic_blocks[*basic_blocks_len as usize - 1].init(self.buf, basic_block_data.start as usize, self.buf.insts.len() - 1);
            if let Some(last_label) = basic_block_data.last_label {
                basic_block_label_mapping.insert(last_label.0, *basic_blocks_len as usize - 1);
            }
        }

        for (label, block_index) in basic_blocks_unlikely_label_mapping {
            basic_blocks_label_mapping.insert(label, block_index + basic_blocks_len as usize);
        }
        if self.cache.basic_blocks.len() < (basic_blocks_len + basic_blocks_unlikely_len) as usize {
            self.cache.basic_blocks.resize_with((basic_blocks_len + basic_blocks_unlikely_len) as usize, BasicBlock::new);
        }
        self.cache.basic_blocks[basic_blocks_len as usize..(basic_blocks_len + basic_blocks_unlikely_len) as usize]
            .swap_with_slice(&mut self.cache.basic_blocks_unlikely[..basic_blocks_unlikely_len as usize]);
        basic_blocks_len += basic_blocks_unlikely_len;

        let basic_blocks_len = basic_blocks_len as usize;
        // Link blocks
        for (i, basic_block) in self.cache.basic_blocks[..basic_blocks_len].iter_mut().enumerate() {
            let last_inst_index = basic_block.block_entry_end;
            let cond = self.buf.get_inst(last_inst_index).cond;

            match &mut self.buf.get_inst_mut(last_inst_index).inst_type {
                BlockInstType::Branch(inner) => {
                    let labelled_block_index = basic_blocks_label_mapping.get(&inner.label.0).unwrap();
                    basic_block.exit_blocks.push(*labelled_block_index);
                    inner.block_index = *labelled_block_index;
                    if (inner.fallthrough || cond != Cond::AL) && i + 1 < basic_blocks_len {
                        basic_block.exit_blocks.push(i + 1);
                    }
                }
                // Don't add exit when last command in basic block is a breakout
                BlockInstType::Call(inner) => {
                    if (inner.has_return || cond != Cond::AL) && i + 1 < basic_blocks_len {
                        basic_block.exit_blocks.push(i + 1);
                    }
                }
                BlockInstType::Epilogue(_) => {
                    if cond != Cond::AL && i + 1 < basic_blocks_len {
                        basic_block.exit_blocks.push(i + 1);
                    }
                }
                _ if i + 1 < basic_blocks_len => basic_block.exit_blocks.push(i + 1),
                _ => {}
            }
        }

        if IS_DEBUG && unsafe { BLOCK_LOG } {
            for i in 0..basic_blocks_len {
                println!("basic block: {i} {:?}", self.cache.basic_blocks[i]);
            }
        }

        for i in 0..basic_blocks_len {
            let exit_blocks = &self.cache.basic_blocks[i].exit_blocks;
            debug_assert!(exit_blocks.len() <= 1 || (exit_blocks.len() == 2 && exit_blocks[0] != exit_blocks[1]), "basic block {i}");
            for j in 0..exit_blocks.len() {
                let exit_block = self.cache.basic_blocks[i].exit_blocks[j];
                if !self.cache.basic_blocks[exit_block].enter_blocks.contains(&i) {
                    self.cache.basic_blocks[exit_block].enter_blocks.push(i);
                }
            }
        }

        self.buf.reachable_blocks.clear();
        self.resolve_guest_regs(RegReserve::new(), &[0]);

        for (i, basic_block) in self.cache.basic_blocks[..basic_blocks_len].iter_mut().enumerate() {
            if !self.buf.reachable_blocks.contains(i) {
                continue;
            }

            let mut basic_block_start_pc = block_start_pc;
            for i in (0..=basic_block.block_entry_start).rev() {
                match &self.buf.get_inst(i).inst_type {
                    BlockInstType::Label(inner) => {
                        if let Some(pc) = inner.guest_pc {
                            basic_block_start_pc = pc;
                            break;
                        }
                    }
                    BlockInstType::GuestPc(inner) => {
                        basic_block_start_pc = inner.0;
                        break;
                    }
                    _ => {}
                }
            }
            basic_block.init_insts(self.buf, &self.tmp_regs, basic_block_start_pc);
        }

        for i in (0..basic_blocks_len).rev() {
            if !self.buf.reachable_blocks.contains(i) {
                continue;
            }

            if self.cache.basic_blocks[i].exit_blocks.is_empty() {
                self.resolve_io(&BlockRegSet::new(), &[i]);
            }
        }

        for (i, basic_block) in self.cache.basic_blocks[..basic_blocks_len].iter_mut().enumerate() {
            if !self.buf.reachable_blocks.contains(i) {
                continue;
            }

            basic_block.remove_dead_code(self.buf);
        }

        self.buf.basic_block_label_mapping = basic_blocks_label_mapping;

        basic_blocks_len
    }

    pub fn emit_opcodes(&mut self, block_start_pc: u32) -> usize {
        let basic_blocks_len = self.assemble_basic_blocks(block_start_pc);

        if IS_DEBUG && !self.cache.basic_blocks[0].get_required_inputs().get_guests().is_empty() {
            println!("inputs as requirement {:?}", self.cache.basic_blocks[0].get_required_inputs().get_guests());
            unsafe { BLOCK_LOG = true };
        }

        if IS_DEBUG && unsafe { BLOCK_LOG } {
            for (i, basic_block) in self.cache.basic_blocks[..basic_blocks_len].iter().enumerate() {
                println!("{i}[{i} {:x}]", basic_block.start_pc);
            }

            for (i, basic_block) in self.cache.basic_blocks[..basic_blocks_len].iter().enumerate() {
                for &exit_i in &basic_block.exit_blocks {
                    println!("{i} --> {exit_i}");
                }
            }

            for (i, basic_block) in self.cache.basic_blocks[..basic_blocks_len].iter().enumerate() {
                println!("{i}: {basic_block:?}");
            }
        }

        self.buf.reg_allocator.dirty_regs.clear();
        self.buf.reg_allocator.global_mapping.fill(Reg::None);
        let mut input_regs = BlockRegSet::new();
        for (i, basic_block) in self.cache.basic_blocks[..basic_blocks_len].iter().enumerate() {
            if !self.buf.reachable_blocks.contains(i) {
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

        self.buf.opcodes.clear();
        self.buf.block_opcode_offsets.resize(basic_blocks_len, 0);
        self.buf.resize_placeholders(basic_blocks_len);

        for i in 0..basic_blocks_len {
            if let Some((label, correction)) = self.cache.basic_blocks[i].pad_label {
                let block_to_pad_to = *self.buf.basic_block_label_mapping.get(&label.0).unwrap();
                self.cache.basic_blocks[block_to_pad_to].emit_opcodes(self.buf, true, block_to_pad_to);
                self.cache.basic_blocks[i].pad_size = (self.cache.basic_blocks[block_to_pad_to].opcodes.len() as i32 + correction) as usize;
            }
        }

        for (i, basic_block) in self.cache.basic_blocks[..basic_blocks_len].iter_mut().enumerate() {
            let opcodes_len = self.buf.opcodes.len();
            self.buf.block_opcode_offsets[i] = opcodes_len;

            if !self.buf.reachable_blocks.contains(i) {
                self.buf.clear_placeholders_block(i);
                continue;
            }

            basic_block.emit_opcodes(self.buf, false, i);
        }

        self.buf.opcodes.len()
    }

    pub fn finalize(&mut self, jit_mem_offset: usize) -> &Vec<u32> {
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

        for (block_index, placeholders) in self.buf.placeholders.iter().enumerate() {
            unsafe { assert_unchecked(block_index < self.buf.block_opcode_offsets.len()) };
            for &branch_placeholder in &placeholders.branch {
                if IS_DEBUG && unsafe { BLOCK_LOG } {
                    println!("{block_index}: offset: {}, {branch_placeholder}", self.buf.block_opcode_offsets[block_index]);
                }
                let index = self.buf.block_opcode_offsets[block_index] + branch_placeholder;
                unsafe { assert_unchecked(index < self.buf.opcodes.len()) };
                let encoding = BranchEncoding::from(self.buf.opcodes[index]);
                if Cond::from(u8::from(encoding.cond())) == Cond::NV {
                    self.buf.opcodes[index] = AluShiftImm::mov_al(Reg::R0, Reg::R0);
                    continue;
                }

                let diff = if encoding.is_call_common() {
                    let opcode_index = (jit_mem_offset >> 2) + index;
                    let branch_to = u32::from(encoding.index()) >> 2;
                    branch_to as i32 - opcode_index as i32
                } else {
                    let block_index = u32::from(encoding.index());
                    let branch_to = self.buf.block_opcode_offsets[block_index as usize];
                    branch_to as i32 - index as i32
                };
                if diff == 1 && !encoding.has_return() {
                    self.buf.opcodes[index] = AluShiftImm::mov_al(Reg::R0, Reg::R0);
                } else {
                    self.buf.opcodes[index] = if encoding.has_return() { B::bl } else { B::b }(diff - 2, Cond::from(u8::from(encoding.cond())));
                }
            }

            for &prologue_placeholder in &placeholders.prologue {
                let index = self.buf.block_opcode_offsets[block_index] + prologue_placeholder;
                unsafe { assert_unchecked(index < self.buf.opcodes.len()) };
                self.buf.opcodes[index] = LdmStm::generic(Reg::SP, used_host_regs + Reg::LR, false, true, false, true, Cond::AL);
            }

            for &epilogue_placeholder in &placeholders.epilogue {
                let index = self.buf.block_opcode_offsets[block_index] + epilogue_placeholder;
                unsafe { assert_unchecked(index < self.buf.opcodes.len()) };
                let restore_all_regs = self.buf.opcodes[index] != 0;
                self.buf.opcodes[index] = LdmStm::generic(
                    Reg::SP,
                    if restore_all_regs { ALLOCATION_REGS + Reg::R12 } else { used_host_regs } + Reg::PC,
                    true,
                    true,
                    true,
                    false,
                    Cond::AL,
                );
            }
        }

        &self.buf.opcodes
    }
}
