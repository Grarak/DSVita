use bilge::prelude::*;

#[bitsize(16)]
#[derive(FromBits)]
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

pub struct Gpu3DContext {}

impl Gpu3DContext {
    pub fn new() -> Self {
        Gpu3DContext {}
    }

    pub fn set_disp_3d_cnt(&mut self, mask: u16, value: u16) {}
}
