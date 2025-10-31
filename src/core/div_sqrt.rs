use std::hint::unreachable_unchecked;
use std::intrinsics::{unchecked_div, unchecked_rem};

#[derive(Default, Debug)]
#[repr(C)]
pub struct CpContext {
    div_numer: i64,
    div_denom: i64,
    sqrt_param: u64,
    pub div_cnt: u16,
    pub sqrt_cnt: u16,
}

pub struct DivSqrt {
    pub context: CpContext,
    sqrt_result: u32,
    sqrt_dirty: bool,
    div_result: i64,
    divrem_result: i64,
    div_dirty: bool,
}

impl DivSqrt {
    pub fn new() -> Self {
        DivSqrt {
            context: CpContext::default(),
            sqrt_result: 0,
            sqrt_dirty: true,
            div_result: 0,
            divrem_result: 0,
            div_dirty: true,
        }
    }

    pub fn get_sqrt_param_l(&self) -> u32 {
        self.context.sqrt_param as u32
    }

    pub fn get_sqrt_param_h(&self) -> u32 {
        (self.context.sqrt_param >> 32) as u32
    }

    pub fn get_sqrt_result(&mut self) -> u32 {
        self.sqrt();
        self.sqrt_result
    }

    fn sqrt(&mut self) {
        if !self.sqrt_dirty {
            return;
        }
        self.sqrt_dirty = false;
        if self.context.sqrt_cnt & 1 == 0 {
            self.sqrt_result = (self.context.sqrt_param as u32).isqrt();
        } else {
            self.sqrt_result = self.context.sqrt_param.isqrt() as u32;
        }
    }

    pub fn set_sqrt_cnt(&mut self, mut mask: u16, value: u16) {
        mask &= 0x1;
        self.context.sqrt_cnt = (self.context.sqrt_cnt & !mask) | (value & mask);
        self.sqrt_dirty = true;
    }

    pub fn set_sqrt_param_l(&mut self, mask: u32, value: u32) {
        self.context.sqrt_param = (self.context.sqrt_param & !(mask as u64)) | (value & mask) as u64;
        self.sqrt_dirty = true;
    }

    pub fn set_sqrt_param_h(&mut self, mask: u32, value: u32) {
        self.context.sqrt_param = (self.context.sqrt_param & !((mask as u64) << 32)) | (((value & mask) as u64) << 32);
        self.sqrt_dirty = true;
    }

    pub fn get_div_numer_l(&self) -> u32 {
        self.context.div_numer as u32
    }

    pub fn get_div_numer_h(&self) -> u32 {
        (self.context.div_numer as u64 >> 32) as u32
    }

    pub fn get_div_denom_l(&self) -> u32 {
        self.context.div_denom as u32
    }

    pub fn get_div_denom_h(&self) -> u32 {
        (self.context.div_denom as u64 >> 32) as u32
    }

    pub fn get_div_result_l(&mut self) -> u32 {
        self.div();
        self.div_result as u32
    }

    pub fn get_div_result_h(&mut self) -> u32 {
        self.div();
        (self.div_result as u64 >> 32) as u32
    }

    pub fn get_divrem_result_l(&mut self) -> u32 {
        self.div();
        self.divrem_result as u32
    }

    pub fn get_divrem_result_h(&mut self) -> u32 {
        self.div();
        (self.divrem_result as u64 >> 32) as u32
    }

    fn div(&mut self) {
        if !self.div_dirty {
            return;
        }
        self.div_dirty = false;
        if self.context.div_denom == 0 {
            self.context.div_cnt |= 1 << 14;
        } else {
            self.context.div_cnt &= !(1 << 14);
        }

        match self.context.div_cnt & 0x3 {
            0 => {
                let num = self.context.div_numer as i32;
                let denom = self.context.div_denom as i32;
                if num == i32::MIN && denom == -1 {
                    self.div_result = (num as u64 ^ ((!0u32 as u64) << 32)) as i64;
                    self.divrem_result = 0;
                } else if denom != 0 {
                    self.div_result = unsafe { unchecked_div(num, denom) } as i64;
                    self.divrem_result = unsafe { unchecked_rem(num, denom) } as i64;
                } else {
                    self.div_result = (if num < 0 { 1 } else { -1i32 } as u64 ^ ((!0u32 as u64) << 32)) as i64;
                    self.divrem_result = num as i64;
                }
            }
            1 => {
                let num = self.context.div_numer;
                let denom = self.context.div_denom as i32;
                if num == i64::MIN && denom == -1 {
                    self.div_result = num;
                    self.divrem_result = 0;
                } else if denom != 0 {
                    self.div_result = unsafe { unchecked_div(num, denom as i64) };
                    self.divrem_result = unsafe { unchecked_rem(num, denom as i64) };
                } else {
                    self.div_result = if num < 0 { 1 } else { -1 };
                    self.divrem_result = num;
                }
            }
            2 => {
                let num = self.context.div_numer;
                let denom = self.context.div_denom;
                if num == i64::MIN && denom == -1 {
                    self.div_result = num;
                    self.divrem_result = 0;
                } else if denom != 0 {
                    self.div_result = unsafe { unchecked_div(num, denom) };
                    self.divrem_result = unsafe { unchecked_rem(num, denom) };
                } else {
                    self.div_result = if num < 0 { 1 } else { -1 };
                    self.divrem_result = num;
                }
            }
            _ => unsafe { unreachable_unchecked() },
        }
    }

    pub fn set_div_cnt(&mut self, mut mask: u16, value: u16) {
        mask &= 0x3;
        self.context.div_cnt = (self.context.div_cnt & !mask) | (value & mask);
        self.div_dirty = true;
    }

    pub fn set_div_numer_l(&mut self, mask: u32, value: u32) {
        self.context.div_numer = ((self.context.div_numer as u64 & !(mask as u64)) | (value & mask) as u64) as i64;
        self.div_dirty = true;
    }

    pub fn set_div_numer_h(&mut self, mask: u32, value: u32) {
        self.context.div_numer = ((self.context.div_numer as u64 & !((mask as u64) << 32)) | (((value & mask) as u64) << 32)) as i64;
        self.div_dirty = true;
    }

    pub fn set_div_denom_l(&mut self, mask: u32, value: u32) {
        self.context.div_denom = ((self.context.div_denom as u64 & !(mask as u64)) | (value & mask) as u64) as i64;
        self.div_dirty = true;
    }

    pub fn set_div_denom_h(&mut self, mask: u32, value: u32) {
        self.context.div_denom = ((self.context.div_denom as u64 & !((mask as u64) << 32)) | (((value & mask) as u64) << 32)) as i64;
        self.div_dirty = true;
    }

    pub fn get_context(&self, context: &mut CpContext) {
        context.div_numer = self.context.div_numer;
        context.div_denom = self.context.div_denom;
        context.sqrt_param = self.context.sqrt_param;
        context.div_cnt = self.context.div_cnt;
        context.sqrt_cnt = self.context.sqrt_cnt;
    }

    pub fn set_context(&mut self, context: &CpContext) {
        self.context.div_numer = context.div_numer;
        self.context.div_denom = context.div_denom;
        self.context.sqrt_param = context.sqrt_param;
        self.context.div_cnt = context.div_cnt;
        self.context.sqrt_cnt = context.sqrt_cnt;
        self.div_dirty = true;
        self.sqrt_dirty = true;
    }
}
