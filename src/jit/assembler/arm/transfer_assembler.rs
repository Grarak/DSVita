use crate::jit::{Cond, Reg};
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
    ) -> Self {
        LdrStrImm::new(
            u12::from(imm_offset),
            u4::from(rd as u8),
            u4::from(rn as u8),
            u1::from(1u8),
            u1::from(t_w),
            u1::from(byte),
            u1::from(add),
            u1::from(pre),
            u1::from(0u8),
            u2::from(1u8),
            u4::from(cond as u8),
        )
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
    pub fn push(regs: &[Reg], cond: Cond) -> u32 {
        let mut rlist = 0u16;
        regs.iter().for_each(|reg| rlist |= 1 << (*reg as u8));
        u32::from(LdmStm::new(
            rlist,
            u4::new(Reg::SP as u8),
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
    pub fn push_al(regs: &[Reg]) -> u32 {
        LdmStm::push(regs, Cond::AL)
    }

    #[inline]
    pub fn pop(regs: &[Reg], cond: Cond) -> u32 {
        let mut rlist = 0u16;
        regs.iter().for_each(|reg| rlist |= 1 << (*reg as u8));
        u32::from(LdmStm::new(
            rlist,
            u4::new(Reg::SP as u8),
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
    pub fn pop_al(regs: &[Reg]) -> u32 {
        LdmStm::pop(regs, Cond::AL)
    }
}
