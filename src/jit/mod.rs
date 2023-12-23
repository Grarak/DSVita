use std::{mem, ops};

pub mod assembler;
pub mod disassembler;
mod emitter;
pub mod inst_info;
mod inst_info_thumb;
mod inst_mem_handler;
pub mod jit_asm;
pub mod jit_memory;
pub mod reg;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum Op {
    AdcAri,
    AdcArr,
    AdcImm,
    AdcLli,
    AdcLlr,
    AdcLri,
    AdcLrr,
    AdcRri,
    AdcRrr,
    AdcsAri,
    AdcsArr,
    AdcsImm,
    AdcsLli,
    AdcsLlr,
    AdcsLri,
    AdcsLrr,
    AdcsRri,
    AdcsRrr,
    AddAri,
    AddArr,
    AddImm,
    AddLli,
    AddLlr,
    AddLri,
    AddLrr,
    AddRri,
    AddRrr,
    AddsAri,
    AddsArr,
    AddsImm,
    AddsLli,
    AddsLlr,
    AddsLri,
    AddsLrr,
    AddsRri,
    AddsRrr,
    AndAri,
    AndArr,
    AndImm,
    AndLli,
    AndLlr,
    AndLri,
    AndLrr,
    AndRri,
    AndRrr,
    AndsAri,
    AndsArr,
    AndsImm,
    AndsLli,
    AndsLlr,
    AndsLri,
    AndsLrr,
    AndsRri,
    AndsRrr,
    B,
    BicAri,
    BicArr,
    BicImm,
    BicLli,
    BicLlr,
    BicLri,
    BicLrr,
    BicRri,
    BicRrr,
    BicsAri,
    BicsArr,
    BicsImm,
    BicsLli,
    BicsLlr,
    BicsLri,
    BicsLrr,
    BicsRri,
    BicsRrr,
    Bl,
    BlxReg,
    Bx,
    Clz,
    CmnAri,
    CmnArr,
    CmnImm,
    CmnLli,
    CmnLlr,
    CmnLri,
    CmnLrr,
    CmnRri,
    CmnRrr,
    CmpAri,
    CmpArr,
    CmpImm,
    CmpLli,
    CmpLlr,
    CmpLri,
    CmpLrr,
    CmpRri,
    CmpRrr,
    EorAri,
    EorArr,
    EorImm,
    EorLli,
    EorLlr,
    EorLri,
    EorLrr,
    EorRri,
    EorRrr,
    EorsAri,
    EorsArr,
    EorsImm,
    EorsLli,
    EorsLlr,
    EorsLri,
    EorsLrr,
    EorsRri,
    EorsRrr,
    Ldmda,
    LdmdaU,
    LdmdaUW,
    LdmdaW,
    Ldmdb,
    LdmdbU,
    LdmdbUW,
    LdmdbW,
    Ldmia,
    LdmiaU,
    LdmiaUW,
    LdmiaW,
    Ldmib,
    LdmibU,
    LdmibUW,
    LdmibW,
    LdrOfim,
    LdrOfip,
    LdrOfrmar,
    LdrOfrmll,
    LdrOfrmlr,
    LdrOfrmrr,
    LdrOfrpar,
    LdrOfrpll,
    LdrOfrplr,
    LdrOfrprr,
    LdrPrim,
    LdrPrip,
    LdrPrrmar,
    LdrPrrmll,
    LdrPrrmlr,
    LdrPrrmrr,
    LdrPrrpar,
    LdrPrrpll,
    LdrPrrplr,
    LdrPrrprr,
    LdrPtim,
    LdrPtip,
    LdrPtrmar,
    LdrPtrmll,
    LdrPtrmlr,
    LdrPtrmrr,
    LdrPtrpar,
    LdrPtrpll,
    LdrPtrplr,
    LdrPtrprr,
    LdrbOfim,
    LdrbOfip,
    LdrbOfrmar,
    LdrbOfrmll,
    LdrbOfrmlr,
    LdrbOfrmrr,
    LdrbOfrpar,
    LdrbOfrpll,
    LdrbOfrplr,
    LdrbOfrprr,
    LdrbPrim,
    LdrbPrip,
    LdrbPrrmar,
    LdrbPrrmll,
    LdrbPrrmlr,
    LdrbPrrmrr,
    LdrbPrrpar,
    LdrbPrrpll,
    LdrbPrrplr,
    LdrbPrrprr,
    LdrbPtim,
    LdrbPtip,
    LdrbPtrmar,
    LdrbPtrmll,
    LdrbPtrmlr,
    LdrbPtrmrr,
    LdrbPtrpar,
    LdrbPtrpll,
    LdrbPtrplr,
    LdrbPtrprr,
    LdrdOfim,
    LdrdOfip,
    LdrdOfrm,
    LdrdOfrp,
    LdrdPrim,
    LdrdPrip,
    LdrdPrrm,
    LdrdPrrp,
    LdrdPtim,
    LdrdPtip,
    LdrdPtrm,
    LdrdPtrp,
    LdrhOfim,
    LdrhOfip,
    LdrhOfrm,
    LdrhOfrp,
    LdrhPrim,
    LdrhPrip,
    LdrhPrrm,
    LdrhPrrp,
    LdrhPtim,
    LdrhPtip,
    LdrhPtrm,
    LdrhPtrp,
    LdrsbOfim,
    LdrsbOfip,
    LdrsbOfrm,
    LdrsbOfrp,
    LdrsbPrim,
    LdrsbPrip,
    LdrsbPrrm,
    LdrsbPrrp,
    LdrsbPtim,
    LdrsbPtip,
    LdrsbPtrm,
    LdrsbPtrp,
    LdrshOfim,
    LdrshOfip,
    LdrshOfrm,
    LdrshOfrp,
    LdrshPrim,
    LdrshPrip,
    LdrshPrrm,
    LdrshPrrp,
    LdrshPtim,
    LdrshPtip,
    LdrshPtrm,
    LdrshPtrp,
    Mcr,
    Mla,
    Mlas,
    MovAri,
    MovArr,
    MovImm,
    MovLli,
    MovLlr,
    MovLri,
    MovLrr,
    MovRri,
    MovRrr,
    MovsAri,
    MovsArr,
    MovsImm,
    MovsLli,
    MovsLlr,
    MovsLri,
    MovsLrr,
    MovsRri,
    MovsRrr,
    Mrc,
    MrsRc,
    MrsRs,
    MsrIc,
    MsrIs,
    MsrRc,
    MsrRs,
    Mul,
    Muls,
    MvnAri,
    MvnArr,
    MvnImm,
    MvnLli,
    MvnLlr,
    MvnLri,
    MvnLrr,
    MvnRri,
    MvnRrr,
    MvnsAri,
    MvnsArr,
    MvnsImm,
    MvnsLli,
    MvnsLlr,
    MvnsLri,
    MvnsLrr,
    MvnsRri,
    MvnsRrr,
    OrrAri,
    OrrArr,
    OrrImm,
    OrrLli,
    OrrLlr,
    OrrLri,
    OrrLrr,
    OrrRri,
    OrrRrr,
    OrrsAri,
    OrrsArr,
    OrrsImm,
    OrrsLli,
    OrrsLlr,
    OrrsLri,
    OrrsLrr,
    OrrsRri,
    OrrsRrr,
    Qadd,
    Qdadd,
    Qdsub,
    Qsub,
    RsbAri,
    RsbArr,
    RsbImm,
    RsbLli,
    RsbLlr,
    RsbLri,
    RsbLrr,
    RsbRri,
    RsbRrr,
    RsbsAri,
    RsbsArr,
    RsbsImm,
    RsbsLli,
    RsbsLlr,
    RsbsLri,
    RsbsLrr,
    RsbsRri,
    RsbsRrr,
    RscAri,
    RscArr,
    RscImm,
    RscLli,
    RscLlr,
    RscLri,
    RscLrr,
    RscRri,
    RscRrr,
    RscsAri,
    RscsArr,
    RscsImm,
    RscsLli,
    RscsLlr,
    RscsLri,
    RscsLrr,
    RscsRri,
    RscsRrr,
    SbcAri,
    SbcArr,
    SbcImm,
    SbcLli,
    SbcLlr,
    SbcLri,
    SbcLrr,
    SbcRri,
    SbcRrr,
    SbcsAri,
    SbcsArr,
    SbcsImm,
    SbcsLli,
    SbcsLlr,
    SbcsLri,
    SbcsLrr,
    SbcsRri,
    SbcsRrr,
    Smlabb,
    Smlabt,
    Smlal,
    Smlalbb,
    Smlalbt,
    Smlals,
    Smlaltb,
    Smlaltt,
    Smlatb,
    Smlatt,
    Smlawb,
    Smlawt,
    Smulbb,
    Smulbt,
    Smull,
    Smulls,
    Smultb,
    Smultt,
    Smulwb,
    Smulwt,
    Stmda,
    StmdaU,
    StmdaUW,
    StmdaW,
    Stmdb,
    StmdbU,
    StmdbUW,
    StmdbW,
    Stmia,
    StmiaU,
    StmiaUW,
    StmiaW,
    Stmib,
    StmibU,
    StmibUW,
    StmibW,
    StrOfim,
    StrOfip,
    StrOfrmar,
    StrOfrmll,
    StrOfrmlr,
    StrOfrmrr,
    StrOfrpar,
    StrOfrpll,
    StrOfrplr,
    StrOfrprr,
    StrPrim,
    StrPrip,
    StrPrrmar,
    StrPrrmll,
    StrPrrmlr,
    StrPrrmrr,
    StrPrrpar,
    StrPrrpll,
    StrPrrplr,
    StrPrrprr,
    StrPtim,
    StrPtip,
    StrPtrmar,
    StrPtrmll,
    StrPtrmlr,
    StrPtrmrr,
    StrPtrpar,
    StrPtrpll,
    StrPtrplr,
    StrPtrprr,
    StrbOfim,
    StrbOfip,
    StrbOfrmar,
    StrbOfrmll,
    StrbOfrmlr,
    StrbOfrmrr,
    StrbOfrpar,
    StrbOfrpll,
    StrbOfrplr,
    StrbOfrprr,
    StrbPrim,
    StrbPrip,
    StrbPrrmar,
    StrbPrrmll,
    StrbPrrmlr,
    StrbPrrmrr,
    StrbPrrpar,
    StrbPrrpll,
    StrbPrrplr,
    StrbPrrprr,
    StrbPtim,
    StrbPtip,
    StrbPtrmar,
    StrbPtrmll,
    StrbPtrmlr,
    StrbPtrmrr,
    StrbPtrpar,
    StrbPtrpll,
    StrbPtrplr,
    StrbPtrprr,
    StrdOfim,
    StrdOfip,
    StrdOfrm,
    StrdOfrp,
    StrdPrim,
    StrdPrip,
    StrdPrrm,
    StrdPrrp,
    StrdPtim,
    StrdPtip,
    StrdPtrm,
    StrdPtrp,
    StrhOfim,
    StrhOfip,
    StrhOfrm,
    StrhOfrp,
    StrhPrim,
    StrhPrip,
    StrhPrrm,
    StrhPrrp,
    StrhPtim,
    StrhPtip,
    StrhPtrm,
    StrhPtrp,
    SubAri,
    SubArr,
    SubImm,
    SubLli,
    SubLlr,
    SubLri,
    SubLrr,
    SubRri,
    SubRrr,
    SubsAri,
    SubsArr,
    SubsImm,
    SubsLli,
    SubsLlr,
    SubsLri,
    SubsLrr,
    SubsRri,
    SubsRrr,
    Swi,
    Swp,
    Swpb,
    TeqAri,
    TeqArr,
    TeqImm,
    TeqLli,
    TeqLlr,
    TeqLri,
    TeqLrr,
    TeqRri,
    TeqRrr,
    TstAri,
    TstArr,
    TstImm,
    TstLli,
    TstLlr,
    TstLri,
    TstLrr,
    TstRri,
    TstRrr,
    Umlal,
    Umlals,
    Umull,
    Umulls,
    UnkArm,

