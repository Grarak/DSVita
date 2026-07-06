use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, MemoryAmount, ShiftType};
use crate::logging::debug_panic;
use bilge::prelude::*;

#[bitsize(32)]
#[derive(FromBits)]
pub struct LdrStrImm {
    pub imm_offset: u12,
    pub rd: u4,
    pub rn: u4,
    pub read: bool,
    pub write_back: bool,
    pub is_byte: bool,
    pub add_to_base: bool,
    pub pre: bool,
    pub reg_offset: bool,
    pub id: u2,
    pub cond: u4,
}

impl LdrStrImm {
    #[inline]
    pub fn generic(rd: Reg, rn: Reg, imm_offset: u16, read: bool, write_back: bool, byte: bool, add: bool, pre: bool, cond: Cond) -> u32 {
        u32::from(LdrStrImm::new(
            u12::new(imm_offset),
            u4::new(rd as u8),
            u4::new(rn as u8),
            read,
            write_back,
            byte,
            add,
            pre,
            false,
            u2::new(1),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn ldr(rd: Reg, rn: Reg, imm_offset: u16, write_back: bool, byte: bool, add: bool, pre: bool, cond: Cond) -> u32 {
        Self::generic(rd, rn, imm_offset, true, write_back, byte, add, pre, cond)
    }

    #[inline]
    pub fn ldr_offset_al(op0: Reg, op1: Reg, offset: u16) -> u32 {
        Self::ldr(op0, op1, offset, false, false, true, true, Cond::AL)
    }

    #[inline]
    pub fn ldrb_offset_al(op0: Reg, op1: Reg, offset: u16) -> u32 {
        Self::ldr(op0, op1, offset, false, true, true, true, Cond::AL)
    }

    #[inline]
    pub fn ldr_sub_offset_al(op0: Reg, op1: Reg, offset: u16) -> u32 {
        Self::ldr(op0, op1, offset, false, false, false, true, Cond::AL)
    }

    #[inline]
    pub fn ldr_al(op0: Reg, op1: Reg) -> u32 {
        Self::ldr_offset_al(op0, op1, 0)
    }

    #[inline]
    pub fn str(imm_offset: u16, rd: Reg, rn: Reg, write_back: bool, byte: bool, add: bool, pre: bool, cond: Cond) -> u32 {
        Self::generic(rd, rn, imm_offset, false, write_back, byte, add, pre, cond)
    }

    #[inline]
    pub fn str_al(op0: Reg, op1: Reg) -> u32 {
        Self::str_offset_al(op0, op1, 0)
    }

    #[inline]
    pub fn str_offset_al(op0: Reg, op1: Reg, offset: u16) -> u32 {
        Self::str(offset, op0, op1, false, false, true, true, Cond::AL)
    }

    #[inline]
    pub fn strb_offset_al(op0: Reg, op1: Reg, offset: u16) -> u32 {
        Self::str(offset, op0, op1, false, true, true, true, Cond::AL)
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct LdrStrReg {
    pub rm: u4,
    id: u1,
    pub shift_type: u2,
    pub shift_amount: u5,
    pub rd: u4,
    pub rn: u4,
    pub read: bool,
    pub write_back: bool,
    pub is_byte: bool,
    pub add_to_base: bool,
    pub pre: bool,
    pub reg_offset: bool,
    id2: u2,
    pub cond: u4,
}

impl LdrStrReg {
    #[inline]
    pub fn generic(op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, shift_type: ShiftType, read: bool, write_back: bool, byte: bool, add: bool, pre: bool, cond: Cond) -> u32 {
        u32::from(Self::new(
            u4::new(op2 as u8),
            u1::new(0),
            u2::new(shift_type as u8),
            u5::new(shift_amount),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            read,
            write_back,
            byte,
            add,
            pre,
            true,
            u2::new(0b01),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn ldrb(op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, shift_type: ShiftType, write_back: bool, add: bool, pre: bool, cond: Cond) -> u32 {
        Self::generic(op0, op1, op2, shift_amount, shift_type, true, write_back, true, add, pre, cond)
    }

    #[inline]
    pub fn ldrb_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::ldrb(op0, op1, op2, 0, ShiftType::Lsl, false, true, true, Cond::AL)
    }

    #[inline]
    pub fn ldr(op0: Reg, op1: Reg, op2: Reg, shift_amount: u8, shift_type: ShiftType, write_back: bool, add: bool, pre: bool, cond: Cond) -> u32 {
        Self::generic(op0, op1, op2, shift_amount, shift_type, true, write_back, false, add, pre, cond)
    }

    #[inline]
    pub fn ldr_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::ldr(op0, op1, op2, 0, ShiftType::Lsl, false, true, true, Cond::AL)
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct LdrStrRegSBHD {
    pub rm: u4,
    id: u1,
    pub opcode: u2,
    id2: u5,
    pub rd: u4,
    pub rn: u4,
    pub read: bool,
    pub write_back: bool,
    pub imm: bool,
    pub add_to_base: bool,
    pub pre: bool,
    id3: u3,
    pub cond: u4,
}

impl LdrStrRegSBHD {
    #[inline]
    pub fn generic(op0: Reg, op1: Reg, op2: Reg, signed: bool, amount: MemoryAmount, mut read: bool, write_back: bool, add: bool, pre: bool, cond: Cond) -> u32 {
        let opcode = match amount {
            MemoryAmount::Byte => match (signed, read) {
                (true, true) => 2,
                _ => debug_panic!("invalid combination signed: {signed} amount: {amount:?}"),
            },
            MemoryAmount::Half => match (signed, read) {
                (false, false) => 1,
                (false, true) => 1,
                (true, true) => 3,
                _ => debug_panic!("invalid combination signed: {signed} amount: {amount:?}"),
            },
            MemoryAmount::Word => debug_panic!("invalid combination signed: {signed} amount: {amount:?}"),
            MemoryAmount::Double => match (signed, read) {
                (false, false) => 3,
                (false, true) => {
                    read = false;
                    2
                }
                _ => debug_panic!("invalid combination signed: {signed} amount: {amount:?}"),
            },
        };
        u32::from(Self::new(
            u4::new(op2 as u8),
            u1::new(1),
            u2::new(opcode),
            u5::new(1),
            u4::new(op0 as u8),
            u4::new(op1 as u8),
            read,
            write_back,
            false,
            add,
            pre,
            u3::new(0),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn ldrsb(op0: Reg, op1: Reg, op2: Reg, cond: Cond) -> u32 {
        Self::generic(op0, op1, op2, true, MemoryAmount::Byte, true, false, true, true, cond)
    }

    #[inline]
    pub fn ldrsb_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::ldrsb(op0, op1, op2, Cond::AL)
    }

    #[inline]
    pub fn ldrh(op0: Reg, op1: Reg, op2: Reg, cond: Cond) -> u32 {
        Self::generic(op0, op1, op2, false, MemoryAmount::Half, true, false, true, true, cond)
    }

    #[inline]
    pub fn ldrh_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::ldrh(op0, op1, op2, Cond::AL)
    }

    #[inline]
    pub fn ldrsh(op0: Reg, op1: Reg, op2: Reg, cond: Cond) -> u32 {
        Self::generic(op0, op1, op2, true, MemoryAmount::Half, true, false, true, true, cond)
    }

    #[inline]
    pub fn ldrsh_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::ldrsh(op0, op1, op2, Cond::AL)
    }

    #[inline]
    pub fn ldrd(op0: Reg, op1: Reg, op2: Reg, cond: Cond) -> u32 {
        Self::generic(op0, op1, op2, false, MemoryAmount::Double, true, false, true, true, cond)
    }

    #[inline]
    pub fn ldrd_al(op0: Reg, op1: Reg, op2: Reg) -> u32 {
        Self::ldrd(op0, op1, op2, Cond::AL)
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct LdrStrImmSBHD {
    pub imm_lower: u4,
    id: u1,
    pub opcode: u2,
    id2: u1,
    pub imm_upper: u4,
    pub rd: u4,
    pub rn: u4,
    pub read: bool,
    pub write_back: bool,
    pub imm: bool,
    pub add_to_base: bool,
    pub pre: bool,
    id3: u3,
    pub cond: u4,
}

impl LdrStrImmSBHD {
    #[inline]
    pub fn generic(op0: Reg, op1: Reg, op2: u8, signed: bool, amount: MemoryAmount, read: bool, write_back: bool, add: bool, pre: bool, cond: Cond) -> u32 {
        let mut opcode = Self::from(LdrStrRegSBHD::generic(op0, op1, Reg::R0, signed, amount, read, write_back, add, pre, cond));
        opcode.set_imm_lower(u4::new(op2 & 0xF));
        opcode.set_imm_upper(u4::new(op2 >> 4));
        opcode.set_imm(true);
        u32::from(opcode)
    }

    #[inline]
    pub fn ldrd(op0: Reg, op1: Reg, op2: u8, cond: Cond) -> u32 {
        Self::generic(op0, op1, op2, false, MemoryAmount::Double, true, false, true, true, cond)
    }

    #[inline]
    pub fn ldrd_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        Self::ldrd(op0, op1, op2, Cond::AL)
    }

    #[inline]
    pub fn strh(op0: Reg, op1: Reg, op2: u8, cond: Cond) -> u32 {
        Self::generic(op0, op1, op2, false, MemoryAmount::Half, false, false, true, true, cond)
    }

    #[inline]
    pub fn strh_al(op0: Reg, op1: Reg, op2: u8) -> u32 {
        Self::strh(op0, op1, op2, Cond::AL)
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct LdmStm {
    pub rlist: u16,
    pub rn: u4,
    pub read: bool,
    pub write_back: bool,
    pub psr: bool,
    pub add_to_base: bool,
    pub pre: bool,
    pub id: u3,
    pub cond: u4,
}

impl LdmStm {
    #[inline]
    pub fn generic(op0: Reg, regs: RegReserve, read: bool, write_back: bool, add_to_base: bool, pre: bool, cond: Cond) -> u32 {
        debug_assert!(!write_back || !regs.is_reserved(op0));
        u32::from(LdmStm::new(
            regs.0 as u16,
            u4::new(op0 as u8),
            read,
            write_back,
            false,
            add_to_base,
            pre,
            u3::new(0b100),
            u4::new(cond as u8),
        ))
    }

    #[inline]
    pub fn push_post(regs: RegReserve, sp: Reg, cond: Cond) -> u32 {
        debug_assert!(!regs.is_reserved(sp));
        u32::from(LdmStm::new(regs.0 as u16, u4::new(sp as u8), false, true, false, false, false, u3::new(0b100), u4::new(cond as u8)))
    }

    #[inline]
    pub fn push_post_al(regs: RegReserve) -> u32 {
        LdmStm::push_post(regs, Reg::SP, Cond::AL)
    }

    #[inline]
    pub fn push_pre(regs: RegReserve, sp: Reg, cond: Cond) -> u32 {
        debug_assert!(!regs.is_reserved(sp));
        u32::from(LdmStm::new(regs.0 as u16, u4::new(sp as u8), false, true, false, false, true, u3::new(0b100), u4::new(cond as u8)))
    }

    #[inline]
    pub fn pop_post(regs: RegReserve, sp: Reg, cond: Cond) -> u32 {
        debug_assert!(!regs.is_reserved(sp));
        u32::from(LdmStm::new(regs.0 as u16, u4::new(sp as u8), true, true, false, true, false, u3::new(0b100), u4::new(cond as u8)))
    }

    #[inline]
    pub fn pop_post_al(regs: RegReserve) -> u32 {
        LdmStm::pop_post(regs, Reg::SP, Cond::AL)
    }

    #[inline]
    pub fn pop_pre(regs: RegReserve, sp: Reg, cond: Cond) -> u32 {
        debug_assert!(!regs.is_reserved(sp));
        u32::from(LdmStm::new(regs.0 as u16, u4::new(sp as u8), true, true, false, true, true, u3::new(0b100), u4::new(cond as u8)))
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

#[bitsize(32)]
#[derive(FromBits)]
pub struct Preload {
    pub imm: u12,
    pub id: u4,
    pub rn: u4,
    pub id1: u3,
    pub add: bool,
    pub id2: u8,
}

impl Preload {
    pub fn pli(op0: Reg, imm: u16, add: bool) -> u32 {
        u32::from(Preload::new(u12::new(imm), u4::new(0b1111), u4::new(op0 as u8), u3::new(0b101), add, 0b11110100))
    }
}
