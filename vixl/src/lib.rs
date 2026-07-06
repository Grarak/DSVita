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
        if count >= Reg::SP as u32 {
            None
        } else {
            Some(Reg::from(count as u8))
        }
    }

    pub fn peek_gp(self) -> Option<Reg> {
        let count = self.0.trailing_zeros();
        if count >= Reg::SP as u32 {
            None
        } else {
            Some(Reg::from(count as u8))
        }
    }

    pub fn peek(self) -> Option<Reg> {
        let count = self.0.trailing_zeros();
        if count >= Reg::CPSR as u32 {
            None
        } else {
            Some(Reg::from(count as u8))
        }
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

// Everything below depends on the generated aarch32 bindings.
#[cfg(target_arch = "arm")]
mod aarch32_glue {
    use super::*;

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
            let single_mov = v & 0xFFFF == v || {
                let shifted = v >> v.trailing_zeros();
                shifted & 0x1F == shifted
            };
            if !single_mov && (self.isa == InstructionSet_T32 && reg.is_low()) {
                self.ldr3(Cond::AL, reg, v)
            } else {
                self.mov4(FlagsUpdate_LeaveFlags, Cond::AL, reg, &v.into());
            }
        }
    }
}
#[cfg(target_arch = "arm")]
pub use aarch32_glue::*;

#[cfg(target_arch = "aarch64")]
mod aarch64_glue {
    use super::Cond;
    use std::slice;

    include!(concat!(env!("OUT_DIR"), "/vixl_a64_bindings.rs"));

    /// A64 host registers. Codes 0-30 are x/w registers, ZR and SP both encode 31 in the
    /// instruction but are distinct operands — the shim layer maps them onto vixl's
    /// xzr/wzr and sp/wsp.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    #[repr(u32)]
    pub enum A64Reg {
        X0 = 0,
        X1 = 1,
        X2 = 2,
        X3 = 3,
        X4 = 4,
        X5 = 5,
        X6 = 6,
        X7 = 7,
        X8 = 8,
        X9 = 9,
        X10 = 10,
        X11 = 11,
        X12 = 12,
        X13 = 13,
        X14 = 14,
        X15 = 15,
        X16 = 16,
        X17 = 17,
        X18 = 18,
        X19 = 19,
        X20 = 20,
        X21 = 21,
        X22 = 22,
        X23 = 23,
        X24 = 24,
        X25 = 25,
        X26 = 26,
        X27 = 27,
        X28 = 28,
        X29 = 29,
        X30 = 30,
        ZR = 31,
        SP = 32,
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    #[repr(i32)]
    pub enum A64ShiftKind {
        LSL = A64Shift_A64_LSL as i32,
        LSR = A64Shift_A64_LSR as i32,
        ASR = A64Shift_A64_ASR as i32,
        ROR = A64Shift_A64_ROR as i32,
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    #[repr(i32)]
    pub enum A64ExtendKind {
        UXTB = A64Extend_A64_UXTB as i32,
        UXTH = A64Extend_A64_UXTH as i32,
        UXTW = A64Extend_A64_UXTW as i32,
        UXTX = A64Extend_A64_UXTX as i32,
        SXTB = A64Extend_A64_SXTB as i32,
        SXTH = A64Extend_A64_SXTH as i32,
        SXTW = A64Extend_A64_SXTW as i32,
        SXTX = A64Extend_A64_SXTX as i32,
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    #[repr(i32)]
    pub enum A64AddrModeKind {
        Offset = A64AddrMode_A64_OFFSET as i32,
        PreIndex = A64AddrMode_A64_PREINDEX as i32,
        PostIndex = A64AddrMode_A64_POSTINDEX as i32,
    }

    pub struct A64Label {
        inner: *mut Aarch64Label,
    }

    impl A64Label {
        pub fn new() -> Self {
            A64Label {
                inner: unsafe { create_aarch64_label() },
            }
        }
    }

    impl Drop for A64Label {
        fn drop(&mut self) {
            unsafe { destroy_aarch64_label(self.inner) };
        }
    }

    /// A pool literal, registered with `masm`'s literal pool at creation — keep it alive
    /// until after `finalize()` (the pool holds a pointer until placement).
    pub struct A64LiteralU64 {
        inner: *mut Aarch64LiteralU64,
    }

    impl A64LiteralU64 {
        pub fn new(masm: &mut A64MacroAssembler, value: u64) -> Self {
            A64LiteralU64 {
                inner: unsafe { create_aarch64_literal_u64(masm.inner, value) },
            }
        }
    }

    impl Drop for A64LiteralU64 {
        fn drop(&mut self) {
            unsafe { destroy_aarch64_literal_u64(self.inner) };
        }
    }

