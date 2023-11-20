use bilge::prelude::*;

#[bitsize(32)]
#[derive(FromBits)]
pub struct LdrImm {
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

impl LdrImm {
    pub fn create(
        imm_offset: u16,
        rd: u8,
        rn: u8,
        t_w: bool,
        byte: bool,
        add: bool,
        pre: bool,
        cond: u8,
    ) -> Self {
        LdrImm::new(
            u12::from(imm_offset),
            u4::from(rd),
            u4::from(rn),
            u1::from(1u8),
            u1::from(t_w),
            u1::from(byte),
            u1::from(add),
            u1::from(pre),
            u1::from(0u8),
            u2::from(1u8),
            u4::from(cond),
        )
    }
}
