use crate::hle::CpuType;

pub struct GpuContext {
    disp_stat: [u16; 2],
    pow_cnt1: u16,
}

impl GpuContext {
    pub fn new() -> GpuContext {
        GpuContext {
            disp_stat: [0u16; 2],
            pow_cnt1: 0,
        }
    }

    pub fn get_disp_stat(&self, cpu_type: CpuType) -> u16 {
        self.disp_stat[cpu_type as usize]
    }

    pub fn set_disp_stat(&mut self, cpu_type: CpuType, mask: u16, value: u16) {
        self.disp_stat[cpu_type as usize] =
            (self.disp_stat[cpu_type as usize] & !mask) | (value & mask);
    }

    pub fn set_pow_cnt1(&mut self, mask: u16, value: u16) {
        self.pow_cnt1 = (self.pow_cnt1 & !mask) | (value & mask);
    }
}
