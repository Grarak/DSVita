use crate::core::emu::Emu;
use bilge::prelude::*;
use std::hint::unreachable_unchecked;
use std::intrinsics::{unchecked_div, unchecked_rem};

#[bitsize(16)]
#[derive(Clone, Copy, FromBits)]
pub struct DivCnt {
    mode: u2,
    not_used: u12,
    zero_error: bool,
    busy: bool,
}

impl Default for DivCnt {
    fn default() -> Self {
        DivCnt::from(0)
    }
}

pub struct DivSqrt {
    sqrt_dirty: bool,
    div_dirty: bool,
}

impl Emu {
    pub fn div_get_result_l(&mut self) {
        self.div();
    }

    pub fn div_get_result_h(&mut self) {
        self.div();
    }

    pub fn div_get_rem_result_l(&mut self) {
        self.div();
    }

    pub fn div_get_rem_result_h(&mut self) {
        self.div();
    }

    pub fn sqrt_get_result(&mut self) {
        self.sqrt();
    }

    pub fn div_set_cnt(&mut self) {
        self.mem.io.arm9().div_cnt.value &= 0x3;
        self.div_sqrt.div_dirty = true;
    }

    pub fn sqrt_set_cnt(&mut self) {
        self.mem.io.arm9().sqrt_cnt &= 0x1;
        self.div_sqrt.sqrt_dirty = true;
    }

    fn div(&mut self) {
        if !self.div_sqrt.div_dirty {
            return;
        }
        self.div_sqrt.div_dirty = false;
        let div_denom = ((self.mem.io.arm9().div_denom_h as u64) << 32) | (self.mem.io.arm9().div_denom_l as u64);
        let div_denom = div_denom as i64;
        self.mem.io.arm9().div_cnt.set_zero_error(div_denom == 0);

        let div_numer = ((self.mem.io.arm9().div_numer_h as u64) << 32) | (self.mem.io.arm9().div_numer_l as u64);
        let div_numer = div_numer as i64;

        let (div_result, divrem_result) = match u8::from(self.mem.io.arm9().div_cnt.mode()) {
            0 => {
                let num = div_numer as i32;
                let denom = div_denom as i32;
                if num == i32::MIN && denom == -1 {
                    ((num as u64 ^ ((!0u32 as u64) << 32)) as i64, 0)
                } else if denom != 0 {
                    (unsafe { unchecked_div(num, denom) } as i64, unsafe { unchecked_rem(num, denom) } as i64)
                } else {
                    ((if num < 0 { 1 } else { -1i32 } as u64 ^ ((!0u32 as u64) << 32)) as i64, num as i64)
                }
            }
            1 => {
                let num = div_numer;
                let denom = div_denom as i32;
                if num == i64::MIN && denom == -1 {
                    (num, 0)
                } else if denom != 0 {
                    (unsafe { unchecked_div(num, denom as i64) }, unsafe { unchecked_rem(num, denom as i64) })
                } else {
                    (if num < 0 { 1 } else { -1 }, num)
                }
            }
            2 => {
                let num = div_numer;
                let denom = div_denom;
                if num == i64::MIN && denom == -1 {
                    (num, 0)
                } else if denom != 0 {
                    (unsafe { unchecked_div(num, denom) }, unsafe { unchecked_rem(num, denom) })
                } else {
                    (if num < 0 { 1 } else { -1 }, num)
                }
            }
            _ => unsafe { unreachable_unchecked() },
        };

        self.mem.io.arm9().div_result_h = ((div_result as u64) >> 32) as u32;
        self.mem.io.arm9().div_result_l = (div_result as u64) as u32;
        self.mem.io.arm9().divrem_result_h = ((divrem_result as u64) >> 32) as u32;
        self.mem.io.arm9().divrem_result_l = (divrem_result as u64) as u32;
    }

    fn sqrt(&mut self) {
        if !self.div_sqrt.sqrt_dirty {
            return;
        }
        self.div_sqrt.sqrt_dirty = false;
        let cnt = self.mem.io.arm9().sqrt_cnt;
        self.mem.io.arm9().sqrt_result = if cnt & 1 == 0 {
            self.mem.io.arm9().sqrt_param_l.isqrt()
        } else {
            let param = ((self.mem.io.arm9().sqrt_param_h as u64) << 32) | (self.mem.io.arm9().sqrt_param_l as u64);
            param.isqrt() as u32
        };
    }
}

impl DivSqrt {
    pub fn new() -> Self {
        DivSqrt { sqrt_dirty: true, div_dirty: true }
    }

    pub fn set_sqrt_param_l(&mut self) {
        self.sqrt_dirty = true;
    }

    pub fn set_sqrt_param_h(&mut self) {
        self.sqrt_dirty = true;
    }

    pub fn set_div_numer_l(&mut self) {
        self.div_dirty = true;
    }

    pub fn set_div_numer_h(&mut self) {
        self.div_dirty = true;
    }

    pub fn set_div_denom_l(&mut self) {
        self.div_dirty = true;
    }

    pub fn set_div_denom_h(&mut self) {
        self.div_dirty = true;
    }
}
