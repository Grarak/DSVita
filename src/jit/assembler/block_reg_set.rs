use crate::bitset::Bitset;
use crate::jit::assembler::{BlockReg, ANY_REG_LIMIT};
use crate::jit::reg::{Reg, RegReserve};
use crate::utils;
use std::fmt::{Debug, Formatter};
use std::hint::assert_unchecked;
use std::ops::{Add, AddAssign, BitAnd, BitXor, BitXorAssign, Not, Sub, SubAssign};

pub const BLOCK_REG_SET_ARRAY_SIZE: usize = 2;

#[derive(Copy, Clone, Default, Eq, PartialEq)]
pub struct BlockRegSet(Bitset<BLOCK_REG_SET_ARRAY_SIZE>);

macro_rules! block_reg_set {
    ($($reg:expr),*) => {
        {
            #[allow(unused_mut)]
            let mut set = crate::jit::assembler::block_reg_set::BlockRegSet::new();
            $(
                let reg: Option<crate::jit::assembler::BlockReg> = $reg;
                if let Some(reg) = reg {
                    set += reg;
                }
            )*
            set
        }
    };
}
pub(crate) use block_reg_set;

impl BlockRegSet {
    pub const fn new() -> Self {
        BlockRegSet(Bitset::new())
    }

    pub const fn new_fixed(reg_reserve: RegReserve) -> Self {
        let mut set = BlockRegSet::new();
        set.0 .0[0] = reg_reserve.0 & ((1 << Reg::None as u8) - 1);
        set
    }

    pub fn contains(&self, reg: BlockReg) -> bool {
        self.0.contains(reg.get_id())
    }

    pub fn get_guests(&self) -> RegReserve {
        let guest_regs = (self.0 .0[0] >> Reg::None as u8) & ((1 << Reg::None as u8) - 1);
        let spilled_over_count = Reg::None as u8 * 2 - 32;
        let guest_regs = guest_regs | ((self.0 .0[1] & ((1 << spilled_over_count) - 1)) << (Reg::None as u8 - spilled_over_count));
        RegReserve::from(guest_regs)
    }

    pub fn get_fixed(&self) -> RegReserve {
        RegReserve::from(self.0 .0[0] & ((1 << Reg::None as u8) - 1))
    }

    pub const fn add_guests(&mut self, reg_reserve: RegReserve) {
        self.0 .0[0] |= reg_reserve.0 << Reg::None as u8;
        let spilled_over_count = Reg::None as u8 * 2 - 32;
        self.0 .0[1] |= reg_reserve.0 >> (Reg::None as u8 - spilled_over_count);
    }

    pub const fn remove_guests(&mut self, reg_reserve: RegReserve) {
        self.0 .0[0] &= !(reg_reserve.0 << Reg::None as u8);
        let spilled_over_count = Reg::None as u8 * 2 - 32;
        self.0 .0[1] &= !(reg_reserve.0 >> (Reg::None as u8 - spilled_over_count));
    }

    pub const fn clear(&mut self) {
        self.0.clear();
    }

    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter_any(&self) -> BlockRegAnySetIter {
        let set = Bitset([self.0 .0[0] >> Reg::None as u8, self.0 .0[1]]);
        BlockRegAnySetIter { set, current: 0 }
    }
}

impl Add<BlockReg> for BlockRegSet {
    type Output = BlockRegSet;

    fn add(mut self, rhs: BlockReg) -> Self::Output {
        self.0 += rhs.get_id();
        self
    }
}

impl AddAssign<BlockReg> for BlockRegSet {
    fn add_assign(&mut self, rhs: BlockReg) {
        self.0 += rhs.get_id();
    }
}

impl Sub<BlockReg> for BlockRegSet {
    type Output = BlockRegSet;

    fn sub(mut self, rhs: BlockReg) -> Self::Output {
        self.0 -= rhs.get_id();
        self
    }
}

impl SubAssign<BlockReg> for BlockRegSet {
    fn sub_assign(&mut self, rhs: BlockReg) {
        self.0 -= rhs.get_id()
    }
}

impl Add<BlockRegSet> for BlockRegSet {
    type Output = BlockRegSet;

    fn add(mut self, rhs: BlockRegSet) -> Self::Output {
        self.0 += rhs.0;
        self
    }
}

