use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::reg::Reg;
use crate::jit::{Cond, Op, ShiftType};
use bilge::prelude::*;

#[bitsize(32)]
#[derive(FromBits)]
pub struct AluImm {
    pub nn: u8,
    pub ror: u4,
    pub rd: u4,
    pub rn: u4,
    pub s: u1,
    pub op: u4,
    pub imm: u1,
    pub id: u2,
    pub cond: u4,
}

impl AluImm {
    #[inline]
    pub fn add(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        let op = u32::from(AluImm::new(
            op2,
            u4::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(0),
            u4::new(0x4),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ));
        debug_assert_eq!(lookup_opcode(op).0, Op::AddImm);
        op
    }

    #[inline]
    pub fn add_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        AluImm::add(op0, op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn adds(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        let op = u32::from(AluImm::new(
            op2,
            u4::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(1),
            u4::new(0x4),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ));
        debug_assert_eq!(lookup_opcode(op).0, Op::AddsImm);
        op
    }

    #[inline]
    pub fn adds_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        AluImm::adds(op0, op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn and(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        u32::from(AluImm::new(
            op2,
            u4::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(0),
            u4::new(0x0),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn bic(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        u32::from(AluImm::new(
            op2,
            u4::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(0),
            u4::new(0xE),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn cmp(op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        let op = u32::from(AluImm::new(
            op2,
            u4::new(shift),
            u4::new(0),
            u4::new(op1 as u8),
            u1::new(1),
            u4::new(0xA),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ));
        debug_assert_eq!(lookup_opcode(op).0, Op::CmpImm);
        op
    }

    #[inline]
    pub fn cmp_al(op1: Reg, op2: u8) -> u32 {
        AluImm::cmp(op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn orr(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        u32::from(AluImm::new(
            op2,
            u4::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(0),
            u4::new(0xC),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn sub(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        let op = u32::from(AluImm::new(
            op2,
            u4::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(0),
            u4::new(0x2),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ));
        debug_assert_eq!(lookup_opcode(op).0, Op::SubImm);
        op
    }

    #[inline]
    pub fn sub_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        AluImm::sub(op0, op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn subs(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        let op = u32::from(AluImm::new(
            op2,
            u4::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(1),
            u4::new(0x2),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ));
        debug_assert_eq!(lookup_opcode(op).0, Op::SubsImm);
        op
    }

    #[inline]
    pub fn subs_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        AluImm::subs(op0, op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn rsbs(op0: Reg, op1: Reg, op2: u8, shift: u8, cond: Cond) -> u32 {
        let op = u32::from(AluImm::new(
            op2,
            u4::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(1),
            u4::new(0x3),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ));
        debug_assert_eq!(lookup_opcode(op).0, Op::RsbsImm);
        op
    }

    #[inline]
    pub fn rsbs_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        AluImm::rsbs(op0, op1, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn mov(op0: Reg, op2: u8, ror: u8, cond: Cond) -> u32 {
        let op = u32::from(AluImm::new(
            op2,
            u4::new(ror),
            u4::new(op0 as u8),
            u4::new(0),
            u1::new(0),
            u4::new(0xD),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ));
        debug_assert_eq!(lookup_opcode(op).0, Op::MovImm);
        op
    }

    #[inline]
    pub fn mov_al(op0: Reg, op2: u8) -> u32 {
        AluImm::mov(op0, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn movs(op0: Reg, op2: u8, ror: u8, cond: Cond) -> u32 {
        let op = u32::from(AluImm::new(
            op2,
            u4::new(ror),
            u4::new(op0 as u8),
            u4::new(0),
            u1::new(1),
            u4::new(0xD),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ));
        debug_assert_eq!(lookup_opcode(op).0, Op::MovsImm);
        op
    }

    #[inline]
    pub fn movs_al(op0: Reg, op2: u8) -> u32 {
        AluImm::movs(op0, op2, 0, Cond::AL)
    }

    #[inline]
    pub fn mov16(op0: Reg, op2: u16, cond: Cond) -> u32 {
        u32::from(AluImm::new(
            (op2 & 0xFF) as u8,
            u4::new(((op2 >> 8) & 0xF) as u8),
            u4::new(op0 as u8),
            u4::new((op2 >> 12) as u8),
            u1::new(0),
            u4::new(0x8),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn mov16_al(op0: Reg, op2: u16) -> u32 {
        AluImm::mov16(op0, op2, Cond::AL)
    }

    #[inline]
    pub fn mov_t(op0: Reg, op2: u16, cond: Cond) -> u32 {
        u32::from(AluImm::new(
            (op2 & 0xFF) as u8,
            u4::new(((op2 >> 8) & 0xF) as u8),
            u4::new(op0 as u8),
            u4::new((op2 >> 12) as u8),
            u1::new(0),
            u4::new(0xA),
            u1::new(1),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn mov_t_al(op0: Reg, op2: u16) -> u32 {
        AluImm::mov_t(op0, op2, Cond::AL)
    }

    #[inline]
    pub fn mov32(op0: Reg, op2: u32) -> Vec<u32> {
        if op2 & 0xFFFFFF00 == 0 {
            vec![AluImm::mov_al(op0, op2 as u8)]
        } else {
            vec![
                AluImm::mov16_al(op0, (op2 & 0xFFFF) as u16),
                AluImm::mov_t_al(op0, (op2 >> 16) as u16),
            ]
        }
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct AluShiftImm {
    pub rm: u4,
    pub shift_r: u1,
    pub shift_type: u2,
    pub shift_imm: u5,
    pub rd: u4,
    pub rn: u4,
    pub set: u1,
    pub op: u4,
    pub imm: u1,
    pub id: u2,
    pub cond: u4,
}

impl AluShiftImm {
    #[inline]
    pub fn and(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(0),
            u4::new(0x0),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn ands(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(1),
            u4::new(0x0),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn ands_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        AluShiftImm::ands(op0, op1, op2, ShiftType::LSL, 0, Cond::AL)
    }

    #[inline]
    pub fn adcs(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(1),
            u4::new(0x5),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn adcs_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        AluShiftImm::adcs(op0, op1, op2, ShiftType::LSL, 0, Cond::AL)
    }

    #[inline]
    pub fn add(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(0),
            u4::new(0x4),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn add_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        AluShiftImm::add(op0, op1, op2, ShiftType::LSL, 0, Cond::AL)
    }

    #[inline]
    pub fn adds(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(1),
            u4::new(0x4),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn adds_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        AluShiftImm::adds(op0, op1, op2, ShiftType::LSL, 0, Cond::AL)
    }

    #[inline]
    pub fn bic(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(0),
            u4::new(0xE),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn bics(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(1),
            u4::new(0xE),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn bics_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        AluShiftImm::bics(op0, op1, op2, ShiftType::LSL, 0, Cond::AL)
    }

    #[inline]
    pub fn cmp(op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(0),
            u4::new(op1 as u8),
            u1::new(1),
            u4::new(0xA),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn cmp_al(op1: Reg, op2: Reg) -> u32 {
        AluShiftImm::cmp(op1, op2, ShiftType::LSL, 0, Cond::AL)
    }

    #[inline]
    pub fn orr(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(0),
            u4::new(0xC),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn orr_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        AluShiftImm::orr(op0, op1, op2, ShiftType::LSL, 0, Cond::AL)
    }

    #[inline]
    pub fn orrs(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(1),
            u4::new(0xC),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn orrs_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        AluShiftImm::orrs(op0, op1, op2, ShiftType::LSL, 0, Cond::AL)
    }

    #[inline]
    pub fn sub(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(0),
            u4::new(0x2),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn sub_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        AluShiftImm::sub(op0, op1, op2, ShiftType::LSL, 0, Cond::AL)
    }

    #[inline]
    pub fn subs(op0: Reg, op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u1::new(1),
            u4::new(0x2),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn subs_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        AluShiftImm::subs(op0, op1, op2, ShiftType::LSL, 0, Cond::AL)
    }

    #[inline]
    pub fn tst(op1: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(0),
            u4::new(op1 as u8),
            u1::new(1),
            u4::new(0x8),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn tst_al(op1: Reg, op2: Reg) -> u32 {
        AluShiftImm::tst(op1, op2, ShiftType::LSL, 0, Cond::AL)
    }

    #[inline]
    pub fn mov(op0: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(0),
            u1::new(0),
            u4::new(0xD),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn mov_al(op0: Reg, op2: Reg) -> u32 {
        AluShiftImm::mov(op0, op2, ShiftType::LSL, 0, Cond::AL)
    }

    #[inline]
    pub fn movs(op0: Reg, op2: Reg, shift_type: ShiftType, shift: u8, cond: Cond) -> u32 {
        u32::from(AluShiftImm::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift),
            u4::new(op0 as u8),
            u4::new(0),
            u1::new(1),
            u4::new(0xD),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn movs_al(op0: Reg, op2: Reg) -> u32 {
        AluShiftImm::movs(op0, op2, ShiftType::LSL, 0, Cond::AL)
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct AluReg {
    pub rm: u4,
    pub shift_r: u1,
    pub shift_type: u2,
    pub res: u1,
    pub rs: u4,
    pub rd: u4,
    pub rn: u4,
    pub set: u1,
    pub op: u4,
    pub imm: u1,
    pub id: u2,
    pub cond: u4,
}

impl AluReg {
    #[inline]
    pub fn movs(op0: Reg, op2: Reg, shift_type: ShiftType, shift_reg: Reg, cond: Cond) -> u32 {
        u32::from(AluReg::new(
            u4::new(op2 as u8),
            u1::new(1),
            u2::new(shift_type as u8),
            u1::new(0),
            u4::new(shift_reg as u8),
            u4::new(op0 as u8),
            u4::new(0),
            u1::new(1),
            u4::new(0xD),
            u1::new(0),
            u2::new(0),
            u4::new(cond as u8),
        ))
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
    pub set: u1,
    pub op: u4,
    pub id: u3,
    pub cond: u4,
}

impl MulReg {
    #[inline]
    pub fn muls(op0: Reg, op1: Reg, op2: Reg, cond: Cond) -> u32 {
        u32::from(MulReg::new(
            u4::new(op2 as u8),
            u4::new(0b1001),
            u4::new(op1 as u8),
            u4::new(0),
            u4::new(op0 as u8),
            u1::new(1),
            u4::new(0b0),
            u3::new(0b000),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn muls_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        MulReg::muls(op0, op1, op2, Cond::AL)
    }
}

// TODO Add const asserts once const features has been added back to rust
// https://github.com/rust-lang/rust/issues/110395
//const_assert_eq!(lookup_opcode(AluImm::add(0, 0, 0, 0)).0 as u8, And as u8);
