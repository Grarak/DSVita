use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg, AluShiftImm, Bfc, Bfi, MulReg, Ubfx};
use crate::jit::assembler::arm::branch_assembler::Bx;
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, LdrStrImm, LdrStrImmSBHD, LdrStrReg, LdrStrRegSBHD, Mrs, Msr};
use crate::jit::assembler::arm::{transfer_assembler, Bkpt};
use crate::jit::assembler::block_reg_allocator::BlockRegAllocator;
use crate::jit::assembler::block_reg_set::{block_reg_set, BlockRegSet};
use crate::jit::assembler::{BlockAsmPlaceholders, BlockLabel, BlockOperand, BlockOperandShift, BlockReg, BlockShift};
use crate::jit::inst_info::{InstInfo, Operand, Shift, ShiftValue};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, ShiftType};
use bilge::prelude::*;
use enum_dispatch::enum_dispatch;
use std::cell::UnsafeCell;
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AluOp {
    And = 0,
    Eor = 1,
    Sub = 2,
    Rsb = 3,
    Add = 4,
    Adc = 5,
    Sbc = 6,
    Rsc = 7,
    Tst = 8,
    Teq = 9,
    Cmp = 10,
    Cmn = 11,
    Orr = 12,
    Mov = 13,
    Bic = 14,
    Mvn = 15,
}

enum AluType {
    Alu3,
    Alu2Op1,
    Alu2Op0,
    Mul,
}

impl From<AluOp> for AluType {
    fn from(value: AluOp) -> Self {
        match value {
            AluOp::And | AluOp::Eor | AluOp::Sub | AluOp::Rsb | AluOp::Add | AluOp::Adc | AluOp::Sbc | AluOp::Rsc | AluOp::Orr | AluOp::Bic => AluType::Alu3,
            AluOp::Tst | AluOp::Teq | AluOp::Cmp | AluOp::Cmn => AluType::Alu2Op1,
            AluOp::Mov | AluOp::Mvn => AluType::Alu2Op0,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum AluSetCond {
    None,
    Host,
    HostGuest,
}

impl Debug for AluSetCond {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AluSetCond::None => write!(f, ""),
            AluSetCond::Host => write!(f, "s"),
            AluSetCond::HostGuest => write!(f, "s_guest"),
        }
    }
}

pub struct Alu {
    op: AluOp,
    op_type: AluType,
    operands: [BlockOperandShift; 3],
    set_cond: AluSetCond,
    pub thumb_pc_aligned: bool,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum TransferOp {
    #[default]
    Read,
    Write,
}

pub struct Transfer {
    pub op: TransferOp,
    pub operands: [BlockOperandShift; 3],
    pub signed: bool,
    pub amount: MemoryAmount,
    pub add_to_base: bool,
}

pub struct TransferMultiple {
    pub op: TransferOp,
    pub operand: BlockReg,
    pub regs: RegReserve,
    pub write_back: bool,
    pub pre: bool,
    pub add_to_base: bool,
}

pub struct GuestTransferMultiple {
    pub op: TransferOp,
    pub addr_reg: BlockReg,
    pub addr_out_reg: BlockReg,
    pub gp_regs: RegReserve,
    pub fixed_regs: RegReserve,
    pub write_back: bool,
    pub pre: bool,
    pub add_to_base: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SystemRegOp {
    Mrs,
    Msr,
}

pub struct SystemReg {
    pub op: SystemRegOp,
    pub operand: BlockOperand,
}

#[derive(Debug)]
pub enum BitFieldOp {
    Bfc,
    Bfi,
    Ubfx,
}

pub struct BitField {
    op: BitFieldOp,
    operands: [BlockReg; 2],
    lsb: u8,
    width: u8,
}

pub struct SaveReg {
    pub guest_reg: Reg,
    pub reg_mapped: BlockReg,
    pub thread_regs_addr_reg: BlockReg,
    pub tmp_host_cpsr_reg: BlockReg,
}

pub struct RestoreReg {
    pub guest_reg: Reg,
    pub reg_mapped: BlockReg,
    pub thread_regs_addr_reg: BlockReg,
    pub tmp_guest_cpsr_reg: BlockReg,
}

pub struct GuestInstInfo(pub NonNull<InstInfo>);

impl GuestInstInfo {
    pub fn new(inst_info: &mut InstInfo) -> Self {
        GuestInstInfo(NonNull::from(inst_info))
    }
}

impl Debug for GuestInstInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}

impl Deref for GuestInstInfo {
    type Target = InstInfo;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_ref() }
    }
}

impl DerefMut for GuestInstInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.as_mut() }
    }
}

