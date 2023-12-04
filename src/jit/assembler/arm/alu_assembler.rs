use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::reg::Reg;
use crate::jit::Op::{MovImm, SubImm};
use crate::jit::{Cond, ShiftType};
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
        debug_assert_eq!(lookup_opcode(op).0, SubImm);
        op
    }

    #[inline]
    pub fn sub_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        AluImm::sub(op0, op1, op2, 0, Cond::AL)
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
        debug_assert_eq!(lookup_opcode(op).0, MovImm);
        op
    }

    #[inline]
    pub fn mov_al(op0: Reg, op2: u8) -> u32 {
        AluImm::mov(op0, op2, 0, Cond::AL)
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
    pub fn mov32(op0: Reg, op2: u32) -> [u32; 2] {
        [
            AluImm::mov16_al(op0, (op2 & 0xFFFF) as u16),
            AluImm::mov_t_al(op0, (op2 >> 16) as u16),
        ]
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
    pub fn add(
        op0: Reg,
        op1: Reg,
        op2: Reg,
        shift_type: ShiftType,
        shift_reg: Reg,
        cond: Cond,
    ) -> u32 {
        u32::from(AluReg::new(
            u4::new(op2 as u8),
            u1::new(1),
            u2::new(shift_type as u8),
            u1::new(1),
            u4::new(shift_reg as u8),
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
    pub fn orr(
        op0: Reg,
        op1: Reg,
        op2: Reg,
        shift_type: ShiftType,
        shift_reg: Reg,
        cond: Cond,
    ) -> u32 {
        u32::from(AluReg::new(
            u4::new(op2 as u8),
            u1::new(1),
            u2::new(shift_type as u8),
            u1::new(1),
            u4::new(shift_reg as u8),
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
    pub fn sub(
        op0: Reg,
        op1: Reg,
        op2: Reg,
        shift_type: ShiftType,
        shift_reg: Reg,
        cond: Cond,
    ) -> u32 {
        u32::from(AluReg::new(
            u4::new(op2 as u8),
            u1::new(1),
            u2::new(shift_type as u8),
            u1::new(1),
            u4::new(shift_reg as u8),
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
    pub fn mov(op0: Reg, op2: Reg, cond: Cond) -> u32 {
        u32::from(AluReg::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(0),
            u1::new(0),
            u4::new(0),
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
        AluReg::mov(op0, op2, Cond::AL)
    }
}

// TODO Add const asserts once const features has been added back to rust
// https://github.com/rust-lang/rust/issues/110395
//const_assert_eq!(lookup_opcode(AluImm::add(0, 0, 0, 0)).0 as u8, And as u8);
