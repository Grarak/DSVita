use crate::logging::debug_println;

pub struct GpuContext {
    pow_cnt1: u16,
}

impl GpuContext {
    pub fn new() -> Self {
        GpuContext { pow_cnt1: 0 }
    }

    // TODO
    pub fn set_pow_cnt1(&mut self, mask: u16, value: u16) {
        debug_println!("set pow cnt1 with mask {:x} and value {:x}", mask, value);
        self.pow_cnt1 = value & mask;
    }
}
