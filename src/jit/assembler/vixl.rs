use crate::jit;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::Cond;
use std::pin::Pin;
use std::{ptr, slice};
use vixl::*;

pub mod vixl {
    #![allow(warnings, unused)]
    include!(concat!(env!("OUT_DIR"), "/vixl_bindings.rs"));
}

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

impl From<ShiftType> for Shift {
    fn from(value: u32) -> Self {
        Shift { shift_: value }
    }
}

impl From<(Reg, jit::ShiftType, Reg)> for Operand {
    fn from((reg, shift_type, shift_reg): (Reg, jit::ShiftType, Reg)) -> Self {
        unsafe { Operand::new5(reg.into(), Shift { shift_: shift_type as _ }, shift_reg.into()) }
    }
}

impl From<(Reg, jit::ShiftType, u8)> for Operand {
    fn from((reg, shift_type, shift_imm): (Reg, jit::ShiftType, u8)) -> Self {
        unsafe { Operand::new4(reg.into(), Shift { shift_: shift_type as _ }, shift_imm as _) }
    }
}

impl MemOperand {
    pub fn reg(reg: Reg) -> Self {
        unsafe { MemOperand::new(reg.into(), AddrMode_Offset) }
    }

    pub fn reg_offset(reg: Reg, offset: i32) -> Self {
        unsafe { MemOperand::new1(reg.into(), offset, AddrMode_Offset) }
    }

    pub fn reg_offset2(reg: Reg, reg_offset: Reg) -> Self {
        unsafe { MemOperand::new4(reg.into(), reg_offset.into(), AddrMode_Offset) }
    }
}

impl From<RegReserve> for RegisterList {
    fn from(value: RegReserve) -> Self {
        RegisterList { list_: value.0 }
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

impl WriteBack {
    pub const fn no() -> Self {
        WriteBack { value_: WriteBackValue_NO_WRITE_BACK }
    }

    pub const fn yes() -> Self {
        WriteBack { value_: WriteBackValue_WRITE_BACK }
    }
}

pub struct MacroAssembler {
    inner: *mut Aarch32MacroAssembler,
}

impl MacroAssembler {
    pub fn new() -> Self {
        MacroAssembler {
            inner: unsafe { create_aarch32_masm() },
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
}

impl Drop for MacroAssembler {
    fn drop(&mut self) {
        if !self.inner.is_null() {
            unsafe { destroy_aarch32_masm(self.inner) };
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/vixl_inst_wrapper.rs"));

impl MasmLdr2<Reg, u32> for MacroAssembler {
    fn ldr2(&mut self, reg: Reg, v: u32) {
        self.ldr3(Cond::AL, reg, v)
    }
}
