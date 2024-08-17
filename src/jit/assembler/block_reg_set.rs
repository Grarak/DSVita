use crate::jit::assembler::{BlockReg, ANY_REG_LIMIT};
use crate::jit::reg::{Reg, RegReserve};
use std::fmt::{Debug, Formatter};
use std::ops::{Add, AddAssign, Not, Sub, SubAssign};

#[derive(Copy, Clone)]
pub struct BlockRegSet(u64);

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
        BlockRegSet(0)
    }

    pub const fn new_all() -> Self {
        BlockRegSet(!0)
    }

    fn _add(&mut self, reg: BlockReg) {
        match reg {
            BlockReg::Any(id) => self.0 |= 1 << (id + Reg::SPSR as u8 * 2),
            BlockReg::Guest(reg) => self.0 |= 1 << reg as u8,
            BlockReg::Fixed(reg) => self.0 |= 1 << (reg as u8 + Reg::SPSR as u8),
        }
    }

    pub fn contains(&self, reg: BlockReg) -> bool {
        match reg {
            BlockReg::Any(id) => self.0 & (1 << (id + Reg::SPSR as u8 * 2)) != 0,
            BlockReg::Guest(reg) => self.0 & (1 << reg as u8) != 0,
            BlockReg::Fixed(reg) => self.0 & (1 << (reg as u8 + Reg::SPSR as u8)) != 0,
        }
    }

    pub fn get_guests(&self) -> RegReserve {
        RegReserve::from((self.0 as u32) & ((1 << Reg::SPSR as u8) - 1))
    }

    pub fn get_fixed(&self) -> RegReserve {
        RegReserve::from(((self.0 >> Reg::SPSR as u8) as u32) & ((1 << Reg::SPSR as u8) - 1))
    }

    pub const fn len(&self) -> usize {
        self.0.count_zeros() as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter_fixed(&self) -> BlockRegFixedSetIter {
        BlockRegFixedSetIter { block_reg_set: self, current: 0 }
    }

    pub fn iter_any(&self) -> BlockRegAnySetIter {
        BlockRegAnySetIter { block_reg_set: self, current: 0 }
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

impl Add<BlockRegSet> for BlockRegSet {
    type Output = BlockRegSet;

    fn add(mut self, rhs: BlockRegSet) -> Self::Output {
        self.0 |= rhs.0;
        self
    }
}

impl AddAssign<BlockRegSet> for BlockRegSet {
    fn add_assign(&mut self, rhs: BlockRegSet) {
        self.0 |= rhs.0;
    }
}

impl Sub<BlockRegSet> for BlockRegSet {
    type Output = BlockRegSet;

    fn sub(mut self, rhs: BlockRegSet) -> Self::Output {
        self.0 &= !rhs.0;
        self
    }
}

impl SubAssign<BlockRegSet> for BlockRegSet {
    fn sub_assign(&mut self, rhs: BlockRegSet) {
        self.0 &= !rhs.0;
    }
}

impl Not for BlockRegSet {
    type Output = BlockRegSet;

    fn not(self) -> Self::Output {
        BlockRegSet(!self.0)
    }
}

impl From<RegReserve> for BlockRegSet {
    fn from(value: RegReserve) -> Self {
        BlockRegSet(value.0 as u64)
    }
}

impl Debug for BlockRegSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut str = "".to_owned();
        for i in Reg::R0 as u8..Reg::SPSR as u8 {
            let reg = BlockReg::Guest(Reg::from(i));
            if self.contains(reg) {
                str += &format!("{:?}, ", reg);
            }
        }
        for i in Reg::R0 as u8..Reg::SPSR as u8 {
            let reg = BlockReg::Fixed(Reg::from(i));
            if self.contains(reg) {
                str += &format!("{:?}, ", reg);
            }
        }
        for i in 0..ANY_REG_LIMIT {
            let reg = BlockReg::Any(i);
            if self.contains(reg) {
                str += &format!("{:?}, ", reg);
            }
        }
        let mut chars = str.chars();
        chars.next_back();
        chars.next_back();
        f.write_str(chars.as_str())
    }
}

pub struct BlockRegFixedSetIter<'a> {
    block_reg_set: &'a BlockRegSet,
    current: u8,
}

impl<'a> Iterator for BlockRegFixedSetIter<'a> {
    type Item = Reg;

    fn next(&mut self) -> Option<Self::Item> {
        for i in self.current..Reg::SPSR as u8 {
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
    current: u8,
}

impl<'a> Iterator for BlockRegAnySetIter<'a> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        for i in self.current..ANY_REG_LIMIT {
            if self.block_reg_set.contains(BlockReg::Any(i)) {
                self.current = i + 1;
                return Some(i);
            }
        }
        None
    }
}
