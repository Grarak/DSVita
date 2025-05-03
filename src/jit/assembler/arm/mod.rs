use bilge::prelude::*;

pub mod alu_assembler;
pub mod branch_assembler;
pub mod transfer_assembler;

pub const NOP: u32 = 0xe320f000;

#[bitsize(32)]
#[derive(FromBits)]
pub struct Bkpt {
    imm_lower: u4,
    id2: u4,
    imm_upper: u12,
    id: u12,
}

impl Bkpt {
    pub fn bkpt(id: u16) -> u32 {
        u32::from(Bkpt::new(u4::new((id & 0xF) as u8), u4::new(0b0111), u12::new(id >> 4), u12::new(0b111000010010)))
    }
}
