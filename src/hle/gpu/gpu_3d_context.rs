use bilge::prelude::*;

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
struct Disp3DCnt {
    texture_mapping: u1,
    polygon_attr_shading: u1,
    alpha_test: u1,
    alpha_blending: u1,
    anti_aliasing: u1,
    edge_marking: u1,
    alpha_mode: u1,
    fog_master_enable: u1,
    fog_depth_shift: u4,
    color_buf_rdlines_underflow: u1,
    polygon_ram_overflow: u1,
    rear_plane_mode: u1,
    not_used: u1,
}

pub struct Gpu3DContext {
    pub disp_cnt: Disp3DCnt,
}

impl Gpu3DContext {
    pub fn new() -> Self {
        Gpu3DContext {
            disp_cnt: Disp3DCnt::from(0),
        }
    }

    pub fn get_disp_3d_cnt(&self) -> u16 {
        u16::from(self.disp_cnt)
    }

    pub fn set_disp_3d_cnt(&mut self, mut mask: u16, value: u16) {
        let new_cnt = Disp3DCnt::from(value);
        if bool::from(new_cnt.color_buf_rdlines_underflow()) {
            self.disp_cnt.set_color_buf_rdlines_underflow(u1::new(0));
        }
        if bool::from(new_cnt.polygon_ram_overflow()) {
            self.disp_cnt.set_polygon_ram_overflow(u1::new(0));
        }

        mask &= 0x4FFF;
        let new_value = (u16::from(self.disp_cnt) & !mask) | (value & mask);
        if u16::from(self.disp_cnt) != new_value {
            self.disp_cnt = new_value.into();
            // TODO invalidate 3d
        }
    }

    pub fn set_clear_color(&mut self, mask: u32, value: u32) {}

    pub fn set_clear_depth(&mut self, mask: u16, value: u16) {}

    pub fn set_swap_buffers(&mut self, mask: u32, value: u32) {}

    pub fn set_viewport(&mut self, mask: u32, value: u32) {}

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
}