    // Thumb
    AdcDpT,
    AddHT,
    AddImm3T,
    AddImm8T,
    AddPcT,
    AddRegT,
    AddSpImmT,
    AddSpT,
    AndDpT,
    AsrDpT,
    AsrImmT,
    BT,
    BccT,
    BcsT,
    BeqT,
    BgeT,
    BgtT,
    BhiT,
    BicDpT,
    BlOffT,
    BlSetupT,
    BleT,
    BlsT,
    BltT,
    BlxOffT,
    BlxRegT,
    BmiT,
    BneT,
    BplT,
    BvcT,
    BvsT,
    BxRegT,
    CmnDpT,
    CmpDpT,
    CmpHT,
    CmpImm8T,
    EorDpT,
    LdmiaT,
    LdrImm5T,
    LdrPcT,
    LdrRegT,
    LdrSpT,
    LdrbImm5T,
    LdrbRegT,
    LdrhImm5T,
    LdrhRegT,
    LdrsbRegT,
    LdrshRegT,
    LslDpT,
    LslImmT,
    LsrDpT,
    LsrImmT,
    MovHT,
    MovImm8T,
    MulDpT,
    MvnDpT,
    NegDpT,
    OrrDpT,
    PopPcT,
    PopT,
    PushLrT,
    PushT,
    RorDpT,
    SbcDpT,
    StmiaT,
    StrImm5T,
    StrRegT,
    StrSpT,
    StrbImm5T,
    StrbRegT,
    StrhImm5T,
    StrhRegT,
    SubImm3T,
    SubImm8T,
    SubRegT,
    SwiT,
    TstDpT,
    UnkThumb,
}

