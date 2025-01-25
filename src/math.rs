use paste::paste;
use std::arch::arm::{
    int32x4_t, int64x1_t, int64x2_t, uint64x2_t, vaddq_u64, vget_high_s32, vget_high_s64, vget_lane_s32, vget_low_s32, vget_low_s64, vmlal_s32, vmovn_u64, vmull_s32, vmull_u32, vreinterpretq_s64_u64,
    vreinterpretq_u64_s64, vshlq_n_u64, vshr_n_s64, vshrq_n_u64,
};
use std::arch::asm;
use std::fmt::{Display, Formatter};
use std::intrinsics::simd::simd_add;
use std::ops::{Index, IndexMut};
use std::{mem, ops};

// Taken from https://github.com/awxkee/erydanos/blob/master/src/neon/general.rs
#[inline]
/// Multiplies u64 together and takes low part, do not care about overflowing
pub unsafe fn vmulq_u64(ab: uint64x2_t, cd: uint64x2_t) -> uint64x2_t {
    /* ac = (ab & 0xFFFFFFFF) * (cd & 0xFFFFFFFF); */
    let ab_low = vmovn_u64(ab);
    let cd_low = vmovn_u64(cd);
    let ac = vmull_u32(ab_low, cd_low);

    /* b = ab >> 32; */
    let b = vshrq_n_u64::<32>(ab);

    /* bc = b * (cd & 0xFFFFFFFF); */
    let bc = vmull_u32(vmovn_u64(b), vmovn_u64(cd));

    /* d = cd >> 32; */
    let d = vshrq_n_u64::<32>(cd);

    /* ad = (ab & 0xFFFFFFFF) * d; */
    let ad = vmull_u32(vmovn_u64(ab), vmovn_u64(d));

    /* high = bc + ad; */
    let mut high = vaddq_u64(bc, ad);

    /* high <<= 32; */
    high = vshlq_n_u64::<32>(high);

    /* return ac + high; */
    vaddq_u64(high, ac)
}

#[inline]
pub unsafe fn vmulq_s64(ab: int64x2_t, cd: int64x2_t) -> int64x2_t {
    vreinterpretq_s64_u64(vmulq_u64(vreinterpretq_u64_s64(ab), vreinterpretq_u64_s64(cd)))
}

#[derive(Copy, Clone)]
pub struct Matrix([i32; 16]);

impl ops::Mul<&Matrix> for Matrix {
    type Output = Matrix;

