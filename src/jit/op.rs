use std::mem;

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
    Blx,
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
    pub const fn is_branch(self) -> bool {
        matches!(
            self,
            Op::Bx
                | Op::BlxReg
                | Op::B
                | Op::Bl
                | Op::Blx
                | Op::BxRegT
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
                | Op::BlOffT
                | Op::BlxOffT
        )
    }

    pub const fn is_uncond_branch(self) -> bool {
        matches!(self, Op::Bx | Op::BlxReg | Op::B | Op::Bl | Op::Blx | Op::BxRegT | Op::BlxRegT | Op::BT | Op::BlOffT | Op::BlxOffT)
    }

    pub fn is_labelled_branch(self) -> bool {
        matches!(
            self,
            Op::B | Op::Bl | Op::Blx | Op::BT | Op::BeqT | Op::BneT | Op::BcsT | Op::BccT | Op::BmiT | Op::BplT | Op::BvsT | Op::BvcT | Op::BhiT | Op::BlsT | Op::BgeT | Op::BltT | Op::BgtT | Op::BleT
        )
    }

    pub const fn is_single_mem_transfer(self) -> bool {
        matches!(
            self,
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
                | Op::LdrdOfim
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
                | Op::StrdPtrp
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
                | Op::LdrImm5T
                | Op::LdrPcT
                | Op::LdrRegT
                | Op::LdrSpT
                | Op::LdrbImm5T
                | Op::LdrbRegT
                | Op::LdrhImm5T
                | Op::LdrhRegT
                | Op::LdrsbRegT
                | Op::LdrshRegT
                | Op::StrImm5T
                | Op::StrRegT
                | Op::StrSpT
                | Op::StrbImm5T
                | Op::StrbRegT
                | Op::StrhImm5T
                | Op::StrhRegT
        )
    }

    pub const fn is_multiple_mem_transfer(self) -> bool {
        matches!(
            self,
            Op::Ldmda
                | Op::LdmdaU
                | Op::LdmdaUW
                | Op::LdmdaW
                | Op::Ldmdb
                | Op::LdmdbU
                | Op::LdmdbUW
                | Op::LdmdbW
                | Op::Ldmia
                | Op::LdmiaU
                | Op::LdmiaUW
                | Op::LdmiaW
                | Op::Ldmib
                | Op::LdmibU
                | Op::LdmibUW
                | Op::LdmibW
                | Op::Stmda
                | Op::StmdaU
                | Op::StmdaUW
                | Op::StmdaW
                | Op::Stmdb
                | Op::StmdbU
                | Op::StmdbUW
                | Op::StmdbW
                | Op::Stmia
                | Op::StmiaU
                | Op::StmiaUW
                | Op::StmiaW
                | Op::Stmib
                | Op::StmibU
                | Op::StmibUW
                | Op::StmibW
                | Op::LdmiaT
                | Op::StmiaT
                | Op::PushT
                | Op::PushLrT
                | Op::PopT
                | Op::PopPcT
        )
    }

    pub const fn mem_is_write(self) -> bool {
        matches!(
            self,
            Op::StrOfim
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
                | Op::StrdPtrp
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
                | Op::StrImm5T
                | Op::StrRegT
                | Op::StrSpT
                | Op::StrbImm5T
                | Op::StrbRegT
                | Op::StrhImm5T
                | Op::StrhRegT
                | Op::Stmda
                | Op::StmdaU
                | Op::StmdaUW
                | Op::StmdaW
                | Op::Stmdb
                | Op::StmdbU
                | Op::StmdbUW
                | Op::StmdbW
                | Op::Stmia
                | Op::StmiaU
                | Op::StmiaUW
                | Op::StmiaW
                | Op::Stmib
                | Op::StmibU
                | Op::StmibUW
                | Op::StmibW
                | Op::StmiaT
                | Op::PushT
                | Op::PushLrT
        )
    }

    pub const fn mem_transfer_pre(self) -> bool {
        !matches!(self, |Op::LdrPtim| Op::LdrPtip
            | Op::LdrPtrmar
            | Op::LdrPtrmll
            | Op::LdrPtrmlr
            | Op::LdrPtrmrr
            | Op::LdrPtrpar
            | Op::LdrPtrpll
            | Op::LdrPtrplr
            | Op::LdrPtrprr
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
            | Op::LdrdPtim
            | Op::LdrdPtip
            | Op::LdrdPtrm
            | Op::LdrdPtrp
            | Op::LdrhPtim
            | Op::LdrhPtip
            | Op::LdrhPtrm
            | Op::LdrhPtrp
            | Op::LdrsbPtim
            | Op::LdrsbPtip
            | Op::LdrsbPtrm
            | Op::LdrsbPtrp
            | Op::LdrshPtim
            | Op::LdrshPtip
            | Op::LdrshPtrm
            | Op::LdrshPtrp
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
            | Op::StrdPtim
            | Op::StrdPtip
            | Op::StrdPtrm
            | Op::StrdPtrp
            | Op::StrhPtim
            | Op::StrhPtip
            | Op::StrhPtrm
            | Op::StrhPtrp
            | Op::Ldmia
            | Op::LdmiaU
            | Op::LdmiaUW
            | Op::LdmiaW
            | Op::Stmia
            | Op::StmiaU
            | Op::StmiaUW
            | Op::StmiaW
            | Op::LdmiaT
            | Op::StmiaT
            | Op::Ldmda
            | Op::LdmdaU
            | Op::LdmdaUW
            | Op::LdmdaW
            | Op::Stmda
            | Op::StmdaU
            | Op::StmdaUW
            | Op::StmdaW
            | Op::PopT
            | Op::PopPcT)
    }

    pub const fn mem_transfer_write_back(self) -> bool {
        matches!(
            self,
            Op::LdrPrim
                | Op::LdrPrip
                | Op::LdrPrrmar
                | Op::LdrPrrmll
                | Op::LdrPrrmlr
                | Op::LdrPrrmrr
                | Op::LdrPrrpar
                | Op::LdrPrrpll
                | Op::LdrPrrplr
                | Op::LdrPrrprr
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
                | Op::LdrdPrim
                | Op::LdrdPrip
                | Op::LdrdPrrm
                | Op::LdrdPrrp
                | Op::LdrhPrim
                | Op::LdrhPrip
                | Op::LdrhPrrm
                | Op::LdrhPrrp
                | Op::LdrsbPrim
                | Op::LdrsbPrip
                | Op::LdrsbPrrm
                | Op::LdrsbPrrp
                | Op::LdrshPrim
                | Op::LdrshPrip
                | Op::LdrshPrrm
                | Op::LdrshPrrp
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
                | Op::StrdPrim
                | Op::StrdPrip
                | Op::StrdPrrm
                | Op::StrdPrrp
                | Op::StrhPrim
                | Op::StrhPrip
                | Op::StrhPrrm
                | Op::StrhPrrp
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
                | Op::LdrdPtim
                | Op::LdrdPtip
                | Op::LdrdPtrm
                | Op::LdrdPtrp
                | Op::LdrhPtim
                | Op::LdrhPtip
                | Op::LdrhPtrm
                | Op::LdrhPtrp
                | Op::LdrsbPtim
                | Op::LdrsbPtip
                | Op::LdrsbPtrm
                | Op::LdrsbPtrp
                | Op::LdrshPtim
                | Op::LdrshPtip
                | Op::LdrshPtrm
                | Op::LdrshPtrp
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
                | Op::StrdPtim
                | Op::StrdPtip
                | Op::StrdPtrm
                | Op::StrdPtrp
                | Op::StrhPtim
                | Op::StrhPtip
                | Op::StrhPtrm
                | Op::StrhPtrp
                | Op::LdmdaUW
                | Op::LdmdaW
                | Op::LdmdbUW
                | Op::LdmdbW
                | Op::LdmiaUW
                | Op::LdmiaW
                | Op::LdmibUW
                | Op::LdmibW
                | Op::StmdaUW
                | Op::StmdaW
                | Op::StmdbUW
                | Op::StmdbW
                | Op::StmiaUW
                | Op::StmiaW
                | Op::StmibUW
                | Op::StmibW
                | Op::LdmiaT
                | Op::StmiaT
                | Op::PopT
                | Op::PopPcT
                | Op::PushT
                | Op::PushLrT
        )
    }

    pub const fn mem_transfer_decrement(self) -> bool {
        matches!(self, |Op::Ldmda| Op::LdmdaU
            | Op::LdmdaUW
            | Op::LdmdaW
            | Op::Stmda
            | Op::StmdaU
            | Op::StmdaUW
            | Op::StmdaW
            | Op::Ldmdb
            | Op::LdmdbU
            | Op::LdmdbUW
            | Op::LdmdbW
            | Op::Stmdb
            | Op::StmdbU
            | Op::StmdbUW
            | Op::StmdbW
            | Op::PushT
            | Op::PushLrT)
    }

    pub const fn mem_transfer_user(self) -> bool {
        matches!(
            self,
            Op::LdmdaU
                | Op::LdmdaUW
                | Op::LdmdbU
                | Op::LdmdbUW
                | Op::LdmiaU
                | Op::LdmiaUW
                | Op::LdmibU
                | Op::LdmibUW
                | Op::StmdaU
                | Op::StmdaUW
                | Op::StmdbU
                | Op::StmdbUW
                | Op::StmiaU
                | Op::StmiaUW
                | Op::StmibU
                | Op::StmibUW
        )
    }

    pub const fn is_alu_reg_shift(self) -> bool {
        matches!(
            self,
            Op::AdcArr
                | Op::AdcLlr
                | Op::AdcLrr
                | Op::AdcRrr
                | Op::AdcsArr
                | Op::AdcsLlr
                | Op::AdcsLrr
                | Op::AdcsRrr
                | Op::AddArr
                | Op::AddLlr
                | Op::AddLrr
                | Op::AddRrr
                | Op::AddsArr
                | Op::AddsLlr
                | Op::AddsLrr
                | Op::AddsRrr
                | Op::AndArr
                | Op::AndLlr
                | Op::AndLrr
                | Op::AndRrr
                | Op::AndsArr
                | Op::AndsLlr
                | Op::AndsLrr
                | Op::AndsRrr
                | Op::BicArr
                | Op::BicLlr
                | Op::BicLrr
                | Op::BicRrr
                | Op::BicsArr
                | Op::BicsLlr
                | Op::BicsLrr
                | Op::BicsRrr
                | Op::CmnArr
                | Op::CmnLlr
                | Op::CmnLrr
                | Op::CmnRrr
                | Op::CmpArr
                | Op::CmpLlr
                | Op::CmpLrr
                | Op::CmpRrr
                | Op::EorArr
                | Op::EorLlr
                | Op::EorLrr
                | Op::EorRrr
                | Op::EorsArr
                | Op::EorsLlr
                | Op::EorsLrr
                | Op::EorsRrr
                | Op::MovArr
                | Op::MovLlr
                | Op::MovLrr
                | Op::MovRrr
                | Op::MovsArr
                | Op::MovsLlr
                | Op::MovsLrr
                | Op::MovsRrr
                | Op::MvnArr
                | Op::MvnLlr
                | Op::MvnLrr
                | Op::MvnRrr
                | Op::MvnsArr
                | Op::MvnsLlr
                | Op::MvnsLrr
                | Op::MvnsRrr
                | Op::OrrArr
                | Op::OrrLlr
                | Op::OrrLrr
                | Op::OrrRrr
                | Op::OrrsArr
                | Op::OrrsLlr
                | Op::OrrsLrr
                | Op::OrrsRrr
                | Op::RsbArr
                | Op::RsbLlr
                | Op::RsbLrr
                | Op::RsbRrr
                | Op::RsbsArr
                | Op::RsbsLlr
                | Op::RsbsLrr
                | Op::RsbsRrr
                | Op::RscArr
                | Op::RscLlr
                | Op::RscLrr
                | Op::RscRrr
                | Op::RscsArr
                | Op::RscsLlr
                | Op::RscsLrr
                | Op::RscsRrr
                | Op::SbcArr
                | Op::SbcLlr
                | Op::SbcLrr
                | Op::SbcRrr
                | Op::SbcsArr
                | Op::SbcsLlr
                | Op::SbcsLrr
                | Op::SbcsRrr
                | Op::SubArr
                | Op::SubLlr
                | Op::SubLrr
                | Op::SubRrr
                | Op::SubsArr
                | Op::SubsLlr
                | Op::SubsLrr
                | Op::SubsRrr
                | Op::TeqArr
                | Op::TeqLlr
                | Op::TeqLrr
                | Op::TeqRrr
                | Op::TstArr
                | Op::TstLlr
                | Op::TstLrr
                | Op::TstRrr
        )
    }

    pub const fn mem_transfer_single_sub(self) -> bool {
        matches!(
            self,
            Op::LdrOfim
                | Op::LdrOfrmar
                | Op::LdrOfrmll
                | Op::LdrOfrmlr
                | Op::LdrOfrmrr
                | Op::LdrPrim
                | Op::LdrPrrmar
                | Op::LdrPrrmll
                | Op::LdrPrrmlr
                | Op::LdrPrrmrr
                | Op::LdrPtim
                | Op::LdrPtrmar
                | Op::LdrPtrmll
                | Op::LdrPtrmlr
                | Op::LdrPtrmrr
                | Op::LdrbOfim
                | Op::LdrbOfrmar
                | Op::LdrbOfrmll
                | Op::LdrbOfrmlr
                | Op::LdrbOfrmrr
                | Op::LdrbPrim
                | Op::LdrbPrrmar
                | Op::LdrbPrrmll
                | Op::LdrbPrrmlr
                | Op::LdrbPrrmrr
                | Op::LdrbPtim
                | Op::LdrbPtrmar
                | Op::LdrbPtrmll
                | Op::LdrbPtrmlr
                | Op::LdrbPtrmrr
                | Op::LdrdOfim
                | Op::LdrdOfrm
                | Op::LdrdPrim
                | Op::LdrdPrrm
                | Op::LdrdPtim
                | Op::LdrdPtrm
                | Op::LdrhOfim
                | Op::LdrhOfrm
                | Op::LdrhPrim
                | Op::LdrhPrrm
                | Op::LdrhPtim
                | Op::LdrhPtrm
                | Op::LdrsbOfim
                | Op::LdrsbOfrm
                | Op::LdrsbPrim
                | Op::LdrsbPrrm
                | Op::LdrsbPtim
                | Op::LdrsbPtrm
                | Op::LdrshOfim
                | Op::LdrshOfrm
                | Op::LdrshPrim
                | Op::LdrshPrrm
                | Op::LdrshPtim
                | Op::LdrshPtrm
                | Op::StrOfim
                | Op::StrOfrmar
                | Op::StrOfrmll
                | Op::StrOfrmlr
                | Op::StrOfrmrr
                | Op::StrPrim
                | Op::StrPrrmar
                | Op::StrPrrmll
                | Op::StrPrrmlr
                | Op::StrPrrmrr
                | Op::StrPtim
                | Op::StrPtrmar
                | Op::StrPtrmll
                | Op::StrPtrmlr
                | Op::StrPtrmrr
                | Op::StrbOfim
                | Op::StrbOfrmar
                | Op::StrbOfrmll
                | Op::StrbOfrmlr
                | Op::StrbOfrmrr
                | Op::StrbPrim
                | Op::StrbPrrmar
                | Op::StrbPrrmll
                | Op::StrbPrrmlr
                | Op::StrbPrrmrr
                | Op::StrbPtim
                | Op::StrbPtrmar
                | Op::StrbPtrmll
                | Op::StrbPtrmlr
                | Op::StrbPtrmrr
                | Op::StrdOfim
                | Op::StrdOfrm
                | Op::StrdPrim
                | Op::StrdPrrm
                | Op::StrdPtim
                | Op::StrdPtrm
                | Op::StrhOfim
                | Op::StrhOfrm
                | Op::StrhPrim
                | Op::StrhPrrm
                | Op::StrhPtim
                | Op::StrhPtrm
        )
    }

    pub const fn mem_transfer_single_signed(self) -> bool {
        matches!(
            self,
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
                | Op::LdrsbRegT
                | Op::LdrshRegT
        )
    }

    pub const fn is_arm_alu(self) -> bool {
        matches!(
            self,
            Op::AdcAri
                | Op::AdcArr
                | Op::AdcImm
                | Op::AdcLli
                | Op::AdcLlr
                | Op::AdcLri
                | Op::AdcLrr
                | Op::AdcRri
                | Op::AdcRrr
                | Op::AdcsAri
                | Op::AdcsArr
                | Op::AdcsImm
                | Op::AdcsLli
                | Op::AdcsLlr
                | Op::AdcsLri
                | Op::AdcsLrr
                | Op::AdcsRri
                | Op::AdcsRrr
                | Op::AddAri
                | Op::AddArr
                | Op::AddImm
                | Op::AddLli
                | Op::AddLlr
                | Op::AddLri
                | Op::AddLrr
                | Op::AddRri
                | Op::AddRrr
                | Op::AddsAri
                | Op::AddsArr
                | Op::AddsImm
                | Op::AddsLli
                | Op::AddsLlr
                | Op::AddsLri
                | Op::AddsLrr
                | Op::AddsRri
                | Op::AddsRrr
                | Op::AndAri
                | Op::AndArr
                | Op::AndImm
                | Op::AndLli
                | Op::AndLlr
                | Op::AndLri
                | Op::AndLrr
                | Op::AndRri
                | Op::AndRrr
                | Op::AndsAri
                | Op::AndsArr
                | Op::AndsImm
                | Op::AndsLli
                | Op::AndsLlr
                | Op::AndsLri
                | Op::AndsLrr
                | Op::AndsRri
                | Op::AndsRrr
                | Op::BicAri
                | Op::BicArr
                | Op::BicImm
                | Op::BicLli
                | Op::BicLlr
                | Op::BicLri
                | Op::BicLrr
                | Op::BicRri
                | Op::BicRrr
                | Op::BicsAri
                | Op::BicsArr
                | Op::BicsImm
                | Op::BicsLli
                | Op::BicsLlr
                | Op::BicsLri
                | Op::BicsLrr
                | Op::BicsRri
                | Op::BicsRrr
                | Op::CmnAri
                | Op::CmnArr
                | Op::CmnImm
                | Op::CmnLli
                | Op::CmnLlr
                | Op::CmnLri
                | Op::CmnLrr
                | Op::CmnRri
                | Op::CmnRrr
                | Op::CmpAri
                | Op::CmpArr
                | Op::CmpImm
                | Op::CmpLli
                | Op::CmpLlr
                | Op::CmpLri
                | Op::CmpLrr
                | Op::CmpRri
                | Op::CmpRrr
                | Op::EorAri
                | Op::EorArr
                | Op::EorImm
                | Op::EorLli
                | Op::EorLlr
                | Op::EorLri
                | Op::EorLrr
                | Op::EorRri
                | Op::EorRrr
                | Op::EorsAri
                | Op::EorsArr
                | Op::EorsImm
                | Op::EorsLli
                | Op::EorsLlr
                | Op::EorsLri
                | Op::EorsLrr
                | Op::EorsRri
                | Op::EorsRrr
                | Op::MovAri
                | Op::MovArr
                | Op::MovImm
                | Op::MovLli
                | Op::MovLlr
                | Op::MovLri
                | Op::MovLrr
                | Op::MovRri
                | Op::MovRrr
                | Op::MovsAri
                | Op::MovsArr
                | Op::MovsImm
                | Op::MovsLli
                | Op::MovsLlr
                | Op::MovsLri
                | Op::MovsLrr
                | Op::MovsRri
                | Op::MovsRrr
                | Op::MvnAri
                | Op::MvnArr
                | Op::MvnImm
                | Op::MvnLli
                | Op::MvnLlr
                | Op::MvnLri
                | Op::MvnLrr
                | Op::MvnRri
                | Op::MvnRrr
                | Op::MvnsAri
                | Op::MvnsArr
                | Op::MvnsImm
                | Op::MvnsLli
                | Op::MvnsLlr
                | Op::MvnsLri
                | Op::MvnsLrr
                | Op::MvnsRri
                | Op::MvnsRrr
                | Op::OrrAri
                | Op::OrrArr
                | Op::OrrImm
                | Op::OrrLli
                | Op::OrrLlr
                | Op::OrrLri
                | Op::OrrLrr
                | Op::OrrRri
                | Op::OrrRrr
                | Op::OrrsAri
                | Op::OrrsArr
                | Op::OrrsImm
                | Op::OrrsLli
                | Op::OrrsLlr
                | Op::OrrsLri
                | Op::OrrsLrr
                | Op::OrrsRri
                | Op::OrrsRrr
                | Op::RsbAri
                | Op::RsbArr
                | Op::RsbImm
                | Op::RsbLli
                | Op::RsbLlr
                | Op::RsbLri
                | Op::RsbLrr
                | Op::RsbRri
                | Op::RsbRrr
                | Op::RsbsAri
                | Op::RsbsArr
                | Op::RsbsImm
                | Op::RsbsLli
                | Op::RsbsLlr
                | Op::RsbsLri
                | Op::RsbsLrr
                | Op::RsbsRri
                | Op::RsbsRrr
                | Op::RscAri
                | Op::RscArr
                | Op::RscImm
                | Op::RscLli
                | Op::RscLlr
                | Op::RscLri
                | Op::RscLrr
                | Op::RscRri
                | Op::RscRrr
                | Op::RscsAri
                | Op::RscsArr
                | Op::RscsImm
                | Op::RscsLli
                | Op::RscsLlr
                | Op::RscsLri
                | Op::RscsLrr
                | Op::RscsRri
                | Op::RscsRrr
                | Op::SbcAri
                | Op::SbcArr
                | Op::SbcImm
                | Op::SbcLli
                | Op::SbcLlr
                | Op::SbcLri
                | Op::SbcLrr
                | Op::SbcRri
                | Op::SbcRrr
                | Op::SbcsAri
                | Op::SbcsArr
                | Op::SbcsImm
                | Op::SbcsLli
                | Op::SbcsLlr
                | Op::SbcsLri
                | Op::SbcsLrr
                | Op::SbcsRri
                | Op::SbcsRrr
                | Op::SubAri
                | Op::SubArr
                | Op::SubImm
                | Op::SubLli
                | Op::SubLlr
                | Op::SubLri
                | Op::SubLrr
                | Op::SubRri
                | Op::SubRrr
                | Op::SubsAri
                | Op::SubsArr
                | Op::SubsImm
                | Op::SubsLli
                | Op::SubsLlr
                | Op::SubsLri
                | Op::SubsLrr
                | Op::SubsRri
                | Op::SubsRrr
                | Op::TeqAri
                | Op::TeqArr
                | Op::TeqImm
                | Op::TeqLli
                | Op::TeqLlr
                | Op::TeqLri
                | Op::TeqLrr
                | Op::TeqRri
                | Op::TeqRrr
                | Op::TstAri
                | Op::TstArr
                | Op::TstImm
                | Op::TstLli
                | Op::TstLlr
                | Op::TstLri
                | Op::TstLrr
                | Op::TstRri
                | Op::TstRrr
        )
    }

    pub const fn is_alu3_imm(self) -> bool {
        matches!(
            self,
            Op::AndImm
                | Op::AndsImm
                | Op::EorImm
                | Op::EorsImm
                | Op::SubImm
                | Op::SubsImm
                | Op::RsbImm
                | Op::RsbsImm
                | Op::AddImm
                | Op::AddsImm
                | Op::AdcImm
                | Op::AdcsImm
                | Op::SbcImm
                | Op::SbcsImm
                | Op::RscImm
                | Op::RscsImm
                | Op::OrrImm
                | Op::OrrsImm
                | Op::BicImm
                | Op::BicsImm
        )
    }

    pub const fn is_alu3_imm_shift(self) -> bool {
        matches!(
            self,
            Op::AndAri
                | Op::AndLli
                | Op::AndLri
                | Op::AndRri
                | Op::AndsAri
                | Op::AndsLli
                | Op::AndsLri
                | Op::AndsRri
                | Op::EorAri
                | Op::EorLli
                | Op::EorLri
                | Op::EorRri
                | Op::EorsAri
                | Op::EorsLli
                | Op::EorsLri
                | Op::EorsRri
                | Op::SubAri
                | Op::SubLli
                | Op::SubLri
                | Op::SubRri
                | Op::SubsAri
                | Op::SubsLli
                | Op::SubsLri
                | Op::SubsRri
                | Op::RsbAri
                | Op::RsbLli
                | Op::RsbLri
                | Op::RsbRri
                | Op::RsbsAri
                | Op::RsbsLli
                | Op::RsbsLri
                | Op::RsbsRri
                | Op::AddAri
                | Op::AddLli
                | Op::AddLri
                | Op::AddRri
                | Op::AddsAri
                | Op::AddsLli
                | Op::AddsLri
                | Op::AddsRri
                | Op::AdcAri
                | Op::AdcLli
                | Op::AdcLri
                | Op::AdcRri
                | Op::AdcsAri
                | Op::AdcsLli
                | Op::AdcsLri
                | Op::AdcsRri
                | Op::SbcAri
                | Op::SbcLli
                | Op::SbcLri
                | Op::SbcRri
                | Op::SbcsAri
                | Op::SbcsLli
                | Op::SbcsLri
                | Op::SbcsRri
                | Op::RscAri
                | Op::RscLli
                | Op::RscLri
                | Op::RscRri
                | Op::RscsAri
                | Op::RscsLli
                | Op::RscsLri
                | Op::RscsRri
                | Op::OrrAri
                | Op::OrrLli
                | Op::OrrLri
                | Op::OrrRri
                | Op::OrrsAri
                | Op::OrrsLli
                | Op::OrrsLri
                | Op::OrrsRri
                | Op::BicAri
                | Op::BicLli
                | Op::BicLri
                | Op::BicRri
                | Op::BicsAri
                | Op::BicsLli
                | Op::BicsLri
                | Op::BicsRri
        )
    }

    pub const fn is_alu3_reg_shift(self) -> bool {
        matches!(
            self,
            Op::AndArr
                | Op::AndLlr
                | Op::AndLrr
                | Op::AndRrr
                | Op::AndsArr
                | Op::AndsLlr
                | Op::AndsLrr
                | Op::AndsRrr
                | Op::EorArr
                | Op::EorLlr
                | Op::EorLrr
                | Op::EorRrr
                | Op::EorsArr
                | Op::EorsLlr
                | Op::EorsLrr
                | Op::EorsRrr
                | Op::SubArr
                | Op::SubLlr
                | Op::SubLrr
                | Op::SubRrr
                | Op::SubsArr
                | Op::SubsLlr
                | Op::SubsLrr
                | Op::SubsRrr
                | Op::RsbArr
                | Op::RsbLlr
                | Op::RsbLrr
                | Op::RsbRrr
                | Op::RsbsArr
                | Op::RsbsLlr
                | Op::RsbsLrr
                | Op::RsbsRrr
                | Op::AddArr
                | Op::AddLlr
                | Op::AddLrr
                | Op::AddRrr
                | Op::AddsArr
                | Op::AddsLlr
                | Op::AddsLrr
                | Op::AddsRrr
                | Op::AdcArr
                | Op::AdcLlr
                | Op::AdcLrr
                | Op::AdcRrr
                | Op::AdcsArr
                | Op::AdcsLlr
                | Op::AdcsLrr
                | Op::AdcsRrr
                | Op::SbcArr
                | Op::SbcLlr
                | Op::SbcLrr
                | Op::SbcRrr
                | Op::SbcsArr
                | Op::SbcsLlr
                | Op::SbcsLrr
                | Op::SbcsRrr
                | Op::RscArr
                | Op::RscLlr
                | Op::RscLrr
                | Op::RscRrr
                | Op::RscsArr
                | Op::RscsLlr
                | Op::RscsLrr
                | Op::RscsRrr
                | Op::OrrArr
                | Op::OrrLlr
                | Op::OrrLrr
                | Op::OrrRrr
                | Op::OrrsArr
                | Op::OrrsLlr
                | Op::OrrsLrr
                | Op::OrrsRrr
                | Op::BicArr
                | Op::BicLlr
                | Op::BicLrr
                | Op::BicRrr
                | Op::BicsArr
                | Op::BicsLlr
                | Op::BicsLrr
                | Op::BicsRrr
        )
    }

    pub const fn is_alu2_op1_imm(self) -> bool {
        matches!(self, Op::TstImm | Op::TeqImm | Op::CmpImm | Op::CmnImm)
    }

    pub const fn is_alu2_op1_imm_shift(self) -> bool {
        matches!(
            self,
            Op::TstAri
                | Op::TstLli
                | Op::TstLri
                | Op::TstRri
                | Op::TeqAri
                | Op::TeqLli
                | Op::TeqLri
                | Op::TeqRri
                | Op::CmpAri
                | Op::CmpLli
                | Op::CmpLri
                | Op::CmpRri
                | Op::CmnAri
                | Op::CmnLli
                | Op::CmnLri
                | Op::CmnRri
        )
    }

    pub const fn is_alu2_op1_reg_shift(self) -> bool {
        matches!(
            self,
            Op::TstArr
                | Op::TstLlr
                | Op::TstLrr
                | Op::TstRrr
                | Op::TeqArr
                | Op::TeqLlr
                | Op::TeqLrr
                | Op::TeqRrr
                | Op::CmpArr
                | Op::CmpLlr
                | Op::CmpLrr
                | Op::CmpRrr
                | Op::CmnArr
                | Op::CmnLlr
                | Op::CmnLrr
                | Op::CmnRrr
        )
    }

    pub const fn is_alu2_op0_imm(self) -> bool {
        matches!(self, Op::MovImm | Op::MovsImm | Op::MvnImm | Op::MvnsImm)
    }

    pub const fn is_alu2_op0_imm_shift(self) -> bool {
        matches!(
            self,
            Op::MovAri
                | Op::MovLli
                | Op::MovLri
                | Op::MovRri
                | Op::MovsAri
                | Op::MovsLli
                | Op::MovsLri
                | Op::MovsRri
                | Op::MvnAri
                | Op::MvnLli
                | Op::MvnLri
                | Op::MvnRri
                | Op::MvnsAri
                | Op::MvnsLli
                | Op::MvnsLri
                | Op::MvnsRri
        )
    }

    pub const fn is_alu2_op0_reg_shift(self) -> bool {
        matches!(
            self,
            Op::MovArr
                | Op::MovLlr
                | Op::MovLrr
                | Op::MovRrr
                | Op::MovsArr
                | Op::MovsLlr
                | Op::MovsLrr
                | Op::MovsRrr
                | Op::MvnArr
                | Op::MvnLlr
                | Op::MvnLrr
                | Op::MvnRrr
                | Op::MvnsArr
                | Op::MvnsLlr
                | Op::MvnsLrr
                | Op::MvnsRrr
        )
    }

    pub const fn is_mov(self) -> bool {
        matches!(
            self,
            Op::MovAri
                | Op::MovArr
                | Op::MovImm
                | Op::MovLli
                | Op::MovLlr
                | Op::MovLri
                | Op::MovLrr
                | Op::MovRri
                | Op::MovRrr
                | Op::MovsAri
                | Op::MovsArr
                | Op::MovsImm
                | Op::MovsLli
                | Op::MovsLlr
                | Op::MovsLri
                | Op::MovsLrr
                | Op::MovsRri
                | Op::MovsRrr
                | Op::MovHT
                | Op::MovImm8T
        )
    }
}

impl From<u16> for Op {
    fn from(value: u16) -> Self {
        debug_assert!(value <= Op::UnkThumb as u16);
        unsafe { mem::transmute(value) }
    }
}
