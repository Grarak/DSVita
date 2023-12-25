pub struct Gpu2DContext {
    disp_cnt: u32,
    disp_stat: u16,
    pow_cnt1: u16,
}

impl Gpu2DContext {
    pub fn new() -> Self {
        Gpu2DContext {
            disp_cnt: 0,
            disp_stat: 0,
            pow_cnt1: 0,
        }
    }

    pub fn set_disp_cnt(&mut self, mask: u32, value: u32) {
        self.disp_cnt = (self.disp_cnt & !mask) | (value & mask);
    }

    pub fn set_bg_cnt(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_h_ofs(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_v_ofs(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_p_a(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_p_b(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_p_c(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_p_d(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_bg_x(&mut self, _: usize, mask: u32, value: u32) {}

    pub fn set_bg_y(&mut self, _: usize, mask: u32, value: u32) {}

    pub fn set_win_h(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_win_v(&mut self, _: usize, mask: u16, value: u16) {}

    pub fn set_win_in(&mut self, mask: u16, value: u16) {}

    pub fn set_win_out(&mut self, mask: u16, value: u16) {}

    pub fn set_mosaic(&mut self, mask: u16, value: u16) {}

    pub fn set_bld_cnt(&mut self, mask: u16, value: u16) {}

    pub fn set_bld_alpha(&mut self, mask: u16, value: u16) {}

    pub fn set_bld_y(&mut self, value: u8) {}

    pub fn set_master_bright(&mut self, mask: u16, value: u16) {}
}
