use std::fmt::{Debug, Formatter};
use std::{iter, mem, ops};

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Reg {
    R0 = 0,
    R1 = 1,
    R2 = 2,
    R3 = 3,
    R4 = 4,
    R5 = 5,
    R6 = 6,
    R7 = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    SP = 13,
    LR = 14,
    PC = 15,
    CPSR = 16,
    SPSR = 17,
    None = 18,
}

const GP_REGS_BITMASK: u32 = 0x1FFF;
const GP_THUMB_REGS_BITMASK: u32 = 0xFF;
const EMULATED_REGS_BITMASK: u32 = (1 << Reg::LR as u8) | (1 << Reg::PC as u8);
pub const FIRST_EMULATED_REG: Reg = Reg::LR;
pub const EMULATED_REGS_COUNT: usize = u32::count_ones(EMULATED_REGS_BITMASK) as usize;

impl From<u8> for Reg {
    fn from(value: u8) -> Self {
        debug_assert!(value < Reg::None as u8);
        unsafe { mem::transmute(value) }
    }
}

impl Reg {
    pub const fn is_emulated(self) -> bool {
        (EMULATED_REGS_BITMASK & (1 << self as u8)) != 0
    }

    pub fn is_high_gp_reg(self) -> bool {
        !self.is_emulated() && !RegReserve::gp_thumb().is_reserved(self) && self != Reg::SP
    }

    pub const fn is_call_preserved(self) -> bool {
        (self as u8 >= Reg::R4 as u8 && self as u8 <= Reg::R11 as u8) || self as u8 == Reg::SP as u8
    }
}

impl<'a> iter::Sum<&'a Reg> for RegReserve {
    fn sum<I: Iterator<Item = &'a Reg>>(iter: I) -> Self {
        let mut reg_reserve = RegReserve::new();
        for reg in iter {
            reg_reserve += *reg;
        }
        reg_reserve
    }
}

#[derive(Copy, Clone, Default)]
pub struct RegReserve(pub u32);

macro_rules! reg_reserve {
    ($($reg:expr),*) => {
        {
            #[allow(unused_mut)]
            let mut reg_reserve = crate::jit::reg::RegReserve::new();
            $(
                reg_reserve += ($reg);
            )*
            reg_reserve
        }
    };
}

pub(crate) use reg_reserve;

impl RegReserve {
    pub fn new() -> Self {
        RegReserve::default()
    }

    pub fn gp() -> Self {
        RegReserve(GP_REGS_BITMASK)
    }

    pub fn gp_thumb() -> Self {
        RegReserve(GP_THUMB_REGS_BITMASK)
    }

    pub fn caller_saved_gp() -> Self {
        reg_reserve!(Reg::R0, Reg::R1, Reg::R2, Reg::R3, Reg::R12)
    }

    pub fn is_reserved(&self, reg: Reg) -> bool {
        (self.0 >> reg as u8) & 1 == 1
    }

    pub fn next_free(&self) -> Option<Reg> {
        for i in Reg::R0 as u8..Reg::SPSR as u8 {
            let reg = Reg::from(i);
            if !self.is_reserved(reg) {
                return Some(reg);
            }
        }
        None
    }

    pub fn len(&self) -> usize {
        u32::count_ones(self.0) as _
    }

    pub fn emulated_regs_count(&self) -> u8 {
        u32::count_ones(self.0 & EMULATED_REGS_BITMASK) as _
    }

    pub fn get_emulated_regs(&self) -> RegReserve {
        RegReserve(self.0 & EMULATED_REGS_BITMASK)
    }

    pub fn get_gp_regs(&self) -> RegReserve {
        RegReserve(self.0 & GP_REGS_BITMASK)
    }

    pub fn peek(&self) -> Option<Reg> {
        for i in Reg::R0 as u8..Reg::SPSR as u8 {
            let reg = Reg::from(i);
            if self.is_reserved(reg) {
                return Some(reg);
            }
        }
        None
    }

    pub fn pop(&mut self) -> Option<Reg> {
        for i in Reg::R0 as u8..Reg::SPSR as u8 {
            let reg = Reg::from(i);
            if self.is_reserved(reg) {
                self.0 &= !(1 << i);
                return Some(reg);
            }
        }
        None
    }

