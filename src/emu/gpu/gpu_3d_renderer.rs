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

pub struct Gpu3dRenderer {
    disp_cnt: Disp3DCnt,
}

impl Gpu3dRenderer {
    pub fn new() -> Self {
        Gpu3dRenderer { disp_cnt: Disp3DCnt::from(0) }
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

    pub fn set_edge_color(&mut self, index: usize, mask: u16, value: u16) {}

    pub fn set_clear_color(&mut self, mask: u32, value: u32) {}

    pub fn set_clear_depth(&mut self, mask: u16, value: u16) {}

    pub fn set_toon_table(&mut self, index: usize, mask: u16, value: u16) {}

    pub fn set_fog_color(&mut self, mask: u32, value: u32) {}

    pub fn set_fog_offset(&mut self, mask: u16, value: u16) {}

    pub fn set_fog_table(&mut self, index: usize, value: u8) {}
}
