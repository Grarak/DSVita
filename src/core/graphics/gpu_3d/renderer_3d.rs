use bilge::prelude::*;

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
struct Disp3DCnt {
    texture_mapping: bool,
    polygon_attr_shading: u1,
    alpha_test: bool,
    alpha_blending: bool,
    anti_aliasing: bool,
    edge_marking: bool,
    alpha_mode: bool,
    fog_master_enable: bool,
    fog_depth_shift: u4,
    color_buf_rdlines_underflow: bool,
    polygon_vertex_ram_overflow: bool,
    rear_plane_mode: u1,
    not_used: u1,
}

pub struct Gpu3DRenderer {
    disp_cnt: Disp3DCnt,
}

impl Gpu3DRenderer {
    pub fn new() -> Self {
        Gpu3DRenderer { disp_cnt: Disp3DCnt::from(0) }
    }

    pub fn get_disp_3d_cnt(&self) -> u16 {
        self.disp_cnt.into()
    }

    pub fn set_disp_3d_cnt(&mut self, mut mask: u16, value: u16) {
        let new_cnt = Disp3DCnt::from(value);
        if new_cnt.color_buf_rdlines_underflow() {
            self.disp_cnt.set_color_buf_rdlines_underflow(false);
        }
        if new_cnt.polygon_vertex_ram_overflow() {
            self.disp_cnt.set_polygon_vertex_ram_overflow(false);
        }

        mask &= 0x4FFF;
        let new_value = (u16::from(self.disp_cnt) & !mask) | (value & mask);
        if u16::from(self.disp_cnt) != new_value {
            self.disp_cnt = new_value.into();
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
