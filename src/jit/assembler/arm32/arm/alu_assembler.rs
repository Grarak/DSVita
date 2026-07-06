use crate::jit::reg::Reg;
use crate::jit::{Cond, ShiftType};
use bilge::prelude::*;

#[bitsize(32)]
#[derive(FromBits)]
pub struct AluImm {
    pub nn: u8,
    pub ror: u4,
    pub rd: u4,
    pub rn: u4,
    pub s: bool,
    pub op: u4,
    pub imm: bool,
    pub id: u2,
    pub cond: u4,
}

impl AluImm {
    #[inline]
    pub fn generic(op: u8, op0: Reg, op1: Reg, op2: u8, shift: u8, set_cond: bool, cond: Cond) -> u32 {
        u32::from(Self::new(
            op2,
            u4::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            set_cond,
            u4::new(op),
            true,
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn add(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x4, op0, op1, op2, shift, false, cond)
    }

    #[inline]
    pub fn add_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        Self::add(op0, op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn adds(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x4, op0, op1, op2, shift, true, cond)
    }

    #[inline]
    pub fn adds_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        Self::adds(op0, op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn and(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x0, op0, op1, op2, shift, false, cond)
    }

    #[inline]
    pub fn bic(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        Self::generic(0xE, op0, op1, op2, shift, false, cond)
    }

    pub fn bic_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        Self::bic(op0, op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn cmp(op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        Self::generic(0xA, Reg::R0, op1, op2, shift, true, cond)
    }

    #[inline]
    pub fn cmp_al(op1: Reg, op2: u8) -> u32 {
        Self::cmp(op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn orr(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        Self::generic(0xC, op0, op1, op2, shift, false, cond)
    }

    #[inline]
    pub fn orr_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        Self::orr(op0, op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn sub(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x2, op0, op1, op2, shift, false, cond)
    }

    #[inline]
    pub fn sub_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        Self::sub(op0, op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn subs(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x2, op0, op1, op2, shift, true, cond)
    }

    #[inline]
    pub fn subs_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        Self::subs(op0, op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn rsbs(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x3, op0, op1, op2, shift, true, cond)
    }

    #[inline]
    pub fn rsbs_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        Self::rsbs(op0, op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn mov(op0: Reg, op2: u8, ror: u8, cond: Cond) -> u32 {
        Self::generic(0xD, op0, Reg::R0, op2, ror, false, cond)
    }

    #[inline]
    pub fn mov_al(op0: Reg, op2: u8) -> u32 {
        Self::mov(op0, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn movs(op0: Reg, op2: u8, ror: u8, cond: Cond) -> u32 {
        Self::generic(0xD, op0, Reg::R0, op2, ror, true, cond)
    }

    #[inline]
    pub fn movs_al(op0: Reg, op2: u8) -> u32 {
        Self::movs(op0, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn mov16(op0: Reg, op2: u16, cond: Cond) -> u32 {
        u32::from(Self::new(
            (op2 & 0xFF) as u8,
            u4::new(((op2 >> 8) & 0xF) as u8),
            u4::new(op0 as u8),
            u4::new((op2 >> 12) as u8),
            false,
            u4::new(0x8),
            true,
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn mov16_al(op0: Reg, op2: u16) -> u32 {
        Self::mov16(op0, op2, Cond::AL)
    }

    #[inline]
    pub fn mov_t(op0: Reg, op2: u16, cond: Cond) -> u32 {
        u32::from(Self::new(
            (op2 & 0xFF) as u8,
            u4::new(((op2 >> 8) & 0xF) as u8),
            u4::new(op0 as u8),
            u4::new((op2 >> 12) as u8),
            false,
            u4::new(0xA),
            true,
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn mov_t_al(op0: Reg, op2: u16) -> u32 {
        Self::mov_t(op0, op2, Cond::AL)
    }

    #[inline]
    pub fn mov32(op0: Reg, op2: u32) -> ([u32; 2], usize) {
        if op2 & 0xFFFF0000 == 0 {
            ([Self::mov16_al(op0, op2 as u16), 0], 1)
        } else {
            ([Self::mov16_al(op0, (op2 & 0xFFFF) as u16), Self::mov_t_al(op0, (op2 >> 16) as u16)], 2)
        }
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct AluShiftImm {
    pub rm: u4,
    pub shift_reg: bool,
    pub shift_type: u2,
    pub shift_imm: u5,
    pub rd: u4,
    pub rn: u4,
    pub set: bool,
    pub op: u4,
    pub imm: bool,
    pub id: u2,
    pub cond: u4,
}

impl AluShiftImm {
    #[inline]
    pub fn generic(op: u8, op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, set_cond: bool, cond: Cond) -> u32 {
        u32::from(Self::new(
            u4::new(op2 as u8),
            false,
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            set_cond,
            u4::new(op),
            false,
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn and(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x0, op0, op1, op2, shift_type, shift, false, cond)
    }

    #[inline]
    pub fn ands(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x0, op0, op1, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn ands_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::ands(op0, op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn adcs(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x5, op0, op1, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn adcs_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::adcs(op0, op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn add(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x4, op0, op1, op2, shift_type, shift, false, cond)
    }

    #[inline]
    pub fn add_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::add(op0, op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn adds(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x4, op0, op1, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn adds_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::adds(op0, op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn bic(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0xE, op0, op1, op2, shift_type, shift, false, cond)
    }

    #[inline]
    pub fn bic_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::bic(op0, op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn bics(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0xE, op0, op1, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn bics_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::bics(op0, op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn cmp(op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0xA, Reg::R0, op1, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn cmp_al(op1: Reg, op2: Reg) -> u32 {
        Self::cmp(op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn cmn(op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0xB, Reg::R0, op1, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn cmn_al(op1: Reg, op2: Reg) -> u32 {
        Self::cmn(op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn eors(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x1, op0, op1, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn eors_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::eors(op0, op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn orr(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0xC, op0, op1, op2, shift_type, shift, false, cond)
    }

    #[inline]
    pub fn orr_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::orr(op0, op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn orrs(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0xC, op0, op1, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn orrs_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::orrs(op0, op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn sbcs(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x6, op0, op1, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn sbcs_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::sbcs(op0, op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn sub(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x2, op0, op1, op2, shift_type, shift, false, cond)
    }

    #[inline]
    pub fn sub_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::sub(op0, op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn subs(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x2, op0, op1, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn subs_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::subs(op0, op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn tst(op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0x8, Reg::R0, op1, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn tst_al(op1: Reg, op2: Reg) -> u32 {
        Self::tst(op1, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn mov(op0: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0xD, op0, Reg::R0, op2, shift_type, shift, false, cond)
    }

    #[inline]
    pub fn mov_al(op0: Reg, op2: Reg) -> u32 {
        Self::mov(op0, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn movs(op0: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0xD, op0, Reg::R0, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn movs_al(op0: Reg, op2: Reg) -> u32 {
        Self::movs(op0, op2, ShiftType::Lsl, 0, Cond::AL)
    }

    #[inline]
    pub fn mvns(op0: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        Self::generic(0xF, op0, Reg::R0, op2, shift_type, shift, true, cond)
    }

    #[inline]
    pub fn mvns_al(op0: Reg, op2: Reg) -> u32 {
        Self::mvns(op0, op2, ShiftType::Lsl, 0, Cond::AL)
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct AluReg {
    pub rm: u4,
    pub shift_reg: bool,
    pub shift_type: u2,
    pub res: u1,
    pub rs: u4,
    pub rd: u4,
    pub rn: u4,
    pub set: bool,
    pub op: u4,
    pub imm: bool,
    pub id: u2,
    pub cond: u4,
}

impl AluReg {
    #[inline]
    pub fn generic(op: u8, op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift_reg: Reg, set_cond: bool, cond: Cond) -> u32 {
        u32::from(Self::new(
            u4::new(op2 as u8),
            true,
            u2::new(shift_type as u8),
            u1::new(0),
            u4::new(shift_reg as u8),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            set_cond,
            u4::new(op),
            false,
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn mov(op0: Reg, op2: Reg, shift_type: ShiftType, shift_reg: Reg, cond: Cond) -> u32 {
        Self::generic(0xD, op0, Reg::R0, op2, shift_type, shift_reg, false, cond)
    }

    #[inline]
    pub fn movs(op0: Reg, op2: Reg, shift_type: ShiftType, shift_reg: Reg, cond: Cond) -> u32 {
        Self::generic(0xD, op0, Reg::R0, op2, shift_type, shift_reg, true, cond)
    }

    #[inline]
    pub fn add(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift_reg: Reg, cond: Cond) -> u32 {
        Self::generic(0x4, op0, op1, op2, shift_type, shift_reg, false, cond)
    }

    #[inline]
    pub fn bic(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift_reg: Reg, cond: Cond) -> u32 {
        Self::generic(0xE, op0, op1, op2, shift_type, shift_reg, false, cond)
    }

    #[inline]
    pub fn sub(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift_reg: Reg, cond: Cond) -> u32 {
        Self::generic(0x2, op0, op1, op2, shift_type, shift_reg, false, cond)
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct MulReg {
    pub rm: u4,
    pub non_half: u4,
    pub rs: u4,
    pub rn: u4,
    pub rd: u4,
    pub set: bool,
    pub op: u4,
    pub id: u3,
    pub cond: u4,
}

impl MulReg {
    #[inline]
    pub fn mul(op0: Reg, op1: Reg, op2: Reg, set_cond: bool, cond: Cond) -> u32 {
        u32::from(Self::new(
            u4::new(op2 as u8),
            u4::new(0b1001),
            u4::new(op1 as u8),
            u4::new(0),
            u4::new(op0 as u8),
            set_cond,
            u4::new(0b0),
            u3::new(0b000),
            u4::new(cond as u8),
        ))
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct QAddSub {
    pub rm: u4,
    id: u8,
    pub rd: u4,
    pub rn: u4,
    pub op: u4,
    id2: u4,
    pub cond: u4,
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct Clz {
    pub rm: u4,
    pub id: u8,
    pub rd: u4,
    pub id2: u12,
    pub cond: u4,
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct Bfc {
    id: u7,
    lsb: u5,
    rd: u4,
    msb: u5,
    id2: u7,
    cond: u4,
}

impl Bfc {
    #[inline]
    pub fn create(rd: Reg, lsb: u8, width: u8, cond: Cond) -> u32 {
        u32::from(Bfc::new(
            u7::new(0b0011111),
            u5::new(lsb),
            u4::new(rd as u8),
            u5::new(lsb + width - 1),
            u7::new(0b111110),
            u4::new(cond as u8),
        ))
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct Bfi {
    rn: u4,
    id: u3,
    lsb: u5,
    rd: u4,
    msb: u5,
    id2: u7,
    cond: u4,
}

impl Bfi {
    #[inline]
    pub fn create(rd: Reg, rn: Reg, lsb: u8, width: u8, cond: Cond) -> u32 {
        u32::from(Bfi::new(
            u4::new(rn as u8),
            u3::new(0b001),
            u5::new(lsb),
            u4::new(rd as u8),
            u5::new(lsb + width - 1),
            u7::new(0b0111110),
            u4::new(cond as u8),
        ))
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct Ubfx {
    rn: u4,
    id: u3,
    lsb: u5,
    rd: u4,
    widthm1: u5,
    id2: u7,
    cond: u4,
}

impl Ubfx {
    #[inline]
    pub fn create(rd: Reg, rn: Reg, lsb: u8, width: u8, cond: Cond) -> u32 {
        u32::from(Ubfx::new(
            u4::new(rn as u8),
            u3::new(0b101),
            u5::new(lsb),
            u4::new(rd as u8),
            u5::new(width - 1),
            u7::new(0b0111111),
            u4::new(cond as u8),
        ))
    }
}

// TODO Add const asserts once const features has been added back to rust
// https://github.com/rust-lang/rust/issues/110395
//const_assert_eq!(lookup_opcode(Self::add(0, 0, 0, 0)).0 as u8, And as u8);
