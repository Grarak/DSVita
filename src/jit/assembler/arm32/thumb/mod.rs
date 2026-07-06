use crate::jit::reg::Reg;
use bilge::prelude::*;

pub const NOP: u16 = 0b1011111100000000;

#[bitsize(16)]
#[derive(FromBits)]
pub struct MovsThumb {
    imm: u8,
    rd: u3,
    id: u5,
}

impl MovsThumb {
    pub fn movs8(rd: Reg, imm: u8) -> u16 {
        debug_assert!(rd <= Reg::R6);
        u16::from(MovsThumb::new(imm, u3::from(rd as u8), u5::new(0b00100)))
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct Mov {
    imm4: u4,
    id2: u6,
    imm1: u1,
    id: u5,
    imm8: u8,
    rd: u4,
    imm3: u3,
    id3: u1,
}

impl Mov {
    pub fn mov16(rd: Reg, imm: u16) -> u32 {
        u32::from(Mov::new(
            u4::new((imm >> 12) as u8),
            u6::new(0b100100),
            u1::new(((imm >> 11) & 0x1) as u8),
            u5::new(0b11110),
            imm as u8,
            u4::new(rd as u8),
            u3::new(((imm >> 8) & 0x7) as u8),
            u1::new(0),
        ))
    }

    pub fn mov_t(rd: Reg, imm: u16) -> u32 {
        u32::from(Mov::new(
            u4::new((imm >> 12) as u8),
            u6::new(0b101100),
            u1::new(((imm >> 11) & 0x1) as u8),
            u5::new(0b11110),
            imm as u8,
            u4::new(rd as u8),
            u3::new(((imm >> 8) & 0x7) as u8),
            u1::new(0),
        ))
    }

    pub fn mov32(rd: Reg, imm: u32) -> ([u32; 2], usize) {
        if imm & 0xFFFF0000 == 0 {
            ([Self::mov16(rd, imm as u16), 0], 1)
        } else {
            ([Self::mov16(rd, imm as u16), Self::mov_t(rd, (imm >> 16) as u16)], 2)
        }
    }
}

#[bitsize(16)]
#[derive(FromBits)]
pub struct MovReg {
    rd: u3,
    rm: u4,
    msb_rd: u1,
    opcode: u2,
    id: u6,
}

impl MovReg {
    pub fn mov(rd: Reg, rm: Reg) -> u16 {
        u16::from(MovReg::new(u3::new((rd as u8) & 0x7), u4::new(rm as u8), u1::new((rd as u8) >> 3), u2::new(2), u6::new(0b010001)))
    }
}

#[bitsize(16)]
#[derive(FromBits)]
pub struct BlxReg {
    id2: u3,
    rm: u4,
    id: u9,
}

impl BlxReg {
    pub fn blx_reg(rm: Reg) -> u16 {
        u16::from(BlxReg::new(u3::new(0b000), u4::new(rm as u8), u9::new(0b010001111)))
    }
}
