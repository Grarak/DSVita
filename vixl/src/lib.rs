#![feature(core_intrinsics)]
#![allow(warnings, unused)]

use std::fmt::{Debug, Formatter};
use std::intrinsics::unlikely;
use std::pin::Pin;
use std::{iter, mem, ops, ptr, slice};

#[derive(Copy, Clone, Debug, Default, Eq, Hash, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Reg {
    R0 = 0,
    R1 = 1,
    R2 = 2,
    R3 = 3,
    R4 = 4,
    R5 = 5,
    R6 = 6,
    R7 = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    SP = 13,
    LR = 14,
    PC = 15,
    CPSR = 16,
    SPSR = 17,
    #[default]
    None = 18,
}

impl From<u8> for Reg {
    fn from(value: u8) -> Self {
        debug_assert!(value < Reg::None as u8);
        unsafe { mem::transmute(value) }
    }
}

impl Reg {
    pub const fn is_call_preserved(self) -> bool {
        (self as u8 >= Reg::R4 as u8 && self as u8 <= Reg::R11 as u8) || self as u8 == Reg::SP as u8
    }

    pub const fn is_low(self) -> bool {
        self as u8 <= Reg::R7 as u8
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Cond {
    EQ = 0,
    NE = 1,
    HS = 2,
    LO = 3,
    MI = 4,
    PL = 5,
    VS = 6,
    VC = 7,
    HI = 8,
    LS = 9,
    GE = 10,
    LT = 11,
    GT = 12,
    LE = 13,
    AL = 14,
    NV = 15,
}

impl From<u8> for Cond {
    fn from(value: u8) -> Self {
        unsafe { mem::transmute(value) }
    }
}

impl ops::Not for Cond {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Cond::EQ => Cond::NE,
            Cond::NE => Cond::EQ,
            Cond::HS => Cond::LO,
            Cond::LO => Cond::HS,
            Cond::MI => Cond::PL,
            Cond::PL => Cond::MI,
            Cond::VS => Cond::VC,
            Cond::VC => Cond::VS,
            Cond::HI => Cond::LS,
            Cond::LS => Cond::HI,
            Cond::GE => Cond::LT,
            Cond::LT => Cond::GE,
            Cond::GT => Cond::LE,
            Cond::LE => Cond::GT,
            Cond::AL => Cond::NV,
            Cond::NV => Cond::AL,
        }
    }
}

const GP_REGS_BITMASK: u32 = 0x1FFF;
const GP_LR_REGS_BITMASK: u32 = 0x5FFF;
const GP_THUMB_REGS_BITMASK: u32 = 0xFF;

#[derive(Copy, Clone, Default, Eq, PartialEq)]
pub struct RegReserve(pub u32);

impl RegReserve {
    pub const fn new() -> Self {
        RegReserve(0)
    }

    pub const fn gp() -> Self {
        RegReserve(GP_REGS_BITMASK)
    }

    pub const fn all() -> Self {
        RegReserve(0xFFFF)
    }

    pub fn gp_thumb() -> Self {
        RegReserve(GP_THUMB_REGS_BITMASK)
    }

    pub const fn reserve(&mut self, reg: Reg) {
        self.0 |= 1 << (reg as u8);
    }

    pub fn is_reserved(self, reg: Reg) -> bool {
        (self.0 >> reg as u8) & 1 == 1
    }

    pub fn next_gp_free(self) -> Option<Reg> {
        let count = self.0.trailing_ones();
        if count >= Reg::SP as u32 { None } else { Some(Reg::from(count as u8)) }
    }

    pub fn peek_gp(self) -> Option<Reg> {
        let count = self.0.trailing_zeros();
        if count >= Reg::SP as u32 { None } else { Some(Reg::from(count as u8)) }
    }

    pub fn peek(self) -> Option<Reg> {
        let count = self.0.trailing_zeros();
        if count >= Reg::CPSR as u32 { None } else { Some(Reg::from(count as u8)) }
    }

    pub const fn len(self) -> usize {
        u32::count_ones(self.0) as _
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn get_gp_regs(self) -> RegReserve {
        RegReserve(self.0 & GP_REGS_BITMASK)
    }

    pub const fn get_gp_lr_regs(self) -> RegReserve {
        RegReserve(self.0 & GP_LR_REGS_BITMASK)
    }

    pub fn clear(&mut self) {
        self.0 = 0;
    }

    pub fn get_highest_reg(self) -> Reg {
        Reg::from(32 - self.0.leading_zeros() as u8 - 1)
    }

    pub fn get_lowest_reg(self) -> Reg {
        Reg::from(self.0.trailing_zeros() as u8)
    }
}

impl<'a> iter::Sum<&'a Reg> for RegReserve {
    fn sum<I: Iterator<Item = &'a Reg>>(iter: I) -> Self {
        let mut reg_reserve = RegReserve::new();
        for reg in iter {
            reg_reserve += *reg;
        }
        reg_reserve
    }
}

impl ops::Add<RegReserve> for RegReserve {
    type Output = Self;

    fn add(self, rhs: RegReserve) -> Self::Output {
        RegReserve(self.0 | rhs.0)
    }
}