pub struct GenericGuest {
    pub inst_info: GuestInstInfo,
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct BranchEncoding {
    pub index: u26,
    pub has_return: bool,
    pub is_call_common: bool,
    pub cond: u4,
}

pub enum CallOp {
    Reg(BlockReg),
    Offset(usize),
}

pub struct Call {
    op: CallOp,
    args: [Option<BlockReg>; 4],
    pub has_return: bool,
}

pub struct Label {
    pub label: BlockLabel,
    pub guest_pc: Option<u32>,
    pub unlikely: bool,
}

pub struct Branch {
    pub label: BlockLabel,
    pub block_index: usize,
    pub fallthrough: bool,
}

pub struct Preload {
    pub operand: BlockReg,
    pub offset: u16,
    pub add: bool,
}

pub struct SaveContext {
    pub guest_regs: RegReserve,
}

pub struct GuestPc(pub u32);

pub struct Epilogue {
    pub restore_all_regs: bool,
    pub restore_state: bool,
}

pub struct MarkRegDirty {
    pub guest_reg: Reg,
    pub dirty: bool,
}

pub struct PadBlock {
    pub label: BlockLabel,
    pub correction: i32,
}

pub enum Generic {
    Bkpt(u16),
    Nop,
    Prologue,
}

#[enum_dispatch]
pub enum BlockInstType {
    Alu,
    Transfer,
    TransferMultiple,
    GuestTransferMultiple,
    SystemReg,
    BitField,
    SaveReg,
    RestoreReg,
    GenericGuest,
    Call,
    Label,
    Branch,
    Preload,
    SaveContext,
    GuestPc,
    Epilogue,
    MarkRegDirty,
    PadBlock,
    Generic,
}

#[enum_dispatch(BlockInstType)]
pub trait BlockInstTrait {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet);
    fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg);
    fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg);
    fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, opcode_index: usize, placeholders: &mut BlockAsmPlaceholders);
}

impl Debug for BlockInstType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockInstType::Alu(inner) => inner.fmt(f),
            BlockInstType::Transfer(inner) => inner.fmt(f),
            BlockInstType::TransferMultiple(inner) => inner.fmt(f),
            BlockInstType::GuestTransferMultiple(inner) => inner.fmt(f),
            BlockInstType::SystemReg(inner) => inner.fmt(f),
            BlockInstType::BitField(inner) => inner.fmt(f),
            BlockInstType::SaveReg(inner) => inner.fmt(f),
            BlockInstType::RestoreReg(inner) => inner.fmt(f),
            BlockInstType::GenericGuest(inner) => inner.fmt(f),
            BlockInstType::Call(inner) => inner.fmt(f),
            BlockInstType::Label(inner) => inner.fmt(f),
            BlockInstType::Branch(inner) => inner.fmt(f),
            BlockInstType::Preload(inner) => inner.fmt(f),
            BlockInstType::SaveContext(inner) => inner.fmt(f),
            BlockInstType::GuestPc(inner) => inner.fmt(f),
            BlockInstType::Epilogue(inner) => inner.fmt(f),
            BlockInstType::MarkRegDirty(inner) => inner.fmt(f),
            BlockInstType::PadBlock(inner) => inner.fmt(f),
            BlockInstType::Generic(inner) => inner.fmt(f),
        }
    }
}

pub struct BlockInst {
    pub cond: Cond,
    pub inst_type: BlockInstType,
    io_cache: UnsafeCell<Option<(BlockRegSet, BlockRegSet)>>,
    pub skip: bool,
}

impl BlockInst {
    pub fn new(cond: Cond, inst_type: BlockInstType) -> Self {
        BlockInst {
            cond,
            inst_type,
            io_cache: UnsafeCell::new(None),
            skip: false,
        }
    }

    pub fn invalidate_io_cache(&mut self) {
        *self.io_cache.get_mut() = None;
    }

    pub fn get_io(&self) -> &(BlockRegSet, BlockRegSet) {
        let cached_io = unsafe { self.io_cache.get().as_ref_unchecked() };
        match cached_io {
            None => {
                let (mut inputs, outputs) = self.inst_type.get_io();
                if self.cond != Cond::AL {
                    // For conditional insts initialize output guest regs as well
                    // Otherwise arbitrary values for regs will be saved
                    inputs.add_guests(outputs.get_guests());
                }
                let cache = unsafe { self.io_cache.get().as_mut_unchecked() };
                *cache = Some((inputs, outputs));
                cache.as_ref().unwrap()
            }
            Some(cache) => cache,
        }
    }

    pub fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        *self.io_cache.get_mut() = None;
        self.inst_type.replace_input_regs(old, new);
    }

    pub fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg) {
        *self.io_cache.get_mut() = None;
        self.inst_type.replace_output_regs(old, new);
    }

    pub fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, opcode_index: usize, placeholders: &mut BlockAsmPlaceholders) {
        self.inst_type.emit_opcode(alloc, opcodes, opcode_index, placeholders)
    }
}

impl Debug for BlockInst {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.skip {
            write!(f, "SKIPPED: {:?} {:?}", self.cond, self.inst_type)
        } else {
            write!(f, "{:?} {:?}", self.cond, self.inst_type)
        }
    }
}

impl<T: Into<BlockInstType>> From<T> for BlockInst {
    fn from(value: T) -> Self {
        BlockInst::new(Cond::AL, value.into())
    }
}

fn replace_reg(reg: &mut BlockReg, old: BlockReg, new: BlockReg) {
    if *reg == old {
        *reg = new;
    }
}

fn replace_operand(operand: &mut BlockOperand, old: BlockReg, new: BlockReg) {
    if let BlockOperand::Reg(reg) = operand {
        if *reg == old {
            *reg = new;
        }
    }
}

fn replace_shift_operands(operands: &mut [BlockOperandShift], old: BlockReg, new: BlockReg) {
    for operand in operands {
        operand.replace_regs(old, new);
    }
}

impl Alu {
    pub fn alu3(op: AluOp, operands: [BlockOperandShift; 3], set_cond: AluSetCond, thumb_pc_aligned: bool) -> Self {
        Alu {
            op,
            op_type: AluType::Alu3,
            operands,
            set_cond,
            thumb_pc_aligned,
        }
    }

