use paste::paste;
use std::arch::arm::{
    int32x4_t, int64x1_t, int64x2_t, uint64x2_t, vaddq_u64, vget_high_s32, vget_high_s64, vget_lane_s32, vget_low_s32, vget_low_s64, vgetq_lane_s32, vld1q_s32, vld1q_s32_x4, vmlal_n_s32, vmlal_s32,
    vmovn_u64, vmull_n_s32, vmull_s32, vmull_u32, vreinterpretq_s64_u64, vreinterpretq_u64_s64, vshlq_n_u64, vshr_n_s64, vshrq_n_s64, vshrq_n_u64, vst1q_s32, vuzpq_s32,
};
use std::fmt::{Display, Formatter};
use std::intrinsics::simd::simd_add;
use std::ops::{Index, IndexMut};
use std::{mem, ops};

pub mod neon {
    #![allow(warnings, unused)]
    include!(concat!(env!("OUT_DIR"), "/math_neon.rs"));
}

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

#[rustfmt::skip]
pub const MTX_IDENTITY: [i32; 16] = [
    1 << 12, 0 << 12, 0 << 12, 0 << 12,
    0 << 12, 1 << 12, 0 << 12, 0 << 12,
    0 << 12, 0 << 12, 1 << 12, 0 << 12,
    0 << 12, 0 << 12, 0 << 12, 1 << 12,
];

#[derive(Clone)]
pub struct Matrix(pub [i32; 16]);

impl Matrix {
    pub unsafe fn vld_identity() -> [int32x4_t; 4] {
        let mtx = vld1q_s32_x4(MTX_IDENTITY.as_ptr());
        [mtx.0, mtx.1, mtx.2, mtx.3]
    }

    pub unsafe fn vld(&self) -> [int32x4_t; 4] {
        let mtx = vld1q_s32_x4(self.0.as_ptr());
        [mtx.0, mtx.1, mtx.2, mtx.3]
    }
}

impl ops::Mul<&Matrix> for Matrix {
    type Output = Matrix;

    fn mul(mut self, rhs: &Matrix) -> Self::Output {
        self *= rhs;
        self
    }
}

impl ops::MulAssign<&Matrix> for Matrix {
    fn mul_assign(&mut self, rhs: &Matrix) {
        unsafe { vmult_mat4(self.vld(), rhs.vld(), &mut self.0) };
    }
}

#[inline(always)]
pub unsafe fn vmult_mat4(lm: [int32x4_t; 4], rm: [int32x4_t; 4], result: &mut [i32; 16]) {
    for i in 0..4 {
        vst1q_s32(result.as_mut_ptr().add(i << 2), vmult_vec4_mat4_no_store(lm[i], rm));
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
        Matrix(MTX_IDENTITY)
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
    pub values: [i32; SIZE],
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

    pub unsafe fn vld(&self) -> int32x4_t {
        vld1q_s32(self.values.as_ptr())
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

        let divisor = (1 << fraction_bits) as f32;
        write!(f, "{}", self.values[0] as f32 / divisor)?;

        for i in 1..SIZE {
            write!(f, ", {}", self.values[i] as f32 / divisor)?;
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
        let v: &mut Vectori32<4> = unsafe { mem::transmute(self) };
        *v *= rhs
    }
}

impl ops::MulAssign<&Matrix> for Vectori32<4> {
    fn mul_assign(&mut self, rhs: &Matrix) {
        unsafe { vmult_vec4_mat4(vld1q_s32(self.values.as_ptr()), rhs.vld(), &mut self.values) };
    }
}

#[inline(always)]
pub unsafe fn vmult_vec4_mat4_no_store(v: int32x4_t, m: [int32x4_t; 4]) -> int32x4_t {
    let lower_result = vmull_n_s32(vget_low_s32(m[0]), vgetq_lane_s32::<0>(v));
    let lower_result = vmlal_n_s32(lower_result, vget_low_s32(m[1]), vgetq_lane_s32::<1>(v));
    let lower_result = vmlal_n_s32(lower_result, vget_low_s32(m[2]), vgetq_lane_s32::<2>(v));
    let lower_result = vmlal_n_s32(lower_result, vget_low_s32(m[3]), vgetq_lane_s32::<3>(v));

    let higher_result = vmull_n_s32(vget_high_s32(m[0]), vgetq_lane_s32::<0>(v));
    let higher_result = vmlal_n_s32(higher_result, vget_high_s32(m[1]), vgetq_lane_s32::<1>(v));
    let higher_result = vmlal_n_s32(higher_result, vget_high_s32(m[2]), vgetq_lane_s32::<2>(v));
    let higher_result = vmlal_n_s32(higher_result, vget_high_s32(m[3]), vgetq_lane_s32::<3>(v));

    let lower_result = vshrq_n_s64::<12>(lower_result);
    let higher_result = vshrq_n_s64::<12>(higher_result);

    let v = vuzpq_s32(mem::transmute(lower_result), mem::transmute(higher_result));
    v.0
}

#[inline(always)]
pub unsafe fn vmult_vec3_mat3_no_store(v: int32x4_t, m: [int32x4_t; 4]) -> int32x4_t {
    let lower_result = vmull_n_s32(vget_low_s32(m[0]), vgetq_lane_s32::<0>(v));
    let lower_result = vmlal_n_s32(lower_result, vget_low_s32(m[1]), vgetq_lane_s32::<1>(v));
    let lower_result = vmlal_n_s32(lower_result, vget_low_s32(m[2]), vgetq_lane_s32::<2>(v));

    let higher_result = vmull_n_s32(vget_high_s32(m[0]), vgetq_lane_s32::<0>(v));
    let higher_result = vmlal_n_s32(higher_result, vget_high_s32(m[1]), vgetq_lane_s32::<1>(v));
    let higher_result = vmlal_n_s32(higher_result, vget_high_s32(m[2]), vgetq_lane_s32::<2>(v));

    let lower_result = vshrq_n_s64::<12>(lower_result);
    let higher_result = vshrq_n_s64::<12>(higher_result);

    let v = vuzpq_s32(mem::transmute(lower_result), mem::transmute(higher_result));
    v.0
}

pub unsafe fn vmult_vec4_mat4(v: int32x4_t, m: [int32x4_t; 4], dst: &mut [i32; 4]) {
    vst1q_s32(dst.as_mut_ptr(), vmult_vec4_mat4_no_store(v, m));
}

impl ops::Mul<&Vectori32<3>> for Vectori32<3> {
    type Output = i32;

    fn mul(self, rhs: &Vectori32<3>) -> Self::Output {
        let mut dot = 0;
        for i in 0..3 {
            dot += self[i] as i64 * rhs[i] as i64;
        }
        (dot >> 12) as i32
    }
}

pub unsafe fn vadd_s64(a: int64x1_t, b: int64x1_t) -> int64x1_t {
    simd_add(a, b)
}

#[inline(always)]
pub unsafe fn vdot_vec3(v1: int32x4_t, v2: int32x4_t) -> i32 {
    let ret = vmull_s32(vget_low_s32(v1), vget_low_s32(v2));
    let ret = vmlal_s32(ret, vget_high_s32(v1), vget_high_s32(v2));
    let ret = vadd_s64(vget_low_s64(ret), vget_high_s64(ret));
    let ret = vshr_n_s64::<12>(ret);
    vget_lane_s32::<0>(mem::transmute(ret))
}