impl ops::AddAssign<RegReserve> for RegReserve {
    fn add_assign(&mut self, rhs: RegReserve) {
        self.0 |= rhs.0;
    }
}

impl ops::Sub<RegReserve> for RegReserve {
    type Output = Self;

    fn sub(self, rhs: RegReserve) -> Self::Output {
        RegReserve(self.0 & (!rhs.0))
    }
}

impl ops::SubAssign<RegReserve> for RegReserve {
    fn sub_assign(&mut self, rhs: RegReserve) {
        self.0 &= !rhs.0;
    }
}

impl ops::BitXor<RegReserve> for RegReserve {
    type Output = Self;

    fn bitxor(self, rhs: RegReserve) -> Self::Output {
        RegReserve(self.0 ^ rhs.0)
    }
}

impl ops::BitXorAssign<RegReserve> for RegReserve {
    fn bitxor_assign(&mut self, rhs: RegReserve) {
        self.0 ^= rhs.0;
    }
}

impl ops::BitAnd<RegReserve> for RegReserve {
    type Output = Self;

    fn bitand(self, rhs: RegReserve) -> Self::Output {
        RegReserve(self.0 & rhs.0)
    }
}

impl ops::BitAndAssign<RegReserve> for RegReserve {
    fn bitand_assign(&mut self, rhs: RegReserve) {
        self.0 &= rhs.0;
    }
}

impl ops::Not for RegReserve {
    type Output = RegReserve;

    fn not(self) -> Self::Output {
        RegReserve(!self.0)
    }
}

impl ops::Add<Reg> for RegReserve {
    type Output = RegReserve;

    fn add(self, rhs: Reg) -> Self::Output {
        RegReserve(self.0 | (1 << rhs as u8))
    }
}

impl ops::AddAssign<Reg> for RegReserve {
    fn add_assign(&mut self, rhs: Reg) {
        self.0 |= 1 << rhs as u8;
    }
}

impl ops::Sub<Reg> for RegReserve {
    type Output = RegReserve;

    fn sub(self, rhs: Reg) -> Self::Output {
        RegReserve(self.0 & !(1 << rhs as u8))
    }
}

impl ops::SubAssign<Reg> for RegReserve {
    fn sub_assign(&mut self, rhs: Reg) {
        self.0 &= !(1 << rhs as u8)
    }
}

impl ops::BitAnd<Reg> for RegReserve {
    type Output = RegReserve;

    fn bitand(self, rhs: Reg) -> Self::Output {
        RegReserve(self.0 & (1 << rhs as u8))
    }
}

impl ops::BitAndAssign<Reg> for RegReserve {
    fn bitand_assign(&mut self, rhs: Reg) {
        self.0 &= 1 << rhs as u8;
    }
}

impl ops::BitOrAssign for RegReserve {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl From<u32> for RegReserve {
    fn from(value: u32) -> Self {
        RegReserve(value)
    }
}

impl Debug for RegReserve {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug_set = f.debug_set();
        for i in Reg::R0 as u8..Reg::None as u8 {
            let reg = Reg::from(i);
            if self.is_reserved(reg) {
                debug_set.entry(&reg);
            }
        }
        debug_set.finish()
    }
}

impl IntoIterator for RegReserve {
    type Item = Reg;
    type IntoIter = RegReserveIter;

    fn into_iter(self) -> Self::IntoIter {
        RegReserveIter { reserve: self.0.reverse_bits() }
    }
}

#[derive(Clone)]
pub struct RegReserveIter {
    reserve: u32,
}

impl Iterator for RegReserveIter {
    type Item = <RegReserve as IntoIterator>::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if unlikely(self.reserve == 0) {
            None
        } else {
            let zeros = self.reserve.leading_zeros();
            let reg = Reg::from(zeros as u8);
            self.reserve &= !(0x80000000 >> zeros);
            Some(reg)
        }
    }
}

impl FromIterator<Reg> for RegReserve {
    fn from_iter<T: IntoIterator<Item = Reg>>(iter: T) -> Self {
        let mut reg_reserve = RegReserve::new();
        for reg in iter {
            reg_reserve += reg;
        }
        reg_reserve
    }
}

include!(concat!(env!("OUT_DIR"), "/vixl_bindings.rs"));
include!(concat!(env!("OUT_DIR"), "/vixl_inst_wrapper.rs"));

pub struct DOperand {
    inner: *mut Aarch32DOperand,
}

pub struct QOperand {
    inner: *mut Aarch32DOperand,
}

pub struct SOperand {
    inner: *mut Aarch32DOperand,
}

pub struct RawLiteral {
    inner: *mut Aarch32RawLiteral,
    should_destroy: bool,
    value: Pin<Box<u32>>,
}

impl Drop for RawLiteral {
    fn drop(&mut self) {
        if self.should_destroy {
            unsafe { destroy_aarch32_raw_literal(self.inner) };
        }
    }
}