    pub fn alu2(op: AluOp, operands: [BlockOperandShift; 2], set_cond: AluSetCond, thumb_pc_aligned: bool) -> Self {
        Alu {
            op,
            op_type: AluType::from(op),
            operands: [operands[0], operands[1], BlockOperandShift::default()],
            set_cond,
            thumb_pc_aligned,
        }
    }

    pub fn mul(operands: [BlockOperandShift; 3], set_cond: AluSetCond, thumb_pc_aligned: bool) -> Self {
        Alu {
            op: AluOp::And,
            op_type: AluType::Mul,
            operands,
            set_cond,
            thumb_pc_aligned,
        }
    }
}

impl BlockInstTrait for Alu {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        let mut outputs = BlockRegSet::new();
        match self.set_cond {
            AluSetCond::Host => outputs += BlockReg::Fixed(Reg::CPSR),
            AluSetCond::HostGuest => {
                outputs += BlockReg::from(Reg::CPSR);
                outputs += BlockReg::Fixed(Reg::CPSR);
            }
            _ => {}
        }

        match self.op_type {
            AluType::Alu3 | AluType::Mul => {
                outputs += self.operands[0].as_reg();
                (
                    block_reg_set!(Some(self.operands[1].as_reg()), self.operands[2].try_as_reg(), self.operands[2].try_as_shift_reg()),
                    outputs,
                )
            }
            AluType::Alu2Op1 => {
                debug_assert_ne!(self.set_cond, AluSetCond::None);
                (
                    block_reg_set!(Some(self.operands[0].as_reg()), self.operands[1].try_as_reg(), self.operands[1].try_as_shift_reg()),
                    outputs,
                )
            }
            AluType::Alu2Op0 => {
                outputs += self.operands[0].as_reg();
                (block_reg_set!(self.operands[1].try_as_reg(), self.operands[1].try_as_shift_reg()), outputs)
            }
        }
    }

    fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        match self.op_type {
            AluType::Alu3 | AluType::Mul => {
                self.operands[1].replace_regs(old, new);
                self.operands[2].replace_regs(old, new);
            }
            AluType::Alu2Op1 => replace_shift_operands(&mut self.operands[..2], old, new),
            AluType::Alu2Op0 => self.operands[1].replace_regs(old, new),
        }
    }

    fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg) {
        match self.op_type {
            AluType::Alu3 | AluType::Alu2Op0 | AluType::Mul => self.operands[0].replace_regs(old, new),
            AluType::Alu2Op1 => {}
        }
    }

    fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {
        let alu_reg = |op: AluOp, op0: BlockReg, op1: BlockReg, op2: BlockReg, shift: BlockShift, set_cond: bool| match shift.value {
            BlockOperand::Reg(shift_reg) => AluReg::generic(
                op as u8,
                alloc.for_emit_output(op0),
                alloc.for_emit_input(op1),
                alloc.for_emit_input(op2),
                shift.shift_type,
                alloc.for_emit_input(shift_reg),
                set_cond,
                Cond::AL,
            ),
            BlockOperand::Imm(shift_imm) => {
                debug_assert_eq!(shift_imm & !0x1F, 0);
                AluShiftImm::generic(
                    op as u8,
                    alloc.for_emit_output(op0),
                    alloc.for_emit_input(op1),
                    alloc.for_emit_input(op2),
                    shift.shift_type,
                    shift_imm as u8,
                    set_cond,
                    Cond::AL,
                )
            }
        };
        let alu_imm = |op: AluOp, op0: BlockReg, op1: BlockReg, op2: u32, shift: BlockShift, set_cond: bool| {
            debug_assert_eq!(op2 & !0xFF, 0);
            let shift_value = shift.value.as_imm();
            debug_assert_eq!(shift_value & !0xF, 0);
            debug_assert!(shift_value == 0 || shift.shift_type == ShiftType::Ror);
            AluImm::generic(op as u8, alloc.for_emit_output(op0), alloc.for_emit_input(op1), op2 as u8, shift_value as u8, set_cond, Cond::AL)
        };

        let Self { op, operands, set_cond, .. } = self;

        match self.op_type {
            AluType::Alu3 => match operands[2].operand {
                BlockOperand::Reg(reg) => opcodes.push(alu_reg(*op, operands[0].as_reg(), operands[1].as_reg(), reg, operands[2].shift, *set_cond != AluSetCond::None)),
                BlockOperand::Imm(imm) => opcodes.push(alu_imm(*op, operands[0].as_reg(), operands[1].as_reg(), imm, operands[2].shift, *set_cond != AluSetCond::None)),
            },
            AluType::Alu2Op1 => {
                debug_assert_ne!(*set_cond, AluSetCond::None);
                match operands[1].operand {
                    BlockOperand::Reg(reg) => opcodes.push(alu_reg(*op, BlockReg::Fixed(Reg::R0), operands[0].as_reg(), reg, operands[1].shift, true)),
                    BlockOperand::Imm(imm) => opcodes.push(alu_imm(*op, BlockReg::Fixed(Reg::R0), operands[0].as_reg(), imm, operands[1].shift, true)),
                }
            }
            AluType::Alu2Op0 => match operands[1].operand {
                BlockOperand::Reg(reg) => {
                    if *op == AluOp::Mov && operands[0].as_reg() == reg && operands[1].shift == BlockShift::default() && *set_cond == AluSetCond::None {
                        return;
                    }
                    opcodes.push(alu_reg(*op, operands[0].as_reg(), BlockReg::Fixed(Reg::R0), reg, operands[1].shift, *set_cond != AluSetCond::None))
                }
                BlockOperand::Imm(imm) => {
                    if *op == AluOp::Mov && operands[1].shift == BlockShift::default() && *set_cond == AluSetCond::None {
                        opcodes.extend(AluImm::mov32(alloc.for_emit_output(operands[0].as_reg()), imm))
                    } else {
                        opcodes.push(alu_imm(*op, operands[0].as_reg(), BlockReg::Fixed(Reg::R0), imm, operands[1].shift, *set_cond != AluSetCond::None))
                    }
                }
            },
            AluType::Mul => match operands[2].operand {
                BlockOperand::Reg(reg) => opcodes.push(MulReg::mul(
                    alloc.for_emit_output(operands[0].as_reg()),
                    alloc.for_emit_input(operands[1].as_reg()),
                    alloc.for_emit_input(reg),
                    *set_cond != AluSetCond::None,
                    Cond::AL,
                )),
                BlockOperand::Imm(_) => {
                    todo!()
                }
            },
        }
    }
}

