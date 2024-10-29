use crate::bitset::Bitset;
use crate::jit::assembler::{BlockReg, ANY_REG_LIMIT};
use crate::jit::reg::{Reg, RegReserve};
use std::fmt::{Debug, Formatter};
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

    pub const fn len_any(&self) -> usize {
        let mut sum = 0;
        let mut i = 0;
        while i < self.0 .0.len() {
            sum += self.0 .0[i].count_ones();
            i += 1;
        }
        const FIXED_REGS_OVERFLOW: u8 = Reg::None as u8;
        (sum - (self.0 .0[0] & ((1 << FIXED_REGS_OVERFLOW) - 1)).count_ones()) as usize
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
        self.0.len() == 0
    }

    pub fn iter_fixed(&self) -> BlockRegFixedSetIter {
        BlockRegFixedSetIter { block_reg_set: self, current: 0 }
    }

    pub fn iter_any(&self) -> BlockRegAnySetIter {
        BlockRegAnySetIter {
            block_reg_set: self,
            current: 0,
            found: 0,
            len: self.len_any(),
        }
    }

    pub fn iter(&self) -> BlockRegSetIter {
        BlockRegSetIter {
            block_reg_set: self,
            current: 0,
            found: 0,
            len: self.0.len(),
        }
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

impl SubAssign<BlockRegSet> for BlockRegSet {
    fn sub_assign(&mut self, rhs: BlockRegSet) {
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

pub struct BlockRegFixedSetIter<'a> {
    block_reg_set: &'a BlockRegSet,
    current: u8,
}

impl<'a> Iterator for BlockRegFixedSetIter<'a> {
    type Item = Reg;

    fn next(&mut self) -> Option<Self::Item> {
        for i in self.current..Reg::None as u8 {
            let reg = Reg::from(i);
            if self.block_reg_set.contains(BlockReg::Fixed(reg)) {
                self.current = i + 1;
                return Some(reg);
            }
        }
        None
    }
}

pub struct BlockRegAnySetIter<'a> {
    block_reg_set: &'a BlockRegSet,
    current: u16,
    found: usize,
    len: usize,
}

impl Iterator for BlockRegAnySetIter<'_> {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        if self.found == self.len {
            None
        } else {
            for i in self.current..ANY_REG_LIMIT {
                if self.block_reg_set.contains(BlockReg::Any(i)) {
                    self.current = i + 1;
                    self.found += 1;
                    return Some(i);
                }
            }
            None
        }
    }
}

pub struct BlockRegSetIter<'a> {
    block_reg_set: &'a BlockRegSet,
    current: u16,
    found: usize,
    len: usize,
}

impl Iterator for BlockRegSetIter<'_> {
    type Item = BlockReg;

    fn next(&mut self) -> Option<Self::Item> {
        if self.found == self.len {
            None
        } else {
            const LAST_FIXED: u16 = BlockReg::Fixed(Reg::SPSR).get_id();
            const LAST_ANY: u16 = BlockReg::Any(ANY_REG_LIMIT - 1).get_id();

            for i in self.current..=LAST_FIXED {
                let reg = BlockReg::Fixed(Reg::from(i as u8));
                if self.block_reg_set.contains(reg) {
                    self.current = i + 1;
                    self.found += 1;
                    return Some(reg);
                }
            }
            if self.current <= LAST_FIXED {
                self.current = LAST_FIXED + 1;
            }
            for i in self.current..=LAST_ANY {
                let reg = BlockReg::Any(i - LAST_FIXED - 1);
                if self.block_reg_set.contains(reg) {
                    self.current = i + 1;
                    self.found += 1;
                    return Some(reg);
                }
            }
            None
        }
    }
}
