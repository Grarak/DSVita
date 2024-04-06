pub struct Gpu3D {
    pub gx_stat: u32,
}

impl Gpu3D {
    pub fn new() -> Self {
        Gpu3D {
            gx_stat: 0x04000000,
        }
    }

    pub fn get_clip_mtx_result(&self, index: usize) -> u32 {
        0
    }

    pub fn set_swap_buffers(&mut self, mask: u32, value: u32) {}

    pub fn set_viewport(&mut self, mask: u32, value: u32) {}

    pub fn set_box_test(&mut self, mask: u32, value: u32) {}

    pub fn set_pos_test(&mut self, mask: u32, value: u32) {}

    pub fn set_vec_test(&mut self, mask: u32, value: u32) {}

    pub fn set_gx_fifo(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_mode(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_push(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_pop(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_store(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_restore(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_identity(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_load44(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_load43(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_mult44(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_mult43(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_mult33(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_scale(&mut self, mask: u32, value: u32) {}

    pub fn set_mtx_trans(&mut self, mask: u32, value: u32) {}

    pub fn set_color(&mut self, mask: u32, value: u32) {}

    pub fn set_normal(&mut self, mask: u32, value: u32) {}

    pub fn set_tex_coord(&mut self, mask: u32, value: u32) {}

    pub fn set_vtx16(&mut self, mask: u32, value: u32) {}

    pub fn set_vtx10(&mut self, mask: u32, value: u32) {}

    pub fn set_vtx_x_y(&mut self, mask: u32, value: u32) {}

    pub fn set_vtx_x_z(&mut self, mask: u32, value: u32) {}

    pub fn set_vtx_y_z(&mut self, mask: u32, value: u32) {}

    pub fn set_vtx_diff(&mut self, mask: u32, value: u32) {}

    pub fn set_polygon_attr(&mut self, mask: u32, value: u32) {}

    pub fn set_tex_image_param(&mut self, mask: u32, value: u32) {}

    pub fn set_pltt_base(&mut self, mask: u32, value: u32) {}

    pub fn set_dif_amb(&mut self, mask: u32, value: u32) {}

    pub fn set_spe_emi(&mut self, mask: u32, value: u32) {}

    pub fn set_light_vector(&mut self, mask: u32, value: u32) {}

    pub fn set_light_color(&mut self, mask: u32, value: u32) {}

    pub fn set_shininess(&mut self, mask: u32, value: u32) {}

    pub fn set_begin_vtxs(&mut self, mask: u32, value: u32) {}

    pub fn set_end_vtxs(&mut self, mask: u32, value: u32) {}

    pub fn set_gx_stat(&mut self, mut mask: u32, value: u32) {
        if value & (1 << 15) != 0 {
            self.gx_stat &= !0xA000;
        }

        mask &= 0xC0000000;
        self.gx_stat = (self.gx_stat & !mask) | (value & mask);
    }
}