impl Debug for Alu {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self {
            op,
            operands,
            set_cond,
            thumb_pc_aligned,
            ..
        } = self;
        match self.op_type {
            AluType::Alu3 => write!(f, "{op:?}{set_cond:?} {operands:?}, align pc: {thumb_pc_aligned}"),
            AluType::Alu2Op1 | AluType::Alu2Op0 => write!(f, "{op:?}{set_cond:?} [{:?}, {:?}], align pc: {thumb_pc_aligned}", operands[0], operands[1]),
            AluType::Mul => write!(f, "Mul{set_cond:?} {operands:?}, align pc: {thumb_pc_aligned}"),
        }
    }
}

impl BlockInstTrait for Transfer {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        match self.op {
            TransferOp::Read => (
                block_reg_set!(Some(self.operands[1].as_reg()), self.operands[2].try_as_reg(), self.operands[2].try_as_shift_reg()),
                block_reg_set!(Some(self.operands[0].as_reg())),
            ),
            TransferOp::Write => (
                block_reg_set!(
                    Some(self.operands[0].as_reg()),
                    Some(self.operands[1].as_reg()),
                    self.operands[2].try_as_reg(),
                    self.operands[2].try_as_shift_reg()
                ),
                block_reg_set!(),
            ),
        }
    }

    fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        if self.op == TransferOp::Write {
            self.operands[0].replace_regs(old, new);
        }
        self.operands[1].replace_regs(old, new);
        self.operands[2].replace_regs(old, new);
    }

    fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg) {
        if self.op == TransferOp::Read {
            self.operands[0].replace_regs(old, new);
        }
    }

    fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {
        let Self {
            op,
            operands,
            signed,
            amount,
            add_to_base,
        } = self;

        let op0 = match op {
            TransferOp::Read => alloc.for_emit_output(operands[0].as_reg()),
            TransferOp::Write => alloc.for_emit_input(operands[0].as_reg()),
        };

        opcodes.push(match operands[2].operand {
            BlockOperand::Reg(reg) => {
                let func = match amount {
                    MemoryAmount::Byte => |op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, shift_type: ShiftType, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                        if signed {
                            debug_assert_eq!(shift_amount, 0);
                            LdrStrRegSBHD::generic(op0, op1, op2, true, MemoryAmount::Byte, read, false, add_to_base, true, cond)
                        } else {
                            LdrStrReg::generic(op0, op1, op2, shift_amount, shift_type, read, false, true, add_to_base, true, cond)
                        }
                    },
                    MemoryAmount::Half => |op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, _: ShiftType, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                        debug_assert_eq!(shift_amount, 0);
                        LdrStrRegSBHD::generic(op0, op1, op2, signed, MemoryAmount::Half, read, false, add_to_base, true, cond)
                    },
                    MemoryAmount::Word => |op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, shift_type: ShiftType, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                        debug_assert!(!signed);
                        LdrStrReg::generic(op0, op1, op2, shift_amount, shift_type, read, false, false, add_to_base, true, cond)
                    },
                    MemoryAmount::Double => {
                        todo!()
                    }
                };
                let shift = operands[2].as_shift_imm();
                debug_assert_eq!(shift & !0x1F, 0);
                func(
                    op0,
                    alloc.for_emit_input(operands[1].as_reg()),
                    alloc.for_emit_input(reg),
                    shift as u8,
                    operands[2].shift.shift_type,
                    *signed,
                    *op == TransferOp::Read,
                    *add_to_base,
                    Cond::AL,
                )
            }
            BlockOperand::Imm(imm) => {
                let func = match amount {
                    MemoryAmount::Byte => |op0: Reg, op1: Reg, imm_offset: u16, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                        if signed {
                            debug_assert_eq!(imm_offset & !0xFF, 0);
                            LdrStrImmSBHD::generic(op0, op1, imm_offset as u8, true, MemoryAmount::Byte, read, false, true, true, cond)
                        } else {
                            LdrStrImm::generic(op0, op1, imm_offset, read, false, true, add_to_base, true, cond)
                        }
                    },
                    MemoryAmount::Half => |op0: Reg, op1: Reg, imm_offset: u16, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                        debug_assert_eq!(imm_offset & !0xFF, 0);
                        LdrStrImmSBHD::generic(op0, op1, imm_offset as u8, signed, MemoryAmount::Half, read, false, add_to_base, true, cond)
                    },
                    MemoryAmount::Word => |op0: Reg, op1: Reg, imm_offset: u16, signed: bool, read: bool, add_to_base: bool, cond: Cond| {
                        debug_assert!(!signed);
                        LdrStrImm::generic(op0, op1, imm_offset, read, false, false, add_to_base, true, cond)
                    },
                    MemoryAmount::Double => {
                        todo!()
                    }
                };
                func(op0, alloc.for_emit_input(operands[1].as_reg()), imm as u16, *signed, *op == TransferOp::Read, *add_to_base, Cond::AL)
            }
        });
    }
}

