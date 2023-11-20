use bilge::prelude::*;

#[bitsize(32)]
#[derive(FromBits)]
pub struct MovImm {
    pub nn: u8,
    pub ror: u4,
    pub rd: u4,
    pub rn: u4,
    pub set: u1,
    pub op: u4,
    pub imm: u1,
    pub id: u2,
    pub cond: u4,
}

impl MovImm {
    pub fn create(nn: u8, ror: u8, rd: u8, cond: u8) -> u32 {
        u32::from(MovImm::new(
            u8::new(nn),
            u4::new(ror),
            u4::new(rd),
            u4::new(0),
            u1::new(0),
            u4::new(0xD),
            u1::new(1),
            u2::new(0),
            u4::new(cond),
        ))
    }
}