    fn mul(self, rhs: &Matrix) -> Self::Output {
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

impl Display for Matrix {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let row0 = unsafe { (self.0[..4].as_ptr() as *const Vectori32<4>).as_ref_unchecked() };
        let row1 = unsafe { (self.0[4..8].as_ptr() as *const Vectori32<4>).as_ref_unchecked() };
        let row2 = unsafe { (self.0[8..12].as_ptr() as *const Vectori32<4>).as_ref_unchecked() };
        let row3 = unsafe { (self.0[12..].as_ptr() as *const Vectori32<4>).as_ref_unchecked() };
        write!(f, "[")?;
        row0.fmt(f)?;
        write!(f, ", ")?;
        row1.fmt(f)?;
        write!(f, ", ")?;
        row2.fmt(f)?;
        write!(f, ", ")?;
        row3.fmt(f)?;
        write!(f, "]")
    }
}

macro_rules! define_vector {
    ($t:ident) => {
        paste! {
            #[derive(Copy, Clone)]
            pub struct [<Vector $t>]<const SIZE: usize>(pub [$t; SIZE]);

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

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Vectori32<const SIZE: usize>
where
    [(); 4 - SIZE]:,
{
    values: [i32; SIZE],
    padding: [i32; 4 - SIZE],
}

impl<const SIZE: usize> Vectori32<SIZE>
where
    [(); 4 - SIZE]:,
{
    pub fn new(values: [i32; SIZE]) -> Self {
        Vectori32 {
            values,
            padding: unsafe { mem::zeroed() },
        }
    }
}

impl<const SIZE: usize> Default for Vectori32<SIZE>
where
    [(); 4 - SIZE]:,
{
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

impl<const SIZE: usize> AsRef<[i32; SIZE]> for Vectori32<SIZE>
where
    [(); 4 - SIZE]:,
{
    fn as_ref(&self) -> &[i32; SIZE] {
        &self.values
    }
}

impl<const SIZE: usize> AsMut<[i32; SIZE]> for Vectori32<SIZE>
where
    [(); 4 - SIZE]:,
{
    fn as_mut(&mut self) -> &mut [i32; SIZE] {
        &mut self.values
    }
}

impl<const SIZE: usize> Index<usize> for Vectori32<SIZE>
where
    [(); 4 - SIZE]:,
{
    type Output = i32;
    fn index(&self, index: usize) -> &Self::Output {
        &self.values[index]
    }
}

impl<const SIZE: usize> IndexMut<usize> for Vectori32<SIZE>
where
    [(); 4 - SIZE]:,
{
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.values[index]
    }
}

impl<const SIZE: usize> Display for Vectori32<SIZE>
where
    [(); 4 - SIZE]:,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let fraction_bits = f.precision().unwrap_or(12);
        write!(f, "[")?;

        let decimal = self.values[0] >> fraction_bits;
        let fraction = self.values[0] & ((1 << fraction_bits) - 1);
        write!(f, "{decimal}.{fraction}")?;

        for i in 1..SIZE {
            let decimal = self.values[i] >> fraction_bits;
            let fraction = self.values[i] & ((1 << fraction_bits) - 1);
            write!(f, ", {decimal}.{fraction}")?;
        }
        write!(f, "]")
    }
}

impl From<Vectori32<3>> for Vectori32<4> {
    fn from(value: Vectori32<3>) -> Self {
        unsafe { mem::transmute(value) }
    }
}

define_vector!(u8);
define_vector!(u16);
define_vector!(i16);
define_vector!(f32);

impl ops::Mul<&Matrix> for Vectori32<3> {
    type Output = Self;

    fn mul(mut self, rhs: &Matrix) -> Self::Output {
        self *= rhs;
        self
    }
}

impl ops::Mul<&Matrix> for Vectori32<4> {
    type Output = Self;

    fn mul(mut self, rhs: &Matrix) -> Self::Output {
        self *= rhs;
        self
    }
}

impl ops::MulAssign<&Matrix> for Vectori32<3> {
    fn mul_assign(&mut self, rhs: &Matrix) {
        unsafe {
            asm!(
            "vld1.s32 {{q0}}, [{v}]",
            "vld1.s32 {{q1}}, [{m}]!",
            "vld1.s32 {{q2}}, [{m}]!",
            "vld1.s32 {{q3}}, [{m}]",
            "vmull.s32 q5, d2, d0[0]",
            "vmull.s32 q6, d3, d0[0]",
            "vmlal.s32 q5, d4, d0[1]",
            "vmlal.s32 q6, d5, d0[1]",
            "vmlal.s32 q5, d6, d1[0]",
            "vmlal.s32 q6, d7, d1[0]",
            "vshr.s64 q5, q5, 12",
            "vshr.s64 q6, q6, 12",
            "vuzp.32 q5, q6",
            "vst1.s32 {{q5}}, [{v}]",
            v = in(reg) self.values.as_mut_ptr(),
            m = in(reg) rhs.0.as_ptr(),
            options(preserves_flags, nostack),
            );
        }
    }
}

impl ops::MulAssign<&Matrix> for Vectori32<4> {
    fn mul_assign(&mut self, rhs: &Matrix) {
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
            "vuzp.32 q5, q6",
            "vst1.s32 {{q5}}, [{v}]",
            v = in(reg) self.values.as_mut_ptr(),
            m = in(reg) rhs.0.as_ptr(),
            options(preserves_flags, nostack),
            );
        }
    }
}

impl ops::Mul<&Vectori32<3>> for Vectori32<3> {
    type Output = i32;

    fn mul(mut self, rhs: &Vectori32<3>) -> Self::Output {
        /* Vectorization of
        let mut dot = 0;
        dot += self[0] as i64 * rhs[0] as i64;
        dot += self[1] as i64 * rhs[1] as i64;
        dot += self[2] as i64 * rhs[2] as i64;
        (dot >> 12) as i32
         */

        self.padding[0] = 0;
        let mut dot: i32;
        unsafe {
            asm!(
            "vld1.s32 {{q0}}, [{v1}]",
            "vld1.s32 {{q1}}, [{v2}]",
            "vmull.s32 q2, d0, d2",
            "vmlal.s32 q2, d1, d3",
            "vadd.s64 d4, d4, d5",
            "vshr.s64 d4, d4, 12",
            "vmov.s32 {dot}, d4[0]",
            v1 = in(reg) self.values.as_ptr(),
            v2 = in(reg) rhs.values.as_ptr(),
            dot = out(reg) dot,
            options(pure, readonly, preserves_flags, nostack),
            );
        }
        dot
    }
}

pub unsafe fn vadd_u64(a: int64x1_t, b: int64x1_t) -> int64x1_t {
    simd_add(a, b)
}

pub unsafe fn vdot_vec3(v1: int32x4_t, v2: int32x4_t) -> i32 {
    let ret = vmull_s32(vget_low_s32(v1), vget_low_s32(v2));
    let ret = vmlal_s32(ret, vget_high_s32(v1), vget_high_s32(v2));
    let ret = vadd_u64(vget_low_s64(ret), vget_high_s64(ret));
    let ret = vshr_n_s64::<12>(ret);
    vget_lane_s32::<0>(mem::transmute(ret))
}
