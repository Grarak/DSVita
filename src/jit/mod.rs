use crate::jit::op::Op;
use std::marker::ConstParamTy;
use std::{mem, ops};

pub mod assembler;
pub mod disassembler;
mod emitter;
pub mod inst_info;
mod inst_info_thumb;
mod inst_mem_handler;
pub mod jit_asm;
pub mod jit_memory;
pub mod op;
pub mod reg;
mod inst_exception_handler;
mod inst_threag_regs_handler;
mod inst_cp15_handler;
mod inst_cpu_regs_handler;

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
#[derive(ConstParamTy, PartialEq, Eq)]
pub enum ShiftType {
    Lsl = 0,
    Lsr = 1,
    Asr = 2,
    Ror = 3,
}

#[repr(u8)]
#[derive(ConstParamTy, PartialEq, Eq)]
pub enum MemoryAmount {
    Byte,
    Half,
    Word,
    Double,
}

impl From<Op> for MemoryAmount {
    fn from(value: Op) -> Self {
        match value {
            Op::LdrsbOfim
            | Op::LdrsbOfip
            | Op::LdrsbOfrm
            | Op::LdrsbOfrp
            | Op::LdrsbPrim
            | Op::LdrsbPrip
            | Op::LdrsbPrrm
            | Op::LdrsbPrrp
            | Op::LdrsbPtim
            | Op::LdrsbPtip
            | Op::LdrsbPtrm
            | Op::LdrsbPtrp
            | Op::LdrbOfim
            | Op::LdrbOfip
            | Op::LdrbOfrmar
            | Op::LdrbOfrmll
            | Op::LdrbOfrmlr
            | Op::LdrbOfrmrr
            | Op::LdrbOfrpar
            | Op::LdrbOfrpll
            | Op::LdrbOfrplr
            | Op::LdrbOfrprr
            | Op::LdrbPrim
            | Op::LdrbPrip
            | Op::LdrbPrrmar
            | Op::LdrbPrrmll
            | Op::LdrbPrrmlr
            | Op::LdrbPrrmrr
            | Op::LdrbPrrpar
            | Op::LdrbPrrpll
            | Op::LdrbPrrplr
            | Op::LdrbPrrprr
            | Op::LdrbPtim
            | Op::LdrbPtip
            | Op::LdrbPtrmar
            | Op::LdrbPtrmll
            | Op::LdrbPtrmlr
            | Op::LdrbPtrmrr
            | Op::LdrbPtrpar
            | Op::LdrbPtrpll
            | Op::LdrbPtrplr
            | Op::LdrbPtrprr
            | Op::StrbOfim
            | Op::StrbOfip
            | Op::StrbOfrmar
            | Op::StrbOfrmll
            | Op::StrbOfrmlr
            | Op::StrbOfrmrr
            | Op::StrbOfrpar
            | Op::StrbOfrpll
            | Op::StrbOfrplr
            | Op::StrbOfrprr
            | Op::StrbPrim
            | Op::StrbPrip
            | Op::StrbPrrmar
            | Op::StrbPrrmll
            | Op::StrbPrrmlr
            | Op::StrbPrrmrr
            | Op::StrbPrrpar
            | Op::StrbPrrpll
            | Op::StrbPrrplr
            | Op::StrbPrrprr
            | Op::StrbPtim
            | Op::StrbPtip
            | Op::StrbPtrmar
            | Op::StrbPtrmll
            | Op::StrbPtrmlr
            | Op::StrbPtrmrr
            | Op::StrbPtrpar
            | Op::StrbPtrpll
            | Op::StrbPtrplr
            | Op::StrbPtrprr
            | Op::LdrsbRegT
            | Op::LdrbRegT
            | Op::LdrbImm5T
            | Op::StrbRegT
            | Op::StrbImm5T => MemoryAmount::Byte,
            Op::LdrshOfim
            | Op::LdrshOfip
            | Op::LdrshOfrm
            | Op::LdrshOfrp
            | Op::LdrshPrim
            | Op::LdrshPrip
            | Op::LdrshPrrm
            | Op::LdrshPrrp
            | Op::LdrshPtim
            | Op::LdrshPtip
            | Op::LdrshPtrm
            | Op::LdrshPtrp
            | Op::LdrhOfim
            | Op::LdrhOfip
            | Op::LdrhOfrm
            | Op::LdrhOfrp
            | Op::LdrhPrim
            | Op::LdrhPrip
            | Op::LdrhPrrm
            | Op::LdrhPrrp
            | Op::LdrhPtim
            | Op::LdrhPtip
            | Op::LdrhPtrm
            | Op::LdrhPtrp
            | Op::StrhOfim
            | Op::StrhOfip
            | Op::StrhOfrm
            | Op::StrhOfrp
            | Op::StrhPrim
            | Op::StrhPrip
            | Op::StrhPrrm
            | Op::StrhPrrp
            | Op::StrhPtim
            | Op::StrhPtip
            | Op::StrhPtrm
            | Op::StrhPtrp
            | Op::LdrshRegT
            | Op::LdrhRegT
            | Op::LdrhImm5T
            | Op::StrhRegT
            | Op::StrhImm5T => MemoryAmount::Half,
            Op::LdrOfim
            | Op::LdrOfip
            | Op::LdrOfrmar
            | Op::LdrOfrmll
            | Op::LdrOfrmlr
            | Op::LdrOfrmrr
            | Op::LdrOfrpar
            | Op::LdrOfrpll
            | Op::LdrOfrplr
            | Op::LdrOfrprr
            | Op::LdrPrim
            | Op::LdrPrip
            | Op::LdrPrrmar
            | Op::LdrPrrmll
            | Op::LdrPrrmlr
            | Op::LdrPrrmrr
            | Op::LdrPrrpar
            | Op::LdrPrrpll
            | Op::LdrPrrplr
            | Op::LdrPrrprr
            | Op::LdrPtim
            | Op::LdrPtip
            | Op::LdrPtrmar
            | Op::LdrPtrmll
            | Op::LdrPtrmlr
            | Op::LdrPtrmrr
            | Op::LdrPtrpar
            | Op::LdrPtrpll
            | Op::LdrPtrplr
            | Op::LdrPtrprr
            | Op::StrOfim
            | Op::StrOfip
            | Op::StrOfrmar
            | Op::StrOfrmll
            | Op::StrOfrmlr
            | Op::StrOfrmrr
            | Op::StrOfrpar
            | Op::StrOfrpll
            | Op::StrOfrplr
            | Op::StrOfrprr
            | Op::StrPrim
            | Op::StrPrip
            | Op::StrPrrmar
            | Op::StrPrrmll
            | Op::StrPrrmlr
            | Op::StrPrrmrr
            | Op::StrPrrpar
            | Op::StrPrrpll
            | Op::StrPrrplr
            | Op::StrPrrprr
            | Op::StrPtim
            | Op::StrPtip
            | Op::StrPtrmar
            | Op::StrPtrmll
            | Op::StrPtrmlr
            | Op::StrPtrmrr
            | Op::StrPtrpar
            | Op::StrPtrpll
            | Op::StrPtrplr
            | Op::StrPtrprr
            | Op::LdrRegT
            | Op::LdrImm5T
            | Op::LdrPcT
            | Op::LdrSpT
            | Op::StrRegT
            | Op::StrImm5T
            | Op::StrSpT => MemoryAmount::Word,
            Op::LdrdOfim
            | Op::LdrdOfip
            | Op::LdrdOfrm
            | Op::LdrdOfrp
            | Op::LdrdPrim
            | Op::LdrdPrip
            | Op::LdrdPrrm
            | Op::LdrdPrrp
            | Op::LdrdPtim
            | Op::LdrdPtip
            | Op::LdrdPtrm
            | Op::LdrdPtrp
            | Op::StrdOfim
            | Op::StrdOfip
            | Op::StrdOfrm
            | Op::StrdOfrp
            | Op::StrdPrim
            | Op::StrdPrip
            | Op::StrdPrrm
            | Op::StrdPrrp
            | Op::StrdPtim
            | Op::StrdPtip
            | Op::StrdPtrm
            | Op::StrdPtrp => MemoryAmount::Double,
            _ => todo!("{:?}", value),
        }
    }
}

impl From<u8> for MemoryAmount {
    fn from(value: u8) -> Self {
        debug_assert!(value <= MemoryAmount::Double as u8);
        unsafe { mem::transmute(value) }
    }
}
