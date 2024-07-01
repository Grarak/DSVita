use paste::paste;
use std::ops::{Index, IndexMut};
use std::{mem, ops};

#[derive(Copy, Clone)]
pub struct Matrix([i32; 16]);

impl ops::Mul for Matrix {
    type Output = Matrix;

    fn mul(self, rhs: Self) -> Self::Output {
        let mut ret = Matrix::default();
        for y in 0..4 {
            for x in 0..4 {
                ret.0[y * 4 + x] = ((self.0[y * 4] as i64 * rhs.0[x] as i64
                    + self.0[y * 4 + 1] as i64 * rhs.0[4 + x] as i64
                    + self.0[y * 4 + 2] as i64 * rhs.0[8 + x] as i64
                    + self.0[y * 4 + 3] as i64 * rhs.0[12 + x] as i64)
                    >> 12) as i32;
            }
        }
        ret
    }
}

impl AsRef<[i32; 16]> for Matrix {
    fn as_ref(&self) -> &[i32; 16] {
        &self.0
    }
}

impl AsMut<[i32; 16]> for Matrix {
    fn as_mut(&mut self) -> &mut [i32; 16] {
        &mut self.0
    }
}

impl Index<usize> for Matrix {
    type Output = i32;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for Matrix {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl Default for Matrix {
    fn default() -> Self {
        #[rustfmt::skip]
        Matrix([
            1 << 12, 0 << 12, 0 << 12, 0 << 12,
            0 << 12, 1 << 12, 0 << 12, 0 << 12,
            0 << 12, 0 << 12, 1 << 12, 0 << 12,
            0 << 12, 0 << 12, 0 << 12, 1 << 12,
        ])
    }
}

macro_rules! define_vector {
    ($t:ident) => {
        paste! {
            #[derive(Copy, Clone)]
            pub struct [<Vector $t>]<const SIZE: usize>([$t; SIZE]);

            impl<const SIZE: usize> Default for [<Vector $t>]<SIZE> {
                fn default() -> Self {
                    unsafe { mem::zeroed() }
                }
            }

            impl<const SIZE: usize> AsRef<[$t; SIZE]> for [<Vector $t>]<SIZE> {
                fn as_ref(&self) -> &[$t; SIZE] {
                    &self.0
                }
            }

            impl<const SIZE: usize> AsMut<[$t; SIZE]> for [<Vector $t>]<SIZE> {
                fn as_mut(&mut self) -> &mut [$t; SIZE] {
                    &mut self.0
                }
            }

            impl<const SIZE: usize> Index<usize> for [<Vector $t>]<SIZE> {
                type Output = $t;

                fn index(&self, index: usize) -> &Self::Output {
                    &self.0[index]
                }
            }

            impl<const SIZE: usize> IndexMut<usize> for [<Vector $t>]<SIZE> {
                fn index_mut(&mut self, index: usize) -> &mut Self::Output {
                    &mut self.0[index]
                }
            }

            impl<const SIZE: usize> ops::Mul<$t> for [<Vector $t>]<SIZE> {
                type Output = Self;

                fn mul(mut self, rhs: $t) -> Self::Output {
                    for i in 0..SIZE {
                        self.0[i] *= rhs
                    }
                    self
                }
            }
            
            impl From<[<Vector $t>]<3>> for [<Vector $t>]<4> {
                fn from(value: [<Vector $t>]<3>) -> Self {
                    let mut ret = Self::default();
                    ret[0] = value[0];
                    ret[1] = value[1];
                    ret[2] = value[2];
                    ret
                }
            }
        }
    };
}

define_vector!(u16);
define_vector!(i16);
define_vector!(i32);

impl ops::Mul<Matrix> for Vectori32<3> {
    type Output = Self;

    fn mul(self, rhs: Matrix) -> Self::Output {
        let mut ret = Vectori32::default();
        ret.0[0] = ((self.0[0] as i64 * rhs.0[0] as i64 + self.0[1] as i64 * rhs.0[4] as i64 + self.0[2] as i64 * rhs.0[8] as i64) >> 12) as i32;
        ret.0[1] = ((self.0[0] as i64 * rhs.0[1] as i64 + self.0[1] as i64 * rhs.0[5] as i64 + self.0[2] as i64 * rhs.0[9] as i64) >> 12) as i32;
        ret.0[2] = ((self.0[0] as i64 * rhs.0[2] as i64 + self.0[1] as i64 * rhs.0[6] as i64 + self.0[2] as i64 * rhs.0[10] as i64) >> 12) as i32;
        ret
    }
}

impl ops::Mul<Matrix> for Vectori32<4> {
    type Output = Self;

    fn mul(self, rhs: Matrix) -> Self::Output {
        let mut ret = Vectori32::default();
        ret[0] = ((self[0] as i64 * rhs[0] as i64 + self[1] as i64 * rhs[4] as i64 + self[2] as i64 * rhs[8] as i64 + self[3] as i64 * rhs[12] as i64) >> 12) as i32;
        ret[1] = ((self[0] as i64 * rhs[1] as i64 + self[1] as i64 * rhs[5] as i64 + self[2] as i64 * rhs[9] as i64 + self[3] as i64 * rhs[13] as i64) >> 12) as i32;
        ret[2] = ((self[0] as i64 * rhs[2] as i64 + self[1] as i64 * rhs[6] as i64 + self[2] as i64 * rhs[10] as i64 + self[3] as i64 * rhs[14] as i64) >> 12) as i32;
        ret[3] = ((self[0] as i64 * rhs[3] as i64 + self[1] as i64 * rhs[7] as i64 + self[2] as i64 * rhs[11] as i64 + self[3] as i64 * rhs[15] as i64) >> 12) as i32;
        ret
    }
}

impl ops::MulAssign<Matrix> for Vectori32<3> {
    fn mul_assign(&mut self, rhs: Matrix) {
        *self = *self * rhs;
    }
}

impl ops::MulAssign<Matrix> for Vectori32<4> {
    fn mul_assign(&mut self, rhs: Matrix) {
        *self = *self * rhs;
    }
}

impl<const SIZE: usize> ops::Mul for Vectori32<SIZE> {
    type Output = i32;

    fn mul(self, rhs: Self) -> Self::Output {
        let mut dot = 0;
        for i in 0..SIZE {
            dot += self[i] as i64 * rhs[i] as i64;
        }
        (dot >> 12) as i32
    }
}