impl From<u32> for RawLiteral {
    fn from(value: u32) -> Self {
        let mut literal = RawLiteral {
            inner: ptr::null_mut(),
            should_destroy: false,
            value: Box::pin(value),
        };
        let addr = literal.value.as_ref().get_ref() as *const u32;
        literal.inner = unsafe { create_aarch32_raw_literal(addr as _, size_of::<u32>() as _, PlacementPolicy_kPlacedWhenUsed, DeletionPolicy_kDeletedOnPlacementByPool) };
        literal
    }
}

pub struct Label {
    inner: *mut Aarch32Label,
}

impl Label {
    pub fn new() -> Self {
        Label {
            inner: unsafe { create_aarch32_label() },
        }
    }
}

impl Drop for Label {
    fn drop(&mut self) {
        unsafe { destroy_aarch32_label(self.inner) }
    }
}

impl From<u32> for MaskedSpecialRegister {
    fn from(value: u32) -> Self {
        unsafe { MaskedSpecialRegister::new(value) }
    }
}

impl From<u32> for SpecialRegister {
    fn from(value: u32) -> Self {
        SpecialRegister { reg_: value }
    }
}

impl From<u8> for Operand {
    fn from(value: u8) -> Self {
        (value as u32).into()
    }
}

impl From<u16> for Operand {
    fn from(value: u16) -> Self {
        (value as u32).into()
    }
}

impl From<u32> for Operand {
    fn from(value: u32) -> Self {
        unsafe { Operand::new(value) }
    }
}

impl From<i32> for Operand {
    fn from(value: i32) -> Self {
        unsafe { Operand::new1(value) }
    }
}

impl From<Reg> for Register {
    fn from(value: Reg) -> Self {
        debug_assert!(value as u8 <= Reg::PC as u8, "{value:?} <= {:?}", Reg::PC);
        unsafe { Register::new(value as u32) }
    }
}

impl From<Cond> for Condition {
    fn from(value: Cond) -> Self {
        unsafe { Condition::new(value as u32) }
    }
}

impl From<Reg> for Operand {
    fn from(value: Reg) -> Self {
        unsafe { Operand::new2(value.into()) }
    }
}

impl From<ShiftType> for Shift {
    fn from(value: u32) -> Self {
        Shift { shift_: value }
    }
}

impl From<Reg> for MemOperand {
    fn from(value: Reg) -> Self {
        unsafe { MemOperand::new(value.into(), AddrMode_Offset) }
    }
}

impl From<(Reg, i32)> for MemOperand {
    fn from((reg, offset): (Reg, i32)) -> Self {
        unsafe { MemOperand::new1(reg.into(), offset, AddrMode_Offset) }
    }
}

impl From<(Reg, Reg)> for MemOperand {
    fn from((reg, reg_offset): (Reg, Reg)) -> Self {
        unsafe { MemOperand::new4(reg.into(), reg_offset.into(), AddrMode_Offset) }
    }
}

impl From<RegReserve> for RegisterList {
    fn from(value: RegReserve) -> Self {
        RegisterList { list_: value.0 }
    }
}

impl WriteBack {
    pub const fn no() -> Self {
        WriteBack { value_: WriteBackValue_NO_WRITE_BACK }
    }

    pub const fn yes() -> Self {
        WriteBack { value_: WriteBackValue_WRITE_BACK }
    }
}

impl CPURegister {
    pub fn get_type(self) -> CPURegister_RegisterType {
        (self.value_ & 0x1E0) >> 5
    }

    pub fn get_code(self) -> u32 {
        self.value_ & 0x1F
    }
}

pub struct MacroAssembler {
    inner: *mut Aarch32MacroAssembler,
    isa: InstructionSet,
}

impl MacroAssembler {
    pub fn new(isa: InstructionSet) -> Self {
        MacroAssembler {
            inner: unsafe { create_aarch32_masm(isa) },
            isa,
        }
    }

    pub fn bind(&mut self, label: &mut Label) {
        unsafe { masm_bind(self.inner, label.inner) }
    }

    pub fn finalize(&mut self) {
        unsafe { masm_finalize(self.inner) }
    }

    pub fn get_code_buffer(&self) -> &[u8] {
        let ptr = unsafe { masm_get_start_address(self.inner) };
        let size = unsafe { masm_get_size_of_code_generated(self.inner) };
        unsafe { slice::from_raw_parts(ptr, size as usize) }
    }

    pub fn get_cursor_offset(&self) -> u32 {
        unsafe { masm_get_cursor_offset(self.inner) }
    }

    pub fn ensure_emit_for(&mut self, size: u32) {
        unsafe { masm_ensure_emit_for(self.inner, size) }
    }
}

impl Drop for MacroAssembler {
    fn drop(&mut self) {
        if !self.inner.is_null() {
            unsafe { destroy_aarch32_masm(self.inner) };
        }
    }
}

impl MasmLdr2<Reg, u32> for MacroAssembler {
    fn ldr2(&mut self, reg: Reg, v: u32) {
        if self.isa == InstructionSet_T32 && reg.is_low() {
            self.ldr3(Cond::AL, reg, v)
        } else {
            self.mov4(FlagsUpdate_LeaveFlags, Cond::AL, reg, &v.into());
        }
    }
}
