use crate::jit::reg::Reg;
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
        debug_assert!(value as u8 <= Reg::PC as u8);
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
        unsafe { Operand::new(value.into()) }
    }
}

pub struct MarcoAssembler {
    inner: *mut Aarch32MacroAssembler,
}

impl MarcoAssembler {
    pub fn new() -> Self {
        MarcoAssembler {
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
}

impl Drop for MarcoAssembler {
    fn drop(&mut self) {
        unsafe { destroy_aarch32_masm(self.inner) };
    }
}

include!(concat!(env!("OUT_DIR"), "/vixl_inst_wrapper.rs"));

impl MasmLdr2<Reg, u32> for MarcoAssembler {
    fn ldr2(&mut self, reg: Reg, v: u32) {
        self.ldr3(Cond::AL, reg, v)
    }
}
