use std::ops::{Add, AddAssign, BitAnd, BitXor, BitXorAssign, Not, Sub, SubAssign};

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Bitset<const SIZE: usize>(pub [u32; SIZE]);

impl<const SIZE: usize> Bitset<SIZE> {
    pub const fn new() -> Self {
        Bitset([0; SIZE])
    }

    fn _add(&mut self, bit: impl Into<usize>) {
        let bit = bit.into();
        let array_index = bit >> 5;
        let pos_index = bit & 31;
        unsafe { *self.0.get_unchecked_mut(array_index) |= 1 << pos_index };
    }

    fn _sub(&mut self, bit: impl Into<usize>) {
        let bit = bit.into();
        let array_index = bit >> 5;
        let pos_index = bit & 31;
        unsafe { *self.0.get_unchecked_mut(array_index) &= !(1 << pos_index) };
    }

    pub fn contains(&self, bit: impl Into<usize>) -> bool {
        let bit = bit.into();
        let array_index = bit >> 5;
        let pos_index = bit & 31;
        unsafe { *self.0.get_unchecked(array_index) & (1 << pos_index) != 0 }
    }

    pub const fn is_empty(&self) -> bool {
        let mut i = 0;
        let mut sum = 0;
        while i < self.0.len() {
            sum |= self.0[i];
            i += 1;
        }
        sum == 0
    }

    pub const fn clear(&mut self) {
        self.0 = [0; SIZE];
    }
}

impl<const SIZE: usize> Default for Bitset<SIZE> {
    fn default() -> Self {
        Bitset([0; SIZE])
    }
}

impl<const SIZE: usize, T: Into<usize>> Add<T> for Bitset<SIZE> {
    type Output = Bitset<SIZE>;

    fn add(mut self, rhs: T) -> Self::Output {
        self._add(rhs);
        self
    }
}

impl<const SIZE: usize, T: Into<usize>> AddAssign<T> for Bitset<SIZE> {
    fn add_assign(&mut self, rhs: T) {
        self._add(rhs)
    }
}

impl<const SIZE: usize, T: Into<usize>> Sub<T> for Bitset<SIZE> {
    type Output = Bitset<SIZE>;

    fn sub(mut self, rhs: T) -> Self::Output {
        self._sub(rhs);
        self
    }
}

impl<const SIZE: usize, T: Into<usize>> SubAssign<T> for Bitset<SIZE> {
    fn sub_assign(&mut self, rhs: T) {
        self._sub(rhs);
    }
}

impl<const SIZE: usize> Add<Bitset<SIZE>> for Bitset<SIZE> {
    type Output = Bitset<SIZE>;

    fn add(mut self, rhs: Bitset<SIZE>) -> Self::Output {
        for i in 0..self.0.len() {
            self.0[i] |= rhs.0[i];
        }
        self
    }
}

impl<const SIZE: usize> AddAssign<Bitset<SIZE>> for Bitset<SIZE> {
    fn add_assign(&mut self, rhs: Bitset<SIZE>) {
        for i in 0..self.0.len() {
            self.0[i] |= rhs.0[i];
        }
    }
}

impl<const SIZE: usize> Sub<Bitset<SIZE>> for Bitset<SIZE> {
    type Output = Bitset<SIZE>;

    fn sub(mut self, rhs: Bitset<SIZE>) -> Self::Output {
        for i in 0..self.0.len() {
            self.0[i] &= !rhs.0[i];
        }
        self
    }
}

impl<const SIZE: usize> SubAssign<Bitset<SIZE>> for Bitset<SIZE> {
    fn sub_assign(&mut self, rhs: Bitset<SIZE>) {
        for i in 0..self.0.len() {
            self.0[i] &= !rhs.0[i];
        }
    }
}

impl<const SIZE: usize> BitAnd<Bitset<SIZE>> for Bitset<SIZE> {
    type Output = Bitset<SIZE>;

    fn bitand(mut self, rhs: Bitset<SIZE>) -> Self::Output {
        for i in 0..self.0.len() {
            self.0[i] &= rhs.0[i];
        }
        self
    }
}

impl<const SIZE: usize> BitXor<Bitset<SIZE>> for Bitset<SIZE> {
    type Output = Bitset<SIZE>;

    fn bitxor(mut self, rhs: Bitset<SIZE>) -> Self::Output {
        for i in 0..self.0.len() {
            self.0[i] ^= rhs.0[i];
        }
        self
    }
}

impl<const SIZE: usize> BitXorAssign for Bitset<SIZE> {
    fn bitxor_assign(&mut self, rhs: Self) {
        for i in 0..self.0.len() {
            self.0[i] ^= rhs.0[i];
        }
    }
}

impl<const SIZE: usize> Not for Bitset<SIZE> {
    type Output = Bitset<SIZE>;

    fn not(mut self) -> Self::Output {
        for i in 0..self.0.len() {
            self.0[i] = !self.0[i];
        }
        self
    }
}
