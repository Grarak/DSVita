pub struct GpuContext {
    pow_cnt1: u16,
}

impl GpuContext {
    pub fn new() -> Self {
        GpuContext { pow_cnt1: 0 }
    }

    // TODO
    pub fn set_pow_cnt1(&mut self, value: u16) -> u16 {
        self.pow_cnt1 = value;
        self.pow_cnt1
    }
}