    pub struct A64MacroAssembler {
        inner: *mut Aarch64MacroAssembler,
    }

    /// Blocks literal-pool/veneer emission for exactly `size` bytes — wrap fastmem windows
    /// and patchable sequences so recorded offsets stay exact.
    pub struct A64ExactScope {
        inner: *mut Aarch64ExactAssemblyScope,
    }

    impl Drop for A64ExactScope {
        fn drop(&mut self) {
            unsafe { destroy_aarch64_exact_scope(self.inner) };
        }
    }

    // One thin method per shim. `r()` maps the enum; conditions reuse the crate-wide Cond.
    fn r(reg: A64Reg) -> u32 {
        reg as u32
    }

    impl A64MacroAssembler {
        pub fn new() -> Self {
            A64MacroAssembler {
                inner: unsafe { create_aarch64_masm() },
            }
        }

        pub fn exact_scope(&mut self, size: usize) -> A64ExactScope {
            A64ExactScope {
                inner: unsafe { create_aarch64_exact_scope(self.inner, size) },
            }
        }

        pub fn bind(&mut self, label: &mut A64Label) {
            unsafe { masm_a64_bind(self.inner, label.inner) }
        }

        pub fn finalize(&mut self) {
            unsafe { masm_a64_finalize(self.inner) }
        }

        pub fn get_code_buffer(&self) -> &[u8] {
            let ptr = unsafe { masm_a64_get_start_address(self.inner) };
            let size = unsafe { masm_a64_get_size_of_code_generated(self.inner) };
            unsafe { slice::from_raw_parts(ptr, size as usize) }
        }

        pub fn get_cursor_offset(&self) -> u32 {
            unsafe { masm_a64_get_cursor_offset(self.inner) }
        }

        pub fn ensure_emit_for(&mut self, size: u32) {
            unsafe { masm_a64_ensure_emit_for(self.inner, size) }
        }

        pub fn mov_imm64(&mut self, rd: A64Reg, imm: u64) {
            unsafe { masm_a64_mov_imm(self.inner, r(rd), 1, imm) }
        }

        pub fn mov_imm32(&mut self, rd: A64Reg, imm: u32) {
            unsafe { masm_a64_mov_imm(self.inner, r(rd), 0, imm as u64) }
        }

        pub fn movk(&mut self, rd: A64Reg, imm: u16, shift: u32) {
            unsafe { masm_a64_movk(self.inner, r(rd), imm as u64, shift as i32) }
        }

        pub fn mov_reg(&mut self, rd: A64Reg, rn: A64Reg, is64: bool) {
            unsafe { masm_a64_mov_reg(self.inner, r(rd), r(rn), is64 as i32) }
        }

        pub fn ldr_literal(&mut self, rt: A64Reg, literal: &mut A64LiteralU64) {
            unsafe { masm_a64_ldr_literal(self.inner, r(rt), literal.inner) }
        }

        pub fn b(&mut self, label: &mut A64Label) {
            unsafe { masm_a64_b(self.inner, label.inner) }
        }

        pub fn b_cond(&mut self, label: &mut A64Label, cond: Cond) {
            unsafe { masm_a64_b_cond(self.inner, label.inner, cond as u32) }
        }

        pub fn bl(&mut self, label: &mut A64Label) {
            unsafe { masm_a64_bl(self.inner, label.inner) }
        }

        pub fn cbz(&mut self, rt: A64Reg, is64: bool, label: &mut A64Label) {
            unsafe { masm_a64_cbz(self.inner, r(rt), is64 as i32, label.inner) }
        }

        pub fn cbnz(&mut self, rt: A64Reg, is64: bool, label: &mut A64Label) {
            unsafe { masm_a64_cbnz(self.inner, r(rt), is64 as i32, label.inner) }
        }

        pub fn tbz(&mut self, rt: A64Reg, bit: u32, label: &mut A64Label) {
            unsafe { masm_a64_tbz(self.inner, r(rt), bit, label.inner) }
        }

        pub fn tbnz(&mut self, rt: A64Reg, bit: u32, label: &mut A64Label) {
            unsafe { masm_a64_tbnz(self.inner, r(rt), bit, label.inner) }
        }

        pub fn br(&mut self, rn: A64Reg) {
            unsafe { masm_a64_br(self.inner, r(rn)) }
        }

        pub fn blr(&mut self, rn: A64Reg) {
            unsafe { masm_a64_blr(self.inner, r(rn)) }
        }

        pub fn ret(&mut self) {
            unsafe { masm_a64_ret(self.inner, 30) }
        }

        pub fn adr(&mut self, rd: A64Reg, label: &mut A64Label) {
            unsafe { masm_a64_adr(self.inner, r(rd), label.inner) }
        }