impl Debug for Transfer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self {
            op,
            operands,
            signed,
            amount,
            add_to_base,
        } = self;
        let signed = if *signed { "S" } else { "U" };
        let amount = match amount {
            MemoryAmount::Byte => "8",
            MemoryAmount::Half => "16",
            MemoryAmount::Word => "32",
            MemoryAmount::Double => "64",
        };
        let add_to_base = if *add_to_base { "+" } else { "-" };
        write!(f, "{op:?}{signed}{amount} {:?} [{:?}, {:?}], {add_to_base}base", operands[0], operands[1], operands[2])
    }
}

impl BlockInstTrait for TransferMultiple {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        match self.op {
            TransferOp::Read => (
                block_reg_set!(Some(self.operand)),
                if self.write_back {
                    BlockRegSet::new_fixed(self.regs) + self.operand
                } else {
                    BlockRegSet::new_fixed(self.regs)
                },
            ),
            TransferOp::Write => (
                BlockRegSet::new_fixed(self.regs) + self.operand,
                if self.write_back { block_reg_set!(Some(self.operand)) } else { block_reg_set!() },
            ),
        }
    }

    fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        replace_reg(&mut self.operand, old, new);
    }

    fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg) {
        if self.write_back {
            replace_reg(&mut self.operand, old, new);
        }
    }

    fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {
        opcodes.push(LdmStm::generic(
            if self.write_back {
                alloc.for_emit_output(self.operand)
            } else {
                alloc.for_emit_input(self.operand)
            },
            self.regs,
            self.op == TransferOp::Read,
            self.write_back,
            self.add_to_base,
            self.pre,
            Cond::AL,
        ))
    }
}

impl Debug for TransferMultiple {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self {
            op,
            operand,
            regs,
            write_back,
            pre,
            add_to_base,
        } = self;
        let add_to_base = if *add_to_base { "+" } else { "-" };
        write!(f, "{op:?}M {operand:?} {regs:?}, write back: {write_back}, pre {pre}, {add_to_base}base")
    }
}

impl BlockInstTrait for GuestTransferMultiple {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        match self.op {
            TransferOp::Read => {
                let mut outputs = BlockRegSet::new_fixed(self.fixed_regs);
                outputs.add_guests(self.gp_regs);
                (block_reg_set!(Some(self.addr_reg)), if self.write_back { outputs + self.addr_out_reg } else { outputs })
            }
            TransferOp::Write => {
                let mut inputs = BlockRegSet::new_fixed(self.fixed_regs);
                inputs.add_guests(self.gp_regs);
                (inputs + self.addr_reg, if self.write_back { block_reg_set!(Some(self.addr_out_reg)) } else { block_reg_set!() })
            }
        }
    }

    fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        replace_reg(&mut self.addr_reg, old, new)
    }

    fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg) {
        if self.write_back {
            replace_reg(&mut self.addr_out_reg, old, new);
        }
    }

    fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {
        let addr_reg = alloc.for_emit_input(self.addr_reg);
        if self.write_back && self.addr_reg != self.addr_out_reg {
            opcodes.push(AluShiftImm::mov_al(alloc.for_emit_output(self.addr_out_reg), addr_reg))
        }
        opcodes.push(LdmStm::generic(
            if self.write_back { alloc.for_emit_output(self.addr_out_reg) } else { addr_reg },
            self.gp_regs + self.fixed_regs,
            self.op == TransferOp::Read,
            self.write_back,
            self.add_to_base,
            self.pre,
            Cond::AL,
        ))
    }
}

impl Debug for GuestTransferMultiple {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self {
            op,
            addr_reg,
            addr_out_reg,
            gp_regs,
            fixed_regs,
            write_back,
            pre,
            add_to_base,
        } = self;
        let add_to_base = if *add_to_base { "+" } else { "-" };
        write!(
            f,
            "Guest{op:?}M {addr_reg:?} -> {addr_out_reg:?} gp regs: {gp_regs:?}, fixed regs: {fixed_regs:?}, write back: {write_back}, pre {pre}, {add_to_base}base"
        )
    }
}

impl BlockInstTrait for SystemReg {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        match self.op {
            SystemRegOp::Mrs => (block_reg_set!(), block_reg_set!(Some(self.operand.as_reg()))),
            SystemRegOp::Msr => (block_reg_set!(self.operand.try_as_reg()), block_reg_set!()),
        }
    }

    fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        if self.op == SystemRegOp::Msr {
            replace_operand(&mut self.operand, old, new);
        }
    }

    fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg) {
        if self.op == SystemRegOp::Mrs {
            replace_operand(&mut self.operand, old, new);
        }
    }

    fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {
        match self.op {
            SystemRegOp::Mrs => opcodes.push(Mrs::cpsr(alloc.for_emit_output(self.operand.as_reg()), Cond::AL)),
            SystemRegOp::Msr => opcodes.push(Msr::cpsr_flags(alloc.for_emit_input(self.operand.as_reg()), Cond::AL)),
        }
    }
}

impl Debug for SystemReg {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self { op, operand } = self;
        write!(f, "{op:?} {operand:?}")
    }
}

