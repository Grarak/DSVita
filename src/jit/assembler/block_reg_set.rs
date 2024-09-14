use crate::jit::assembler::{BlockReg, ANY_REG_LIMIT};
use crate::jit::reg::{Reg, RegReserve};
use std::fmt::{Debug, Formatter};
use std::ops::{Add, AddAssign, BitAnd, BitXor, BitXorAssign, Not, Sub, SubAssign};

pub const BLOCK_REG_SET_ARRAY_SIZE: usize = 4;

#[derive(Copy, Clone, Default, Eq, PartialEq)]
pub struct BlockRegSet([u32; BLOCK_REG_SET_ARRAY_SIZE]);

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
        BlockRegSet([0; BLOCK_REG_SET_ARRAY_SIZE])
    }

    pub const fn new_fixed(reg_reserve: RegReserve) -> Self {
        let mut set = BlockRegSet::new();
        set.0[0] = (reg_reserve.0 & ((1 << Reg::None as u8) - 1)) << Reg::None as u8;
        set
    }

    fn _add(&mut self, reg: BlockReg) {
        let id = reg.get_id();
        let array_index = (id >> 5) as usize;
        let pos_index = (id & 31) as usize;
        self.0[array_index] |= 1 << pos_index;
    }

    fn _sub(&mut self, reg: BlockReg) {
        let id = reg.get_id();
        let array_index = (id >> 5) as usize;
        let pos_index = (id & 31) as usize;
        self.0[array_index] &= !(1 << pos_index);
    }

    pub fn contains(&self, reg: BlockReg) -> bool {
        let id = reg.get_id();
        let array_index = (id >> 5) as usize;
        let pos_index = (id & 31) as usize;
        self.0[array_index] & (1 << pos_index) != 0
    }

    pub fn get_guests(&self) -> RegReserve {
        let guest_regs = (self.0[0] >> Reg::None as u8) & ((1 << Reg::None as u8) - 1);
        let spilled_over_count = Reg::None as u8 * 2 - 32;
        let guest_regs = guest_regs | ((self.0[1] & ((1 << spilled_over_count) - 1)) << (Reg::None as u8 - spilled_over_count));
        RegReserve::from(guest_regs)
    }

    pub fn get_fixed(&self) -> RegReserve {
        RegReserve::from(self.0[0] & ((1 << Reg::None as u8) - 1))
    }

    pub const fn len(&self) -> usize {
        let mut sum = 0;
        let mut i = 0;
        while i < self.0.len() {
            sum += self.0[i].count_ones();
            i += 1;
        }
        sum as usize
    }

    pub const fn len_any(&self) -> usize {
        let mut sum = 0;
        let mut i = 0;
        while i < self.0.len() {
            sum += self.0[i].count_ones();
            i += 1;
        }
        const FIXED_REGS_OVERFLOW: u8 = Reg::None as u8;
        (sum - (self.0[0] & ((1 << FIXED_REGS_OVERFLOW) - 1)).count_ones()) as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
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
            len: self.len(),
        }
    }
}

impl Add<BlockReg> for BlockRegSet {
    type Output = BlockRegSet;

    fn add(mut self, rhs: BlockReg) -> Self::Output {
        self._add(rhs);
        self
    }
}

impl AddAssign<BlockReg> for BlockRegSet {
    fn add_assign(&mut self, rhs: BlockReg) {
        self._add(rhs)
    }
}

impl Sub<BlockReg> for BlockRegSet {
    type Output = BlockRegSet;

    fn sub(mut self, rhs: BlockReg) -> Self::Output {
        self._sub(rhs);
        self
    }
}

impl SubAssign<BlockReg> for BlockRegSet {
    fn sub_assign(&mut self, rhs: BlockReg) {
        self._sub(rhs);
    }
}

impl Add<BlockRegSet> for BlockRegSet {
    type Output = BlockRegSet;

    fn add(mut self, rhs: BlockRegSet) -> Self::Output {
        for i in 0..self.0.len() {
            self.0[i] |= rhs.0[i];
        }
        self
    }
}

impl AddAssign<BlockRegSet> for BlockRegSet {
    fn add_assign(&mut self, rhs: BlockRegSet) {
        for i in 0..self.0.len() {
            self.0[i] |= rhs.0[i];
        }
    }
}

impl Sub<BlockRegSet> for BlockRegSet {
    type Output = BlockRegSet;

    fn sub(mut self, rhs: BlockRegSet) -> Self::Output {
        for i in 0..self.0.len() {
            self.0[i] &= !rhs.0[i];
        }
        self
    }
}

impl SubAssign<BlockRegSet> for BlockRegSet {
    fn sub_assign(&mut self, rhs: BlockRegSet) {
        for i in 0..self.0.len() {
            self.0[i] &= !rhs.0[i];
        }
    }
}

impl Sub<RegReserve> for BlockRegSet {
    type Output = BlockRegSet;

    fn sub(mut self, rhs: RegReserve) -> Self::Output {
        self.0[0] &= !rhs.0;
        self
    }
}

impl SubAssign<RegReserve> for BlockRegSet {
    fn sub_assign(&mut self, rhs: RegReserve) {
        self.0[0] &= !rhs.0;
    }
}

impl BitAnd<BlockRegSet> for BlockRegSet {
    type Output = BlockRegSet;

    fn bitand(mut self, rhs: BlockRegSet) -> Self::Output {
        for i in 0..self.0.len() {
            self.0[i] &= rhs.0[i];
        }
        self
    }
}

impl BitXor<BlockRegSet> for BlockRegSet {
    type Output = BlockRegSet;

    fn bitxor(mut self, rhs: BlockRegSet) -> Self::Output {
        for i in 0..self.0.len() {
            self.0[i] ^= rhs.0[i];
        }
        self
    }
}

impl BitXorAssign for BlockRegSet {
    fn bitxor_assign(&mut self, rhs: Self) {
        for i in 0..self.0.len() {
            self.0[i] ^= rhs.0[i];
        }
    }
}

impl Not for BlockRegSet {
    type Output = BlockRegSet;

    fn not(mut self) -> Self::Output {
        for i in 0..self.0.len() {
            self.0[i] = !self.0[i];
        }
        self
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

impl<'a> Iterator for BlockRegAnySetIter<'a> {
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

impl<'a> Iterator for BlockRegSetIter<'a> {
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