        pub fn mrs_nzcv(&mut self, rt: A64Reg) {
            unsafe { masm_a64_mrs_nzcv(self.inner, r(rt)) }
        }

        pub fn msr_nzcv(&mut self, rt: A64Reg) {
            unsafe { masm_a64_msr_nzcv(self.inner, r(rt)) }
        }

        pub fn nop(&mut self) {
            unsafe { masm_a64_nop(self.inner) }
        }

        /// Assembler-level nop — the form allowed inside an exact scope.
        pub fn raw_nop(&mut self) {
            unsafe { masm_a64_raw_nop(self.inner) }
        }

        pub fn brk(&mut self, code: u32) {
            unsafe { masm_a64_brk(self.inner, code) }
        }

        pub fn csel(&mut self, rd: A64Reg, rn: A64Reg, rm: A64Reg, cond: Cond, is64: bool) {
            unsafe { masm_a64_csel(self.inner, r(rd), r(rn), r(rm), cond as u32, is64 as i32) }
        }

        pub fn cset(&mut self, rd: A64Reg, cond: Cond, is64: bool) {
            unsafe { masm_a64_cset(self.inner, r(rd), cond as u32, is64 as i32) }
        }

        pub fn csinc(&mut self, rd: A64Reg, rn: A64Reg, rm: A64Reg, cond: Cond, is64: bool) {
            unsafe { masm_a64_csinc(self.inner, r(rd), r(rn), r(rm), cond as u32, is64 as i32) }
        }

        pub fn ubfx(&mut self, rd: A64Reg, rn: A64Reg, lsb: u32, width: u32, is64: bool) {
            unsafe { masm_a64_ubfx(self.inner, r(rd), r(rn), lsb, width, is64 as i32) }
        }

        pub fn sbfx(&mut self, rd: A64Reg, rn: A64Reg, lsb: u32, width: u32, is64: bool) {
            unsafe { masm_a64_sbfx(self.inner, r(rd), r(rn), lsb, width, is64 as i32) }
        }

        pub fn bfi(&mut self, rd: A64Reg, rn: A64Reg, lsb: u32, width: u32, is64: bool) {
            unsafe { masm_a64_bfi(self.inner, r(rd), r(rn), lsb, width, is64 as i32) }
        }

        pub fn ldp(&mut self, rt: A64Reg, rt2: A64Reg, is64: bool, base: A64Reg, offset: i64, mode: A64AddrModeKind) {
            unsafe { masm_a64_ldp(self.inner, r(rt), r(rt2), is64 as i32, r(base), offset, mode as i32) }
        }

        pub fn stp(&mut self, rt: A64Reg, rt2: A64Reg, is64: bool, base: A64Reg, offset: i64, mode: A64AddrModeKind) {
            unsafe { masm_a64_stp(self.inner, r(rt), r(rt2), is64 as i32, r(base), offset, mode as i32) }
        }

        pub fn ldr_off(&mut self, rt: A64Reg, is64: bool, base: A64Reg, offset: i64, mode: A64AddrModeKind) {
            unsafe { masm_a64_ldr_off(self.inner, r(rt), is64 as i32, r(base), offset, mode as i32) }
        }

        pub fn str_off(&mut self, rt: A64Reg, is64: bool, base: A64Reg, offset: i64, mode: A64AddrModeKind) {
            unsafe { masm_a64_str_off(self.inner, r(rt), is64 as i32, r(base), offset, mode as i32) }
        }

        pub fn ldrh_off(&mut self, rt: A64Reg, base: A64Reg, offset: i64, mode: A64AddrModeKind) {
            unsafe { masm_a64_ldrh_off(self.inner, r(rt), r(base), offset, mode as i32) }
        }

        pub fn strh_off(&mut self, rt: A64Reg, base: A64Reg, offset: i64, mode: A64AddrModeKind) {
            unsafe { masm_a64_strh_off(self.inner, r(rt), r(base), offset, mode as i32) }
        }

        pub fn ldr_regoff(&mut self, rt: A64Reg, is64: bool, base: A64Reg, index: A64Reg, extend: A64ExtendKind, amount: u32) {
            unsafe { masm_a64_ldr_regoff(self.inner, r(rt), is64 as i32, r(base), r(index), extend as i32, amount) }
        }

        pub fn str_regoff(&mut self, rt: A64Reg, is64: bool, base: A64Reg, index: A64Reg, extend: A64ExtendKind, amount: u32) {
            unsafe { masm_a64_str_regoff(self.inner, r(rt), is64 as i32, r(base), r(index), extend as i32, amount) }
        }
    }

    impl Drop for A64MacroAssembler {
        fn drop(&mut self) {
            if !self.inner.is_null() {
                unsafe { destroy_aarch64_masm(self.inner) };
            }
        }
    }

