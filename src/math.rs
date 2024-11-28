use paste::paste;
use std::arch::asm;
use std::ops;
use std::ops::{Index, IndexMut};

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
                    [<Vector $t>]([$t::default(); SIZE])
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
define_vector!(f32);

impl ops::Mul<Matrix> for Vectori32<3> {
    type Output = Self;

    fn mul(self, rhs: Matrix) -> Self::Output {
        let mut v0: i32;
        let mut v1: i32;
        let mut v2: i32;
        unsafe {
            asm!(
            "vmov.s32 d1, 0",
            "vld1.s32 {{d0}}, [{v}]!",
            "vld1.s32 {{d1[0]}}, [{v}]",
            "vld1.s32 {{q1}}, [{m}]!",
            "vld1.s32 {{q2}}, [{m}]!",
            "vld1.s32 {{q3}}, [{m}]!",
            "vld1.s32 {{q4}}, [{m}]",
            "vmull.s32 q5, d2, d0[0]",
            "vmull.s32 q6, d3, d0[0]",
            "vmlal.s32 q5, d4, d0[1]",
            "vmlal.s32 q6, d5, d0[1]",
            "vmlal.s32 q5, d6, d1[0]",
            "vmlal.s32 q6, d7, d1[0]",
            "vmlal.s32 q5, d8, d1[1]",
            "vmlal.s32 q6, d9, d1[1]",
            "vshr.s64 q5, q5, 12",
            "vshr.s64 q6, q6, 12",
            "vmov.s32 {v0}, s20",
            "vmov.s32 {v1}, s22",
            "vmov.s32 {v2}, s24",
            v = in(reg) self.0.as_ptr(),
            m = in(reg) rhs.0.as_ptr(),
            v0 = out(reg) v0,
            v1 = out(reg) v1,
            v2 = out(reg) v2,
            options(pure, readonly, preserves_flags, nostack),
            );
        }
        Vectori32([v0, v1, v2])
    }
}

impl ops::Mul<Matrix> for Vectori32<4> {
    type Output = Self;

    fn mul(self, rhs: Matrix) -> Self::Output {
        let mut v0: i32;
        let mut v1: i32;
        let mut v2: i32;
        let mut v3: i32;
        unsafe {
            asm!(
            "vld1.s32 {{q0}}, [{v}]",
            "vld1.s32 {{q1}}, [{m}]!",
            "vld1.s32 {{q2}}, [{m}]!",
            "vld1.s32 {{q3}}, [{m}]!",
            "vld1.s32 {{q4}}, [{m}]",
            "vmull.s32 q5, d2, d0[0]",
            "vmull.s32 q6, d3, d0[0]",
            "vmlal.s32 q5, d4, d0[1]",
            "vmlal.s32 q6, d5, d0[1]",
            "vmlal.s32 q5, d6, d1[0]",
            "vmlal.s32 q6, d7, d1[0]",
            "vmlal.s32 q5, d8, d1[1]",
            "vmlal.s32 q6, d9, d1[1]",
            "vshr.s64 q5, q5, 12",
            "vshr.s64 q6, q6, 12",
            "vmov.s32 {v0}, s20",
            "vmov.s32 {v1}, s22",
            "vmov.s32 {v2}, s24",
            "vmov.s32 {v3}, s26",
            v = in(reg) self.0.as_ptr(),
            m = in(reg) rhs.0.as_ptr(),
            v0 = out(reg) v0,
            v1 = out(reg) v1,
            v2 = out(reg) v2,
            v3 = out(reg) v3,
            options(pure, readonly, preserves_flags, nostack),
            );
        }
        Vectori32([v0, v1, v2, v3])
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

impl ops::Mul for Vectori32<3> {
    type Output = i32;

    fn mul(self, rhs: Self) -> Self::Output {
        /* Vectorization of
        let mut dot = 0;
        dot += self[0] as i64 * rhs[0] as i64;
        dot += self[1] as i64 * rhs[1] as i64;
        dot += self[2] as i64 * rhs[2] as i64;
        (dot >> 12) as i32
         */

        let v1 = self.0.as_ptr();
        let v2 = rhs.0.as_ptr();
        let mut dot: i32;
        unsafe {
            asm!(
            "vmov.s32 d1, 0",
            "vmov.s32 d3, 0",
            "vld1.s32 {{d0}}, [{v1}]!",
            "vld1.s32 {{d1[0]}}, [{v1}]",
            "vld1.s32 {{d2}}, [{v2}]!",
            "vld1.s32 {{d3[0]}}, [{v2}]",
            "vmull.s32 q2, d0, d2",
            "vmlal.s32 q2, d1, d3",
            "vadd.s64 d4, d4, d5",
            "vshr.s64 d4, d4, 12",
            "vmov.s32 {dot}, d4[0]",
            v1 = in(reg) v1,
            v2 = in(reg) v2,
            dot = out(reg) dot,
            options(pure, readonly, preserves_flags, nostack),
            );
        }
        dot
    }
}