impl BitField {
    pub fn bfc(operand: BlockReg, lsb: u8, width: u8) -> Self {
        BitField {
            op: BitFieldOp::Bfc,
            operands: [operand, BlockReg::default()],
            lsb,
            width,
        }
    }

    pub fn bfi(operands: [BlockReg; 2], lsb: u8, width: u8) -> Self {
        BitField {
            op: BitFieldOp::Bfi,
            operands,
            lsb,
            width,
        }
    }

    pub fn ubfx(operands: [BlockReg; 2], lsb: u8, width: u8) -> Self {
        BitField {
            op: BitFieldOp::Ubfx,
            operands,
            lsb,
            width,
        }
    }
}

impl BlockInstTrait for BitField {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        match self.op {
            BitFieldOp::Bfc => (block_reg_set!(Some(self.operands[0])), block_reg_set!(Some(self.operands[0]))),
            BitFieldOp::Bfi | BitFieldOp::Ubfx => (block_reg_set!(Some(self.operands[0]), Some(self.operands[1])), block_reg_set!(Some(self.operands[0]))),
        }
    }

    fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        match self.op {
            BitFieldOp::Bfc => replace_reg(&mut self.operands[0], old, new),
            BitFieldOp::Bfi => {
                replace_reg(&mut self.operands[0], old, new);
                replace_reg(&mut self.operands[1], old, new);
            }
            BitFieldOp::Ubfx => replace_reg(&mut self.operands[1], old, new),
        }
    }

    fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg) {
        replace_reg(&mut self.operands[0], old, new)
    }

    fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {
        match self.op {
            BitFieldOp::Bfc => opcodes.push(Bfc::create(alloc.for_emit_output(self.operands[0]), self.lsb, self.width, Cond::AL)),
            BitFieldOp::Bfi => opcodes.push(Bfi::create(
                alloc.for_emit_output(self.operands[0]),
                alloc.for_emit_input(self.operands[1]),
                self.lsb,
                self.width,
                Cond::AL,
            )),
            BitFieldOp::Ubfx => opcodes.push(Ubfx::create(
                alloc.for_emit_output(self.operands[0]),
                alloc.for_emit_input(self.operands[1]),
                self.lsb,
                self.width,
                Cond::AL,
            )),
        }
    }
}

impl Debug for BitField {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self { op, operands, lsb, width } = self;
        match op {
            BitFieldOp::Bfc => write!(f, "{op:?} {:?}, {lsb}, {width}", operands[0]),
            BitFieldOp::Bfi | BitFieldOp::Ubfx => write!(f, "{op:?} {:?}, {:?}, {lsb}, {width}", operands[0], operands[1]),
        }
    }
}

impl SaveReg {
    fn save_guest_cpsr(opcodes: &mut Vec<u32>, thread_regs_addr_reg: Reg, host_reg: Reg) {
        opcodes.push(Mrs::cpsr(host_reg, Cond::AL));
        // Only copy the cond flags from host cpsr
        opcodes.push(AluShiftImm::mov(host_reg, host_reg, ShiftType::Lsr, 24, Cond::AL));
        opcodes.push(LdrStrImm::strb_offset_al(host_reg, thread_regs_addr_reg, Reg::CPSR as u16 * 4 + 3));
    }
}

impl BlockInstTrait for SaveReg {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        let mut inputs = BlockRegSet::new();
        let mut outputs = BlockRegSet::new();
        inputs += self.thread_regs_addr_reg;
        match self.guest_reg {
            Reg::CPSR => {
                inputs += BlockReg::Fixed(Reg::CPSR);
                outputs += self.tmp_host_cpsr_reg;
            }
            _ => inputs += self.reg_mapped,
        }
        (inputs, outputs)
    }

    fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        if self.guest_reg != Reg::CPSR {
            replace_reg(&mut self.reg_mapped, old, new);
        }
        replace_reg(&mut self.thread_regs_addr_reg, old, new)
    }

    fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg) {
        if self.guest_reg == Reg::CPSR {
            replace_reg(&mut self.tmp_host_cpsr_reg, old, new);
        }
    }

    fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {
        match self.guest_reg {
            Reg::CPSR => Self::save_guest_cpsr(opcodes, alloc.for_emit_input(self.thread_regs_addr_reg), alloc.for_emit_output(self.tmp_host_cpsr_reg)),
            _ => opcodes.push(LdrStrImm::str_offset_al(
                alloc.for_emit_input(self.reg_mapped),
                alloc.for_emit_input(self.thread_regs_addr_reg),
                self.guest_reg as u16 * 4,
            )),
        }
    }
}

impl Debug for SaveReg {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "SaveReg {:?}", self.guest_reg)
    }
}

impl BlockInstTrait for RestoreReg {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        let mut outputs = BlockRegSet::new();
        outputs += self.reg_mapped;
        if self.guest_reg == Reg::CPSR {
            outputs += self.tmp_guest_cpsr_reg;
            outputs += BlockReg::Fixed(Reg::CPSR);
        }
        (block_reg_set!(Some(self.thread_regs_addr_reg)), outputs)
    }

    fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        replace_reg(&mut self.thread_regs_addr_reg, old, new)
    }

    fn replace_output_regs(&mut self, old: BlockReg, new: BlockReg) {
        replace_reg(&mut self.reg_mapped, old, new);
        if self.guest_reg == Reg::CPSR {
            replace_reg(&mut self.tmp_guest_cpsr_reg, old, new);
        }
    }

    fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {
        match self.guest_reg {
            Reg::CPSR => {
                opcodes.push(LdrStrImm::ldr_offset_al(
                    alloc.for_emit_output(self.tmp_guest_cpsr_reg),
                    alloc.for_emit_input(self.thread_regs_addr_reg),
                    Reg::CPSR as u16 * 4,
                ));
                opcodes.push(Msr::cpsr_flags(alloc.for_emit_output(self.tmp_guest_cpsr_reg), Cond::AL));
            }
            _ => opcodes.push(LdrStrImm::ldr_offset_al(
                alloc.for_emit_output(self.reg_mapped),
                alloc.for_emit_input(self.thread_regs_addr_reg),
                self.guest_reg as u16 * 4,
            )),
        }
    }
}