    macro_rules! alu_imm_method {
        ($name:ident, $shim:ident) => {
            impl A64MacroAssembler {
                pub fn $name(&mut self, rd: A64Reg, rn: A64Reg, imm: u64, is64: bool) {
                    unsafe { $shim(self.inner, rd as u32, rn as u32, imm, is64 as i32) }
                }
            }
        };
    }

    alu_imm_method!(add_imm, masm_a64_add_imm);
    alu_imm_method!(adds_imm, masm_a64_adds_imm);
    alu_imm_method!(sub_imm, masm_a64_sub_imm);
    alu_imm_method!(subs_imm, masm_a64_subs_imm);
    alu_imm_method!(and_imm, masm_a64_and_imm);
    alu_imm_method!(ands_imm, masm_a64_ands_imm);
    alu_imm_method!(orr_imm, masm_a64_orr_imm);
    alu_imm_method!(eor_imm, masm_a64_eor_imm);

    macro_rules! alu_reg_method {
        ($name:ident, $shim:ident) => {
            impl A64MacroAssembler {
                pub fn $name(&mut self, rd: A64Reg, rn: A64Reg, rm: A64Reg, shift: A64ShiftKind, amount: u32, is64: bool) {
                    unsafe { $shim(self.inner, rd as u32, rn as u32, rm as u32, shift as i32, amount, is64 as i32) }
                }
            }
        };
    }

    alu_reg_method!(add_reg, masm_a64_add_reg);
    alu_reg_method!(adds_reg, masm_a64_adds_reg);
    alu_reg_method!(sub_reg, masm_a64_sub_reg);
    alu_reg_method!(subs_reg, masm_a64_subs_reg);
    alu_reg_method!(and_reg, masm_a64_and_reg);
    alu_reg_method!(ands_reg, masm_a64_ands_reg);
    alu_reg_method!(orr_reg, masm_a64_orr_reg);
    alu_reg_method!(eor_reg, masm_a64_eor_reg);
    alu_reg_method!(bic_reg, masm_a64_bic_reg);

    macro_rules! carry_method {
        ($name:ident, $shim:ident) => {
            impl A64MacroAssembler {
                pub fn $name(&mut self, rd: A64Reg, rn: A64Reg, rm: A64Reg, is64: bool) {
                    unsafe { $shim(self.inner, rd as u32, rn as u32, rm as u32, is64 as i32) }
                }
            }
        };
    }

    carry_method!(adc, masm_a64_adc);
    carry_method!(adcs, masm_a64_adcs);
    carry_method!(sbc, masm_a64_sbc);
    carry_method!(sbcs, masm_a64_sbcs);

    impl A64MacroAssembler {
        pub fn cmp_imm(&mut self, rn: A64Reg, imm: u64, is64: bool) {
            unsafe { masm_a64_cmp_imm(self.inner, r(rn), imm, is64 as i32) }
        }

        pub fn cmp_reg(&mut self, rn: A64Reg, rm: A64Reg, shift: A64ShiftKind, amount: u32, is64: bool) {
            unsafe { masm_a64_cmp_reg(self.inner, r(rn), r(rm), shift as i32, amount, is64 as i32) }
        }

        pub fn tst_imm(&mut self, rn: A64Reg, imm: u64, is64: bool) {
            unsafe { masm_a64_tst_imm(self.inner, r(rn), imm, is64 as i32) }
        }

        pub fn tst_reg(&mut self, rn: A64Reg, rm: A64Reg, shift: A64ShiftKind, amount: u32, is64: bool) {
            unsafe { masm_a64_tst_reg(self.inner, r(rn), r(rm), shift as i32, amount, is64 as i32) }
        }

        pub fn lsl_imm(&mut self, rd: A64Reg, rn: A64Reg, shift: u32, is64: bool) {
            unsafe { masm_a64_lsl_imm(self.inner, r(rd), r(rn), shift, is64 as i32) }
        }

        pub fn lsr_imm(&mut self, rd: A64Reg, rn: A64Reg, shift: u32, is64: bool) {
            unsafe { masm_a64_lsr_imm(self.inner, r(rd), r(rn), shift, is64 as i32) }
        }

        pub fn asr_imm(&mut self, rd: A64Reg, rn: A64Reg, shift: u32, is64: bool) {
            unsafe { masm_a64_asr_imm(self.inner, r(rd), r(rn), shift, is64 as i32) }
        }

        pub fn ror_imm(&mut self, rd: A64Reg, rn: A64Reg, shift: u32, is64: bool) {
            unsafe { masm_a64_ror_imm(self.inner, r(rd), r(rn), shift, is64 as i32) }
        }
    }
}

#[cfg(target_arch = "aarch64")]
pub use aarch64_glue::*;