impl Add<&BlockRegSet> for BlockRegSet {
    type Output = BlockRegSet;

    fn add(mut self, rhs: &BlockRegSet) -> Self::Output {
        self.0 += rhs.0;
        self
    }
}

impl AddAssign<BlockRegSet> for BlockRegSet {
    fn add_assign(&mut self, rhs: BlockRegSet) {
        self.0 += rhs.0;
    }
}

impl Sub<BlockRegSet> for BlockRegSet {
    type Output = BlockRegSet;

    fn sub(mut self, rhs: BlockRegSet) -> Self::Output {
        self.0 -= rhs.0;
        self
    }
}

impl Sub<&BlockRegSet> for BlockRegSet {
    type Output = BlockRegSet;

    fn sub(mut self, rhs: &BlockRegSet) -> Self::Output {
        self.0 -= rhs.0;
        self
    }
}

impl SubAssign<BlockRegSet> for BlockRegSet {
    fn sub_assign(&mut self, rhs: BlockRegSet) {
        self.0 -= rhs.0;
    }
}

impl SubAssign<&BlockRegSet> for BlockRegSet {
    fn sub_assign(&mut self, rhs: &BlockRegSet) {
        self.0 -= rhs.0;
    }
}

impl Sub<RegReserve> for BlockRegSet {
    type Output = BlockRegSet;

    fn sub(mut self, rhs: RegReserve) -> Self::Output {
        self.0 .0[0] &= !rhs.0;
        self
    }
}

impl SubAssign<RegReserve> for BlockRegSet {
    fn sub_assign(&mut self, rhs: RegReserve) {
        self.0 .0[0] &= !rhs.0;
    }
}

impl BitAnd<BlockRegSet> for BlockRegSet {
    type Output = BlockRegSet;

    fn bitand(self, rhs: BlockRegSet) -> Self::Output {
        BlockRegSet(self.0 & rhs.0)
    }
}

impl BitXor<BlockRegSet> for BlockRegSet {
    type Output = BlockRegSet;

    fn bitxor(self, rhs: BlockRegSet) -> Self::Output {
        BlockRegSet(self.0 ^ rhs.0)
    }
}

impl BitXorAssign for BlockRegSet {
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0
    }
}

impl Not for BlockRegSet {
    type Output = BlockRegSet;

    fn not(self) -> Self::Output {
        BlockRegSet(!self.0)
    }
}

impl PartialEq<&BlockRegSet> for BlockRegSet {
    fn eq(&self, other: &&BlockRegSet) -> bool {
        for i in 0..self.0 .0.len() {
            if self.0 .0[i] != other.0 .0[i] {
                return false;
            }
        }
        true
    }
}

impl Debug for BlockRegSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug_set = f.debug_set();
        for i in Reg::R0 as u8..Reg::None as u8 {
            let reg = BlockReg::Fixed(Reg::from(i));
            if self.contains(reg) {
                debug_set.entry(&reg);
            }
        }
        for i in 0..ANY_REG_LIMIT {
            let reg = BlockReg::Any(i);
            if self.contains(reg) {
                debug_set.entry(&reg);
            }
        }
        debug_set.finish()
    }
}

pub struct BlockRegAnySetIter<'a> {
    set: Bitset<BLOCK_REG_SET_ARRAY_SIZE>,
    current: u16,
}

impl Iterator for BlockRegAnySetIter<'_> {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.current + Reg::None as u16;
        let array_index = (id >> 5) as usize;
        unsafe { assert_unchecked(array_index < self.set.0.len()) };
        let zeros = self.set.0[array_index].trailing_zeros();
        self.set.0[array_index] = self.set.0[array_index].unbounded_shr(zeros);
        if self.set.0[array_index] == 0 {
            if array_index == self.set.0.len() - 1 {
                None
            } else {
                self.current = utils::align_up(id as usize, 32) as u16 - Reg::None as u16;
                self.next()
            }
        } else {
            self.current += zeros as u16;
            let reg = self.current;
            self.set.0[array_index] >>= 1;
            self.current += 1;
            unsafe { assert_unchecked(reg < ANY_REG_LIMIT) };
            Some(reg)
        }
    }
}
