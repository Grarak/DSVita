use crate::jit::{Cond, Reg};
use bilge::prelude::*;

#[bitsize(32)]
#[derive(FromBits)]
pub struct Bx {
    pub rn: u4,
    pub op: u4,
    pub id: u20,
    pub cond: u4,
}

impl Bx {
    #[inline]
    pub fn create(rn: Reg, cond: Cond) -> u32 {
        u32::from(Bx::new(
            u4::new(rn as u8),
            u4::new(1),
            u20::new(0b0001_0010_1111_1111_1111),
            u4::new(cond as u8),
        ))
    }
}
