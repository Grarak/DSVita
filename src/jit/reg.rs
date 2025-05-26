use std::fmt::{Debug, Formatter};
use std::intrinsics::unlikely;
use std::{iter, mem, ops};

#[derive(Copy, Clone, Debug, Default, Eq, Hash, PartialEq, PartialOrd)]
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
    #[default]
    None = 18,
}

const GP_REGS_BITMASK: u32 = 0x1FFF;
const GP_LR_REGS_BITMASK: u32 = 0x5FFF;
const GP_THUMB_REGS_BITMASK: u32 = 0xFF;

impl From<u8> for Reg {
    fn from(value: u8) -> Self {
        debug_assert!(value < Reg::None as u8);
        unsafe { mem::transmute(value) }
    }
}

impl Reg {
    pub const fn is_call_preserved(self) -> bool {
        (self as u8 >= Reg::R4 as u8 && self as u8 <= Reg::R11 as u8) || self as u8 == Reg::SP as u8
    }

    pub const fn is_low(self) -> bool {
        self as u8 <= Reg::R7 as u8
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

#[derive(Copy, Clone, Default, Eq, PartialEq)]
pub struct RegReserve(pub u32);

macro_rules! reg_reserve {
    ($($reg:expr),*) => {{
        #[allow(unused_mut)]
        let mut reg_reserve = crate::jit::reg::RegReserve::new();
        $(
            reg_reserve.reserve($reg);
        )*
        reg_reserve
    }};
}

pub(crate) use reg_reserve;

impl RegReserve {
    pub const fn new() -> Self {
        RegReserve(0)
    }

    pub const fn gp() -> Self {
        RegReserve(GP_REGS_BITMASK)
    }

    pub const fn all() -> Self {
        RegReserve(0xFFFF)
    }

    pub fn gp_thumb() -> Self {
        RegReserve(GP_THUMB_REGS_BITMASK)
    }

    pub const fn reserve(&mut self, reg: Reg) {
        self.0 |= 1 << (reg as u8);
    }

    pub fn is_reserved(self, reg: Reg) -> bool {
        (self.0 >> reg as u8) & 1 == 1
    }

    pub fn next_gp_free(self) -> Option<Reg> {
        let count = self.0.trailing_ones();
        if count >= Reg::SP as u32 {
            None
        } else {
            Some(Reg::from(count as u8))
        }
    }

    pub fn peek_gp(self) -> Option<Reg> {
        let count = self.0.trailing_zeros();
        if count >= Reg::SP as u32 {
            None
        } else {
            Some(Reg::from(count as u8))
        }
    }

    pub fn peek(self) -> Option<Reg> {
        let count = self.0.trailing_zeros();
        if count >= Reg::CPSR as u32 {
            None
        } else {
            Some(Reg::from(count as u8))
        }
    }

    pub const fn len(self) -> usize {
        u32::count_ones(self.0) as _
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn get_gp_regs(self) -> RegReserve {
        RegReserve(self.0 & GP_REGS_BITMASK)
    }

    pub const fn get_gp_lr_regs(self) -> RegReserve {
        RegReserve(self.0 & GP_LR_REGS_BITMASK)
    }

    pub fn clear(&mut self) {
        self.0 = 0;
    }

    pub fn get_highest_reg(self) -> Reg {
        Reg::from(32 - self.0.leading_zeros() as u8 - 1)
    }

    pub fn get_lowest_reg(self) -> Reg {
        Reg::from(self.0.trailing_zeros() as u8)
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

impl ops::BitOrAssign for RegReserve {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl From<u32> for RegReserve {
    fn from(value: u32) -> Self {
        RegReserve(value)
    }
}

impl Debug for RegReserve {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug_set = f.debug_set();
        for i in Reg::R0 as u8..Reg::None as u8 {
            let reg = Reg::from(i);
            if self.is_reserved(reg) {
                debug_set.entry(&reg);
            }
        }
        debug_set.finish()
    }
}

#[derive(Clone)]
pub struct RegReserveIter {
    reserve: u32,
}

impl IntoIterator for RegReserve {
    type Item = Reg;
    type IntoIter = RegReserveIter;

    fn into_iter(self) -> Self::IntoIter {
        RegReserveIter { reserve: self.0.reverse_bits() }
    }
}

impl Iterator for RegReserveIter {
    type Item = <RegReserve as IntoIterator>::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if unlikely(self.reserve == 0) {
            None
        } else {
            let zeros = self.reserve.leading_zeros();
            let reg = Reg::from(zeros as u8);
            self.reserve &= !(0x80000000 >> zeros);
            Some(reg)
        }
    }
}

impl FromIterator<Reg> for RegReserve {
    fn from_iter<T: IntoIterator<Item = Reg>>(iter: T) -> Self {
        let mut reg_reserve = RegReserve::new();
        for reg in iter {
            reg_reserve += reg;
        }
        reg_reserve
    }
}
