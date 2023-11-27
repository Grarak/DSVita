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
    pub fn push(regs: RegReserve, sp: Reg, cond: Cond) -> u32 {
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
    pub fn push_al(regs: RegReserve) -> u32 {
        LdmStm::push(regs, Reg::SP, Cond::AL)
    }

    #[inline]
    pub fn pop(regs: RegReserve, sp: Reg, cond: Cond) -> u32 {
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
    pub fn pop_al(regs: RegReserve) -> u32 {
        LdmStm::pop(regs, Reg::SP, Cond::AL)
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
pub struct Bx {
    pub rn: u4,
    pub op: u4,
    pub id: u20,
    pub u4: u4,
}

impl Bx {
    pub fn bx(op0: Reg, cond: Cond) -> u32 {
        u32::from(Bx::new(
            u4::new(op0 as u8),
            u4::new(1),
            u20::new(0b00010010111111111111),
            u4::new(cond as u8),
        ))
    }
}
