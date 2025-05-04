use std::fmt::{Debug, Formatter};

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct SingleTransfer(pub u8);

impl SingleTransfer {
    pub const fn new(pre: bool, write_back: bool, add: bool, signed: bool, size: u8) -> Self {
        SingleTransfer((pre as u8) | ((write_back as u8) << 1) | ((add as u8) << 2) | ((signed as u8) << 3) | (size << 4))
    }

    pub const fn pre(&self) -> bool {
        self.0 & 1 != 0
    }

    pub const fn write_back(&self) -> bool {
        self.0 & (1 << 1) != 0
    }

    pub const fn add(&self) -> bool {
        self.0 & (1 << 2) != 0
    }

    pub const fn signed(&self) -> bool {
        self.0 & (1 << 3) != 0
    }

    pub const fn size(&self) -> u8 {
        self.0 >> 4
    }
}

impl Debug for SingleTransfer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleTransfer")
            .field("pre", &self.pre())
            .field("write_back", &self.write_back())
            .field("add", &self.add())
            .field("signed", &self.signed())
            .field("size", &self.size())
            .finish()
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct MultipleTransfer(u8);

impl MultipleTransfer {
    pub const fn new(pre: bool, write_back: bool, add: bool, user: bool) -> Self {
        MultipleTransfer((pre as u8) | ((write_back as u8) << 1) | ((add as u8) << 2) | ((user as u8) << 3))
    }

    pub const fn pre(&self) -> bool {
        self.0 & 1 != 0
    }

    pub const fn write_back(&self) -> bool {
        self.0 & (1 << 1) != 0
    }

    pub const fn add(&self) -> bool {
        self.0 & (1 << 2) != 0
    }

    pub const fn user(&self) -> bool {
        self.0 & (1 << 3) != 0
    }
}

impl Debug for MultipleTransfer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultipleTransfer")
            .field("pre", &self.pre())
            .field("write_back", &self.write_back())
            .field("add", &self.add())
            .field("user", &self.user())
            .finish()
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Op {
    And,
    Eor,
    Sub,
    Rsb,
    Add,
    Adc,
    Sbc,
    Rsc,
    Tst,
    Teq,
    Cmp,
    Cmn,
    Orr,
    Mov,
    Bic,
    Mvn,
    Ands,
    Eors,
    Subs,
    Rsbs,
    Adds,
    Adcs,
    Sbcs,
    Rscs,
    Orrs,
    Movs,
    Bics,
    Mvns,

    Ldr(SingleTransfer),
    Str(SingleTransfer),
    Ldm(MultipleTransfer),
    Stm(MultipleTransfer),

    B,
    Bl,
    Blx,
    BlxReg,
    Bx,
    Clz,
    Mcr,
    Mla,
    Mlas,
    Mrc,
    MrsRc,
    MrsRs,
    MsrIc,
    MsrIs,
    MsrRc,
    MsrRs,
    Mul,
    Muls,
    Qadd,
    Qdadd,
    Qdsub,
    Qsub,
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
    Swi,
    Swp,
    Swpb,
    Umlal,
    Umlals,
    Umull,
    Umulls,
    UnkArm,

    // Thumb
    AddT,
    SubT,
    CmpT,
    MovT,
    LslT,
    LsrT,
    AsrT,
    RorT,
    AndT,
    EorT,
    AdcT,
    SbcT,
    TstT,
    CmnT,
    OrrT,
    BicT,
    MvnT,
    NegT,
    MulT,

    AddPcT,
    AddSpT,
    AddSpImmT,
    AddHT,
    CmpHT,
    MovHT,

    LdrT(SingleTransfer),
    StrT(SingleTransfer),
    LdmT(MultipleTransfer),
    StmT(MultipleTransfer),

    BT,
    BccT,
    BcsT,
    BeqT,
    BgeT,
    BgtT,
    BhiT,
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
    SwiT,
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
        matches!(self, Op::Ldr(_) | Op::LdrT(_) | Op::Str(_) | Op::StrT(_))
    }

    pub const fn is_multiple_mem_transfer(self) -> bool {
        matches!(self, Op::Ldm(_) | Op::LdmT(_) | Op::Stm(_) | Op::StmT(_))
    }

    pub const fn is_write_mem_transfer(self) -> bool {
        matches!(self, Op::Str(_) | Op::StrT(_) | Op::Stm(_) | Op::StmT(_))
    }

    pub const fn is_alu(self) -> bool {
        matches!(
            self,
            Op::And
                | Op::Eor
                | Op::Sub
                | Op::Rsb
                | Op::Add
                | Op::Adc
                | Op::Sbc
                | Op::Rsc
                | Op::Tst
                | Op::Teq
                | Op::Cmp
                | Op::Cmn
                | Op::Orr
                | Op::Mov
                | Op::Bic
                | Op::Mvn
                | Op::Ands
                | Op::Eors
                | Op::Subs
                | Op::Rsbs
                | Op::Adds
                | Op::Adcs
                | Op::Sbcs
                | Op::Rscs
                | Op::Orrs
                | Op::Movs
                | Op::Bics
                | Op::Mvns
                | Op::AddT
                | Op::SubT
                | Op::CmpT
                | Op::MovT
                | Op::LslT
                | Op::LsrT
                | Op::AsrT
                | Op::RorT
                | Op::AndT
                | Op::EorT
                | Op::AdcT
                | Op::SbcT
                | Op::TstT
                | Op::CmnT
                | Op::OrrT
                | Op::BicT
                | Op::MvnT
                | Op::NegT
                | Op::MulT
                | Op::AddPcT
                | Op::AddSpT
                | Op::AddSpImmT
                | Op::AddHT
                | Op::CmpHT
                | Op::MovHT
        )
    }

    pub const fn is_thumb_alu_high(self) -> bool {
        matches!(self, Op::AddSpImmT | Op::AddHT | Op::CmpHT | Op::MovHT)
    }

    pub const fn is_arm_alu2(self) -> bool {
        matches!(self, Op::Tst | Op::Teq | Op::Cmp | Op::Cmn | Op::Mov | Op::Mvn | Op::Movs | Op::Mvns)
    }

    pub const fn is_mov(self) -> bool {
        matches!(self, Op::Mov | Op::Movs | Op::MovT | Op::MovHT)
    }

    pub const fn is_mul(self) -> bool {
        matches!(
            self,
            Op::Mul
                | Op::Muls
                | Op::Mla
                | Op::Mlas
                | Op::Smlabb
                | Op::Smlabt
                | Op::Smlal
                | Op::Smlalbb
                | Op::Smlalbt
                | Op::Smlals
                | Op::Smlaltb
                | Op::Smlaltt
                | Op::Smlatb
                | Op::Smlatt
                | Op::Smlawb
                | Op::Smlawt
                | Op::Smulbb
                | Op::Smulbt
                | Op::Smull
                | Op::Smulls
                | Op::Smultb
                | Op::Smultt
                | Op::Smulwb
                | Op::Smulwt
                | Op::Umull
                | Op::Umulls
                | Op::Umlal
                | Op::Umlals
        )
    }
}