impl Debug for RestoreReg {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "RestoreReg {:?}", self.guest_reg)
    }
}

impl GenericGuest {
    pub fn new(inst_info: &mut InstInfo) -> Self {
        GenericGuest {
            inst_info: GuestInstInfo::new(inst_info),
        }
    }
}

impl BlockInstTrait for GenericGuest {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        let mut inputs = BlockRegSet::new();
        let mut outputs = BlockRegSet::new();
        inputs.add_guests(self.inst_info.src_regs);
        outputs.add_guests(self.inst_info.out_regs);
        (inputs, outputs)
    }

    fn replace_input_regs(&mut self, _: BlockReg, _: BlockReg) {}

    fn replace_output_regs(&mut self, _: BlockReg, _: BlockReg) {}

    fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {
        let outputs = self.inst_info.out_regs;
        let replace_shift = |shift_value: &mut ShiftValue| {
            if let ShiftValue::Reg(reg) = shift_value {
                if *reg >= Reg::SP {
                    if outputs.is_reserved(*reg) {
                        *reg = alloc.for_emit_output(BlockReg::from(*reg));
                    } else {
                        *reg = alloc.for_emit_input(BlockReg::from(*reg));
                    }
                }
            }
        };

        for operand in self.inst_info.operands_mut() {
            if let Operand::Reg { reg, shift } = operand {
                if *reg >= Reg::SP {
                    if outputs.is_reserved(*reg) {
                        *reg = alloc.for_emit_output(BlockReg::from(*reg));
                    } else {
                        *reg = alloc.for_emit_input(BlockReg::from(*reg));
                    }
                }
                if let Some(shift) = shift {
                    match shift {
                        Shift::Lsl(v) => replace_shift(v),
                        Shift::Lsr(v) => replace_shift(v),
                        Shift::Asr(v) => replace_shift(v),
                        Shift::Ror(v) => replace_shift(v),
                    }
                }
            }
        }

        self.inst_info.set_cond(Cond::AL);
        opcodes.push(self.inst_info.assemble());
    }
}

impl Debug for GenericGuest {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.inst_info)
    }
}

impl Call {
    pub fn reg(reg: BlockReg, args: [Option<BlockReg>; 4], has_return: bool) -> Self {
        Call {
            op: CallOp::Reg(reg),
            args,
            has_return,
        }
    }

    pub fn offset(offset: usize, args: [Option<BlockReg>; 4], has_return: bool) -> Self {
        Call {
            op: CallOp::Offset(offset),
            args,
            has_return,
        }
    }
}

impl BlockInstTrait for Call {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        let mut inputs = BlockRegSet::new();
        if let CallOp::Reg(reg) = self.op {
            inputs += reg;
        }
        for &arg in self.args.iter().flatten() {
            inputs += arg;
        }
        (
            inputs,
            block_reg_set!(
                Some(BlockReg::Fixed(Reg::R0)),
                Some(BlockReg::Fixed(Reg::R1)),
                Some(BlockReg::Fixed(Reg::R2)),
                Some(BlockReg::Fixed(Reg::R3)),
                Some(BlockReg::Fixed(Reg::R12)),
                Some(BlockReg::Fixed(Reg::CPSR)),
                if self.has_return { Some(BlockReg::Fixed(Reg::LR)) } else { None }
            ),
        )
    }

    fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        if let CallOp::Reg(reg) = &mut self.op {
            replace_reg(reg, old, new)
        }
    }

    fn replace_output_regs(&mut self, _: BlockReg, _: BlockReg) {}

    fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, opcode_index: usize, placeholders: &mut BlockAsmPlaceholders) {
        match self.op {
            CallOp::Reg(reg) => opcodes.push(if self.has_return {
                Bx::blx(alloc.for_emit_input(reg), Cond::AL)
            } else {
                Bx::bx(alloc.for_emit_input(reg), Cond::AL)
            }),
            CallOp::Offset(offset) => {
                // Encode common offset
                // Branch offset can only be figured out later
                opcodes.push(BranchEncoding::new(u26::new(offset as u32), self.has_return, true, u4::new(Cond::AL as u8)).into());
                placeholders.branch.push(opcode_index);
            }
        }
    }
}

impl Debug for Call {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.op {
            CallOp::Reg(reg) => {
                if self.has_return {
                    write!(f, "Blx {reg:?} {:?}", self.args)
                } else {
                    write!(f, "Bx {reg:?} {:?}", self.args)
                }
            }
            CallOp::Offset(offset) => {
                if self.has_return {
                    write!(f, "Bl {offset:x} {:?}", self.args)
                } else {
                    write!(f, "B {offset:x} {:?}", self.args)
                }
            }
        }
    }
}

impl BlockInstTrait for Label {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        (block_reg_set!(), block_reg_set!())
    }
    fn replace_input_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn replace_output_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn emit_opcode(&mut self, _: &BlockRegAllocator, _: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {}
}

