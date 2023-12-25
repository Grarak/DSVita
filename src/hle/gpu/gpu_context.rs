pub struct GpuContext {
    disp_stat: u16,
    pow_cnt1: u16,
}

impl GpuContext {
    pub fn new() -> GpuContext {
        GpuContext {
            disp_stat: 0,
            pow_cnt1: 0,
        }
    }

    pub fn set_disp_stat(&mut self, mask: u16, value: u16) {
        self.disp_stat = (self.disp_stat & !mask) | (value & mask);
    }

    pub fn set_pow_cnt1(&mut self, mask: u16, value: u16) {
        self.pow_cnt1 = (self.pow_cnt1 & !mask) | (value & mask);
    }
}
