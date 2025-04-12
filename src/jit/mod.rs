use crate::jit::op::Op;
use std::marker::ConstParamTy;
use std::{mem, ops};

pub mod assembler;
pub mod disassembler;
mod emitter;
mod inst_branch_handler;
mod inst_cp15_handler;
mod inst_cpu_regs_handler;
mod inst_exception_handler;
pub mod inst_info;
mod inst_info_thumb;
mod inst_mem_handler;
mod inst_thread_regs_handler;
pub mod jit_asm;
mod jit_asm_common_funs;
pub mod jit_memory;
mod jit_memory_map;
pub mod op;
pub mod reg;
mod analyzer;

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

#[repr(u8)]
#[derive(Copy, Clone, ConstParamTy, Debug, PartialEq, Eq)]
pub enum ShiftType {
    Lsl = 0,
    Lsr = 1,
    Asr = 2,
    Ror = 3,
}

#[repr(u8)]
#[derive(Copy, Clone, ConstParamTy, Debug, PartialEq, Eq)]
pub enum MemoryAmount {
    Byte = 0,
    Half = 1,
    Word = 2,
    Double = 3,
}

impl MemoryAmount {
    pub const fn size(self) -> u8 {
        1 << (self as u8)
    }
}

impl From<Op> for MemoryAmount {
    fn from(op: Op) -> Self {
        match op {
            Op::Ldr(single_transfer) | Op::LdrT(single_transfer) | Op::Str(single_transfer) | Op::StrT(single_transfer) => match single_transfer.size() {
                0 => MemoryAmount::Byte,
                1 => MemoryAmount::Half,
                2 => MemoryAmount::Word,
                3 => MemoryAmount::Double,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }
}

impl From<u8> for MemoryAmount {
    fn from(value: u8) -> Self {
        debug_assert!(value <= MemoryAmount::Double as u8);
        unsafe { mem::transmute(value) }
    }
}