impl Debug for Label {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let guest_pc = match self.guest_pc {
            None => "",
            Some(pc) => &format!(" {pc:x}"),
        };
        write!(f, "Label {:?}{guest_pc} unlikely: {}", self.label, self.unlikely)
    }
}

impl BlockInstTrait for Branch {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        (block_reg_set!(), block_reg_set!())
    }
    fn replace_input_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn replace_output_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn emit_opcode(&mut self, _: &BlockRegAllocator, opcodes: &mut Vec<u32>, opcode_index: usize, placeholders: &mut BlockAsmPlaceholders) {
        // Encode label
        // Branch offset can only be figured out later
        opcodes.push(BranchEncoding::new(u26::new(self.block_index as u32), false, false, u4::new(Cond::AL as u8)).into());
        placeholders.branch.push(opcode_index);
    }
}

impl Debug for Branch {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "B {:?}, block index: {}", self.label, self.block_index)
    }
}

impl BlockInstTrait for Preload {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        (block_reg_set!(Some(self.operand)), block_reg_set!())
    }

    fn replace_input_regs(&mut self, old: BlockReg, new: BlockReg) {
        replace_reg(&mut self.operand, old, new)
    }

    fn replace_output_regs(&mut self, _: BlockReg, _: BlockReg) {}

    fn emit_opcode(&mut self, alloc: &BlockRegAllocator, opcodes: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {
        opcodes.push(transfer_assembler::Preload::pli(alloc.for_emit_input(self.operand), self.offset, self.add))
    }
}

impl Debug for Preload {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pli [{:?}, #{}{}]", self.operand, if self.add { "+" } else { "-" }, self.offset)
    }
}

impl BlockInstTrait for SaveContext {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        (block_reg_set!(), block_reg_set!())
    }
    fn replace_input_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn replace_output_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn emit_opcode(&mut self, _: &BlockRegAllocator, _: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {}
}

impl Debug for SaveContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "SaveContext")
    }
}

impl BlockInstTrait for GuestPc {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        (block_reg_set!(), block_reg_set!())
    }
    fn replace_input_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn replace_output_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn emit_opcode(&mut self, _: &BlockRegAllocator, _: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {}
}

impl Debug for GuestPc {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "GuestPc {:x}", self.0)
    }
}

impl BlockInstTrait for Epilogue {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        (
            block_reg_set!(Some(BlockReg::Fixed(Reg::SP))),
            block_reg_set!(Some(BlockReg::Fixed(Reg::SP)), Some(BlockReg::Fixed(if self.restore_state { Reg::LR } else { Reg::PC }))),
        )
    }
    fn replace_input_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn replace_output_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn emit_opcode(&mut self, _: &BlockRegAllocator, opcodes: &mut Vec<u32>, opcode_index: usize, placeholders: &mut BlockAsmPlaceholders) {
        opcodes.push(self.restore_all_regs as u32 | ((self.restore_state as u32) << 1));
        placeholders.epilogue.push(opcode_index);
    }
}

impl Debug for Epilogue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Epilogue restore all regs {}", self.restore_all_regs)
    }
}

impl BlockInstTrait for MarkRegDirty {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        if self.dirty {
            (block_reg_set!(Some(BlockReg::from(self.guest_reg))), block_reg_set!(Some(BlockReg::from(self.guest_reg))))
        } else {
            (block_reg_set!(Some(BlockReg::from(self.guest_reg))), block_reg_set!())
        }
    }
    fn replace_input_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn replace_output_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn emit_opcode(&mut self, _: &BlockRegAllocator, _: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {}
}

impl Debug for MarkRegDirty {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.dirty {
            write!(f, "Dirty {:?}", self.guest_reg)
        } else {
            write!(f, "Undirty {:?}", self.guest_reg)
        }
    }
}

impl BlockInstTrait for PadBlock {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        (block_reg_set!(), block_reg_set!())
    }
    fn replace_input_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn replace_output_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn emit_opcode(&mut self, _: &BlockRegAllocator, _: &mut Vec<u32>, _: usize, _: &mut BlockAsmPlaceholders) {}
}

impl Debug for PadBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "PadBlock {:?} {}", self.label, self.correction)
    }
}

impl BlockInstTrait for Generic {
    fn get_io(&self) -> (BlockRegSet, BlockRegSet) {
        match self {
            Generic::Prologue => (
                block_reg_set!(Some(BlockReg::Fixed(Reg::SP)), Some(BlockReg::Fixed(Reg::LR))),
                block_reg_set!(Some(BlockReg::Fixed(Reg::SP))),
            ),
            _ => (block_reg_set!(), block_reg_set!()),
        }
    }
    fn replace_input_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn replace_output_regs(&mut self, _: BlockReg, _: BlockReg) {}
    fn emit_opcode(&mut self, _: &BlockRegAllocator, opcodes: &mut Vec<u32>, opcode_index: usize, placeholders: &mut BlockAsmPlaceholders) {
        match self {
            Generic::Bkpt(id) => opcodes.push(Bkpt::bkpt(*id)),
            Generic::Nop => opcodes.push(AluShiftImm::mov_al(Reg::R0, Reg::R0)),
            Generic::Prologue => {
                opcodes.push(0);
                placeholders.prologue.push(opcode_index);
            }
        }
    }
}

impl Debug for Generic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Generic::Bkpt(id) => write!(f, "Bkpt {id}"),
            Generic::Nop => write!(f, "Nop"),
            Generic::Prologue => write!(f, "Prologue"),
        }
    }
}