impl Op {
    pub const fn is_branch(&self) -> bool {
        match self {
            Op::Bx | Op::BlxReg | Op::B | Op::Bl => true,
            _ => false,
        }
    }

    pub const fn is_branch_thumb(&self) -> bool {
        match self {
            Op::BxRegT
            | Op::BlxRegT
            | Op::BT
            | Op::BeqT
            | Op::BneT
            | Op::BcsT
            | Op::BccT
            | Op::BmiT
            | Op::BplT
            | Op::BvsT
            | Op::BvcT
            | Op::BhiT
            | Op::BlsT
            | Op::BgeT
            | Op::BltT
            | Op::BgtT
            | Op::BleT
            | Op::BlSetupT
            | Op::BlOffT
            | Op::BlxOffT => true,
            _ => false,
        }
    }

    pub const fn is_unconditional_branch_thumb(&self) -> bool {
        match self {
            Op::BxRegT | Op::BlxRegT | Op::BT | Op::BlOffT | Op::BlxOffT => true,
            _ => false,
        }
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

impl<T: Into<u8>> From<T> for Cond {
    fn from(value: T) -> Self {
        let value = value.into();
        assert!(value <= Cond::AL as u8);
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
pub enum ShiftType {
    LSL = 0,
    LSR = 1,
    ASR = 2,
    ROR = 3,
}

pub enum MemoryAmount {
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
            | Op::StrbPtrprr
            | Op::StrbImm5T => MemoryAmount::BYTE,
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
            | Op::StrhPtrp
            | Op::LdrhImm5T
            | Op::StrhImm5T => MemoryAmount::HALF,
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
            | Op::LdrImm5T
            | Op::LdrPcT
            | Op::LdrSpT
            | Op::StrImm5T
            | Op::StrSpT => MemoryAmount::WORD,
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
