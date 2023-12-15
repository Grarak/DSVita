use crate::jit::Op;

pub mod indirect_mem_handler;
pub mod indirect_mem_multiple_handler;

pub trait Convert: Copy + Into<u32> {
    fn from(value: u32) -> Self;
}

impl Convert for u8 {
    fn from(value: u32) -> Self {
        value as u8
    }
}

impl Convert for u16 {
    fn from(value: u32) -> Self {
        value as u16
    }
}

impl Convert for u32 {
    fn from(value: u32) -> Self {
        value
    }
}

enum MemoryAmount {
    BYTE,
    HALF,
    WORD,
    DOUBLE,
}

impl From<Op> for MemoryAmount {
    fn from(value: Op) -> Self {
        match value {
            Op::LdrbOfim
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
            | Op::StrbPtrprr => MemoryAmount::BYTE,
            Op::LdrhOfim
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
            | Op::StrhPtrp => MemoryAmount::HALF,
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
            | Op::LdrsbOfim
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
            | Op::LdrshOfim
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
            | Op::LdrPcT => MemoryAmount::WORD,
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
            | Op::StrdPtrp => MemoryAmount::DOUBLE,
            _ => todo!("{:?}", value),
        }
    }
}
