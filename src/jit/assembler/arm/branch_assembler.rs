use crate::jit::reg::Reg;
use crate::jit::Cond;
use bilge::prelude::*;

#[bitsize(32)]
#[derive(FromBits)]
pub struct B {
    pub nn: u24,
    pub op: u1,
    pub id: u3,
    pub u4: u4,
}

impl B {
    pub fn b(imm: i32, cond: Cond) -> u32 {
        u32::from(B::new(
            // Extract first 24 bits, also keep msb
            u24::new((((imm << 8) >> 8) & 0xFFFFFF) as u32),
            u1::new(0),
            u3::new(0b101),
            u4::new(cond as u8),
        ))
    }

    pub fn bl(imm: i32, cond: Cond) -> u32 {
        u32::from(B::new(
            // Extract first 24 bits, also keep msb
            u24::new((((imm << 8) >> 8) & 0xFFFFFF) as u32),
            u1::new(1),
            u3::new(0b101),
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
        u32::from(Bx::new(u4::new(op0 as u8), u4::new(0b1), u20::new(0b00010010111111111111), u4::new(cond as u8)))
    }

    pub fn blx(op0: Reg, cond: Cond) -> u32 {
        u32::from(Bx::new(u4::new(op0 as u8), u4::new(0b11), u20::new(0b00010010111111111111), u4::new(cond as u8)))
    }
}