    pub fn pop_call_reserved(&mut self) -> Option<Reg> {
        for i in Reg::R0 as u8..Reg::SPSR as u8 {
            let reg = Reg::from(i);
            if reg.is_call_preserved() && self.is_reserved(reg) {
                self.0 &= !(1 << i);
                return Some(reg);
            }
        }
        None
    }
}

impl ops::Add<RegReserve> for RegReserve {
    type Output = Self;

    fn add(self, rhs: RegReserve) -> Self::Output {
        RegReserve(self.0 | rhs.0)
    }
}

impl ops::AddAssign<RegReserve> for RegReserve {
    fn add_assign(&mut self, rhs: RegReserve) {
        self.0 |= rhs.0;
    }
}

impl ops::Sub<RegReserve> for RegReserve {
    type Output = Self;

    fn sub(self, rhs: RegReserve) -> Self::Output {
        RegReserve(self.0 & (!rhs.0))
    }
}

impl ops::SubAssign<RegReserve> for RegReserve {
    fn sub_assign(&mut self, rhs: RegReserve) {
        self.0 &= !rhs.0;
    }
}

impl ops::BitXor<RegReserve> for RegReserve {
    type Output = Self;

    fn bitxor(self, rhs: RegReserve) -> Self::Output {
        RegReserve(self.0 ^ rhs.0)
    }
}

impl ops::BitXorAssign<RegReserve> for RegReserve {
    fn bitxor_assign(&mut self, rhs: RegReserve) {
        self.0 ^= rhs.0;
    }
}

impl ops::BitAnd<RegReserve> for RegReserve {
    type Output = Self;

    fn bitand(self, rhs: RegReserve) -> Self::Output {
        RegReserve(self.0 & rhs.0)
    }
}

impl ops::BitAndAssign<RegReserve> for RegReserve {
    fn bitand_assign(&mut self, rhs: RegReserve) {
        self.0 &= rhs.0;
    }
}

impl ops::Not for RegReserve {
    type Output = RegReserve;

    fn not(self) -> Self::Output {
        RegReserve(!self.0)
    }
}

impl ops::Add<Reg> for RegReserve {
    type Output = RegReserve;

    fn add(self, rhs: Reg) -> Self::Output {
        RegReserve(self.0 | (1 << rhs as u8))
    }
}

impl ops::AddAssign<Reg> for RegReserve {
    fn add_assign(&mut self, rhs: Reg) {
        self.0 |= 1 << rhs as u8;
    }
}

impl ops::Sub<Reg> for RegReserve {
    type Output = RegReserve;

    fn sub(self, rhs: Reg) -> Self::Output {
        RegReserve(self.0 & !(1 << rhs as u8))
    }
}

impl ops::SubAssign<Reg> for RegReserve {
    fn sub_assign(&mut self, rhs: Reg) {
        self.0 &= !(1 << rhs as u8)
    }
}

impl ops::BitAnd<Reg> for RegReserve {
    type Output = RegReserve;

    fn bitand(self, rhs: Reg) -> Self::Output {
        RegReserve(self.0 & (1 << rhs as u8))
    }
}

impl ops::BitAndAssign<Reg> for RegReserve {
    fn bitand_assign(&mut self, rhs: Reg) {
        self.0 &= 1 << rhs as u8;
    }
}

impl From<u32> for RegReserve {
    fn from(value: u32) -> Self {
        RegReserve(value)
    }
}

impl Debug for RegReserve {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut str = "".to_owned();
        for i in Reg::R0 as u8..Reg::SPSR as u8 {
            let reg = Reg::from(i);
            if self.is_reserved(reg) {
                str += &format!("{:?}, ", reg);
            }
        }
        let mut chars = str.chars();
        chars.next_back();
        chars.next_back();
        f.write_str(chars.as_str())
    }
}

pub struct RegReserveIter {
    reserve: RegReserve,
    current: usize,
}

impl IntoIterator for RegReserve {
    type Item = Reg;
    type IntoIter = RegReserveIter;

    fn into_iter(self) -> Self::IntoIter {
        RegReserveIter { reserve: self, current: 0 }
    }
}

impl Iterator for RegReserveIter {
    type Item = <RegReserve as IntoIterator>::Item;

    fn next(&mut self) -> Option<Self::Item> {
        for i in self.current..Reg::None as usize {
            let reg = Reg::from(i as u8);
            if self.reserve.is_reserved(reg) {
                self.current = i + 1;
                return Some(reg);
            }
        }
        None
    }
}
