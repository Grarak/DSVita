use crate::jit::reg::{Reg, RegReserve};

pub mod assembler;
pub mod disassembler;
mod emitter;
pub mod jit;
mod reg;
pub mod thread_context;

#[derive(Debug)]
pub struct InstInfo {
    operands: Operands,
    src_regs: RegReserve,
    out_regs: RegReserve,
}

impl InstInfo {
    #[inline]
    pub fn operands(&self) -> &[Operand] {
        &self.operands.values[..self.operands.num as usize]
    }
}

#[derive(Debug)]
pub struct Operands {
    values: [Operand; 3],
    num: u8,
}

impl Operands {
    #[inline]
    pub fn new_1(operand: Operand) -> Self {
        Operands {
            values: [operand, Operand::None, Operand::None],
            num: 1,
        }
    }

    #[inline]
    pub fn new_2(operand1: Operand, operand2: Operand) -> Self {
        Operands {
            values: [operand1, operand2, Operand::None],
            num: 2,
        }
    }

    #[inline]
    pub fn new_3(operand1: Operand, operand2: Operand, operand3: Operand) -> Self {
        Operands {
            values: [operand1, operand2, operand3],
            num: 3,
        }
    }
}

#[derive(Debug)]
pub enum Operand {
    Reg { reg: Reg, shift: Option<Shift> },
    Imm { imm: u32, shift: Option<Shift> },
    None,
}

impl Operand {
    #[inline]
    pub fn reg(reg: Reg) -> Self {
        Operand::Reg { reg, shift: None }
    }

    #[inline]
    pub fn reg_imm_shift(reg: Reg, shift_type: ShiftType, imm: u8) -> Self {
        let shift_value = ShiftValue::Imm(imm);
        Operand::Reg {
            reg,
            shift: Some(match shift_type {
                ShiftType::LSL => Shift::LSL(shift_value),
                ShiftType::LSR => Shift::LSR(shift_value),
                ShiftType::ASR => Shift::ASR(shift_value),
                ShiftType::ROR => Shift::ROR(shift_value),
            }),
        }
    }

    #[inline]
    pub fn imm(imm: u32) -> Self {
        Operand::Imm { imm, shift: None }
    }
}

#[derive(Debug)]
pub enum ShiftValue {
    Reg(Reg),
    Imm(u8),
}

#[derive(Debug)]
#[repr(u8)]
pub enum Shift {
    LSL(ShiftValue),
    LSR(ShiftValue),
    ASR(ShiftValue),
    ROR(ShiftValue),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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
}

impl Op {
    #[inline]
    pub fn is_branch(&self) -> bool {
        match self {
            Op::Bx | Op::BlxReg | Op::B | Op::Bl => true,
            _ => false,
        }
    }
}

#[repr(u8)]
pub enum Cond {
    EQ = 0b0000,
    NE = 0b0001,
    CS = 0b0010,
    CC = 0b0011,
    MI = 0b0100,
    PL = 0b0101,
    VS = 0b0110,
    VC = 0b0111,
    HI = 0b1000,
    LS = 0b1001,
    GE = 0b1010,
    LT = 0b1011,
    GT = 0b1100,
    LE = 0b1101,
    AL = 0b1110,
}

#[repr(u8)]
pub enum ShiftType {
    LSL = 0,
    LSR = 1,
    ASR = 2,
    ROR = 3,
}
