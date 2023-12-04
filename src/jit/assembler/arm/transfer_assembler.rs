use crate::jit::reg::{Reg, RegReserve};
use crate::jit::Cond;
use bilge::prelude::*;

#[bitsize(32)]
#[derive(FromBits)]
pub struct LdrStrImm {
    pub imm_offset: u12,
    pub rd: u4,
    pub rn: u4,
    pub load_store: u1,
    pub t_w: u1,
    pub byte_word: u1,
    pub up_down: u1,
    pub pre_post: u1,
    pub imm: u1,
    pub id: u2,
    pub cond: u4,
}

impl LdrStrImm {
    #[inline]
    pub fn ldr(
        imm_offset: u16,
        rd: Reg,
        rn: Reg,
        t_w: bool,
        byte: bool,
        add: bool,
        pre: bool,
        cond: Cond,
    ) -> u32 {
        u32::from(LdrStrImm::new(
            u12::new(imm_offset),
            u4::new(rd as u8),
            u4::new(rn as u8),
            u1::new(1),
            u1::new(t_w as u8),
            u1::new(byte as u8),
            u1::new(add as u8),
            u1::new(pre as u8),
            u1::new(0),
            u2::new(1),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn ldr_offset_al(op0: Reg, op1: Reg, offset: u16) -> u32 {
        LdrStrImm::ldr(offset, op0, op1, false, false, true, true, Cond::AL)
    }

    #[inline]
    pub fn ldr_sub_offset_al(op0: Reg, op1: Reg, offset: u16) -> u32 {
        LdrStrImm::ldr(offset, op0, op1, false, false, false, true, Cond::AL)
    }

    #[inline]
    pub fn ldr_al(op0: Reg, op1: Reg) -> u32 {
        LdrStrImm::ldr_offset_al(op0, op1, 0)
    }

    #[inline]
    pub fn str(
        imm_offset: u16,
        rd: Reg,
        rn: Reg,
        t_w: bool,
        byte: bool,
        add: bool,
        pre: bool,
        cond: Cond,
    ) -> u32 {
        u32::from(LdrStrImm::new(
            u12::new(imm_offset),
            u4::new(rd as u8),
            u4::new(rn as u8),
            u1::new(0),
            u1::new(t_w as u8),
            u1::new(byte as u8),
            u1::new(add as u8),
            u1::new(pre as u8),
            u1::new(0),
            u2::new(1),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn str_al(op0: Reg, op1: Reg) -> u32 {
        LdrStrImm::str_offset_al(op0, op1, 0)
    }

    #[inline]
    pub fn str_offset_al(op0: Reg, op1: Reg, offset: u16) -> u32 {
        LdrStrImm::str(offset, op0, op1, false, false, true, true, Cond::AL)
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct LdmStm {
    pub rlist: u16,
    pub rn: u4,
    pub load_store: u1,
    pub w: u1,
    pub s: u1,
    pub u: u1,
    pub p: u1,
    pub id: u3,
    pub cond: u4,
}

impl LdmStm {
    #[inline]
    pub fn push_post(regs: RegReserve, sp: Reg, cond: Cond) -> u32 {
        u32::from(LdmStm::new(
            regs.0 as u16,
            u4::new(sp as u8),
            u1::new(0),
            u1::new(1),
            u1::new(0),
            u1::new(0),
            u1::new(0),
            u3::new(0b100),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn push_post_al(regs: RegReserve) -> u32 {
        LdmStm::push_post(regs, Reg::SP, Cond::AL)
    }

    #[inline]
    pub fn push_pre(regs: RegReserve, sp: Reg, cond: Cond) -> u32 {
        u32::from(LdmStm::new(
            regs.0 as u16,
            u4::new(sp as u8),
            u1::new(0),
            u1::new(1),
            u1::new(0),
            u1::new(0),
            u1::new(1),
            u3::new(0b100),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn pop_post(regs: RegReserve, sp: Reg, cond: Cond) -> u32 {
        u32::from(LdmStm::new(
            regs.0 as u16,
            u4::new(sp as u8),
            u1::new(1),
            u1::new(1),
            u1::new(0),
            u1::new(1),
            u1::new(0),
            u3::new(0b100),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn pop_post_al(regs: RegReserve) -> u32 {
        LdmStm::pop_post(regs, Reg::SP, Cond::AL)
    }

    #[inline]
    pub fn pop_pre(regs: RegReserve, sp: Reg, cond: Cond) -> u32 {
        u32::from(LdmStm::new(
            regs.0 as u16,
            u4::new(sp as u8),
            u1::new(1),
            u1::new(1),
            u1::new(0),
            u1::new(1),
            u1::new(1),
            u3::new(0b100),
            u4::new(cond as u8),
        ))
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct LdrexStrex {
    pub rm: u4,
    pub id: u8,
    pub rd: u4,
    pub rn: u4,
    pub op: u3,
    pub id2: u5,
    pub cond: u4,
}

impl LdrexStrex {
    pub fn ldrexd(op0: Reg, op1: Reg, cond: Cond) -> u32 {
        u32::from(LdrexStrex::new(
            u4::new(0b1111),
            0b11111001,
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            u3::new(3),
            u5::new(0b00011),
            u4::new(cond as u8),
        ))
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct Msr {
    pub rm: u4,
    pub id: u8,
    pub id1: u4,
    pub c: u1,
    pub x: u1,
    pub s: u1,
    pub f: u1,
    pub id2: u1,
    pub opcode: u1,
    pub psr: u1,
    pub id3: u2,
    pub imm: u1,
    pub id4: u2,
    pub cond: u4,
}

impl Msr {
    pub fn cpsr_flags(op1: Reg, cond: Cond) -> u32 {
        u32::from(Msr::new(
            u4::new(op1 as u8),
            0,
            u4::new(0b1111),
            u1::new(0),
            u1::new(0),
            u1::new(0),
            u1::new(1),
            u1::new(0),
            u1::new(1),
            u1::new(0),
            u2::new(0b10),
            u1::new(0),
            u2::new(0b00),
            u4::new(cond as u8),
        ))
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct MsrImm {
    pub imm_value: u8,
    pub shift: u4,
    pub id1: u4,
    pub c: u1,
    pub x: u1,
    pub s: u1,
    pub f: u1,
    pub id2: u1,
    pub opcode: u1,
    pub psr: u1,
    pub id3: u2,
    pub imm: u1,
    pub id4: u2,
    pub cond: u4,
}

impl MsrImm {
    pub fn cpsr_flags(imm: u8, shift: u8, cond: Cond) -> u32 {
        u32::from(MsrImm::new(
            imm,
            u4::new(shift),
            u4::new(0b1111),
            u1::new(0),
            u1::new(0),
            u1::new(0),
            u1::new(1),
            u1::new(0),
            u1::new(1),
            u1::new(0),
            u2::new(0b10),
            u1::new(0),
            u2::new(0b00),
            u4::new(cond as u8),
        ))
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct Mrs {
    pub rm: u12,
    pub rd: u4,
    pub id: u4,
    pub id1: u1,
    pub opcode: u1,
    pub psr: u1,
    pub id3: u2,
    pub imm: u1,
    pub id4: u2,
    pub cond: u4,
}

impl Mrs {
    pub fn cpsr(op0: Reg, cond: Cond) -> u32 {
        u32::from(Mrs::new(
            u12::new(0),
            u4::new(op0 as u8),
            u4::new(0b1111),
            u1::new(0),
            u1::new(0),
            u1::new(0),
            u2::new(0b10),
            u1::new(0),
            u2::new(0b00),
            u4::new(cond as u8),
        ))
    }
}
