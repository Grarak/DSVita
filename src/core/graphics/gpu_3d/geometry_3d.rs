use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_3d::registers_3d::{GxStat, FIFO_PARAM_COUNTS, FUNC_NAME_LUT};
use crate::core::graphics::gpu_3d::renderer_3d::Gpu3DRendererContent;
use crate::logging::debug_println;
use crate::math::{vmult_mat4, Matrix, Vectori16, Vectori32};
use crate::utils::{rgb5_to_rgb6, HeapMem};
use bilge::prelude::*;
use std::arch::arm::{int32x4_t, vld1q_s32, vld1q_s32_x3, vsetq_lane_s32};
use std::cell::UnsafeCell;
use std::hint::assert_unchecked;
use std::intrinsics::unlikely;
use std::ops::DerefMut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::{mem, ptr, thread};

static FUNC_LUT: [fn(&mut Gpu3DGeometry, params: &[u32; 32]); 128] = [
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_mtx_mode,
    Gpu3DGeometry::exe_mtx_push,
    Gpu3DGeometry::exe_mtx_pop,
    Gpu3DGeometry::exe_mtx_store,
    Gpu3DGeometry::exe_mtx_restore,
    Gpu3DGeometry::exe_mtx_identity,
    Gpu3DGeometry::exe_mtx_load44,
    Gpu3DGeometry::exe_mtx_load43,
    Gpu3DGeometry::exe_mtx_mult44,
    Gpu3DGeometry::exe_mtx_mult43,
    Gpu3DGeometry::exe_mtx_mult33,
    Gpu3DGeometry::exe_mtx_scale,
    Gpu3DGeometry::exe_mtx_trans,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_color,
    Gpu3DGeometry::exe_normal,
    Gpu3DGeometry::exe_tex_coord,
    Gpu3DGeometry::exe_vtx16,
    Gpu3DGeometry::exe_vtx10,
    Gpu3DGeometry::exe_vtx_x_y,
    Gpu3DGeometry::exe_vtx_x_z,
    Gpu3DGeometry::exe_vtx_y_z,
    Gpu3DGeometry::exe_vtx_diff,
    Gpu3DGeometry::exe_polygon_attr,
    Gpu3DGeometry::exe_tex_image_param,
    Gpu3DGeometry::exe_pltt_base,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_dif_amb,
    Gpu3DGeometry::exe_spe_emi,
    Gpu3DGeometry::exe_light_vector,
    Gpu3DGeometry::exe_light_color,
    Gpu3DGeometry::exe_shininess,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_begin_vtxs,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_swap_buffers,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_viewport,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_box_test,
    Gpu3DGeometry::exe_pos_test,
    Gpu3DGeometry::exe_vec_test,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
    Gpu3DGeometry::exe_empty,
];

pub const POLYGON_LIMIT: usize = 8192;
pub const VERTEX_LIMIT: usize = POLYGON_LIMIT * 4;

#[bitsize(8)]
#[derive(Copy, Clone, Default, FromBits)]
pub struct SwapBuffers {
    pub manual_sort_translucent_polygon: bool,
    pub depth_buffering_w: bool,
    not_used: u6,
}

#[derive(Default)]
#[repr(u8)]
enum MtxMode {
    #[default]
    Projection = 0,
    ModelView = 1,
    ModelViewVec = 2,
    Texture = 3,
}

impl From<u8> for MtxMode {
    fn from(value: u8) -> Self {
        debug_assert!(value <= MtxMode::Texture as u8);
        unsafe { mem::transmute(value) }
    }
}

#[derive(Default)]
pub struct Matrices {
    proj: Matrix,
    proj_stack: Matrix,
    coord: Matrix,
    coord_stack: [Matrix; 32],
    pub dir: Matrix,
    dir_stack: [Matrix; 32],
    tex: Matrix,
    tex_stack: Matrix,
}

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
struct MaterialColor0 {
    dif: u15,
    set_vertex_color: bool,
    amb: u15,
    not_used: u1,
}

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
struct MaterialColor1 {
    spe: u15,
    set_shininess: bool,
    em: u15,
    not_used: u1,
}

#[bitsize(32)]
#[derive(Copy, Clone, FromBits)]
pub struct Viewport {
    pub x1: u8,
    pub y1: u8,
    pub x2: u8,
    pub y2: u8,
}

impl Default for Viewport {
    fn default() -> Self {
        let mut viewport = Viewport::from(0);
        viewport.set_x2(DISPLAY_WIDTH as u8);
        viewport.set_y2(DISPLAY_HEIGHT as u8);
        viewport
    }
}

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
pub struct TexImageParam {
    pub vram_offset: u16,
    pub repeat_s: bool,
    pub repeat_t: bool,
    pub flip_s: bool,
    pub flip_t: bool,
    pub size_s_shift: u3,
    pub size_t_shift: u3,
    pub format: u3,
    color_0_transparent: bool,
    pub coord_trans_mode: u2,
}

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
struct NormalVector {
    x: u10,
    y: u10,
    z: u10,
    not_used: u2,
}

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
struct TexCoord {
    s: u16,
    t: u16,
}

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
struct LightVector {
    x: u10,
    y: u10,
    z: u10,
    num: u2,
}

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
struct LightColor {
    color: u15,
    not_used: u15,
    num: u2,
}

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
struct Shininess {
    shininess0: u8,
    shininess1: u8,
    shininess2: u8,
    shininess3: u8,
}

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
pub struct PolygonAttr {
    enable_lights: u4,
    mode: u2,
    render_back: bool,
    render_front: bool,
    not_used: u3,
    trans_new_depth: bool,
    render_far_plane: bool,
    render_1_bot_polygons: bool,
    depth_test_equal: bool,
    fog: bool,
    pub alpha: u5,
    not_used2: u3,
    id: u6,
    not_used3: u2,
}

#[derive(Copy, Clone, Default)]
#[repr(u8)]
enum PolygonMode {
    #[default]
    Modulation = 0,
    Decal = 1,
    Toon = 2,
    Shadow = 3,
}

impl From<u8> for PolygonMode {
    fn from(value: u8) -> Self {
        debug_assert!(value <= PolygonMode::Shadow as u8);
        unsafe { mem::transmute(value) }
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum TextureFormat {
    #[default]
    None = 0,
    A3I5Translucent = 1,
    Color4Palette = 2,
    Color16Palette = 3,
    Color256Palette = 4,
    Texel4x4Compressed = 5,
    A5I3Translucent = 6,
    Direct = 7,
}

impl From<u8> for TextureFormat {
    fn from(value: u8) -> Self {
        debug_assert!(value <= TextureFormat::Direct as u8);
        unsafe { mem::transmute(value) }
    }
}

#[derive(Copy, Clone, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum TextureCoordTransMode {
    #[default]
    None = 0,
    TexCoord = 1,
    Normal = 2,
    Vertex = 3,
}

impl From<u8> for TextureCoordTransMode {
    fn from(value: u8) -> Self {
        debug_assert!(value <= TextureCoordTransMode::Vertex as u8);
        unsafe { mem::transmute(value) }
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum PrimitiveType {
    #[default]
    SeparateTriangles = 0,
    SeparateQuadliterals = 1,
    TriangleStrips = 2,
    QuadliteralStrips = 3,
}

impl PrimitiveType {
    pub fn vertex_count(self) -> u8 {
        match self {
            PrimitiveType::SeparateTriangles => 3,
            PrimitiveType::SeparateQuadliterals => 4,
            PrimitiveType::TriangleStrips => 3,
            PrimitiveType::QuadliteralStrips => 4,
        }
    }
}

impl From<u8> for PrimitiveType {
    fn from(value: u8) -> Self {
        debug_assert!(value <= PrimitiveType::QuadliteralStrips as u8);
        unsafe { mem::transmute(value) }
    }
}

#[derive(Copy, Clone, Default)]
pub struct Vertex {
    pub coords: Vectori32<4>,
    pub tex_coords: Vectori16<2>,
    pub color: u32,
}

#[derive(Copy, Clone, Default)]
pub struct Polygon {
    pub attr: PolygonAttr,
    pub tex_image_param: TexImageParam,
    pub palette_addr: u16,
    pub polygon_type: PrimitiveType,
    pub vertices_index: u16,
    pub viewport: Viewport,
}

#[derive(Default)]
pub struct Syncing {
    pub swap_mutex: Mutex<()>,
}

#[derive(Default)]
pub struct Gpu3DGeometry {
    pub gx_stat: GxStat,

    pub executed_mtx_push_pop: u32,
    pub executed_tests: u8,

    mtx_mode: MtxMode,
    pub matrices: Matrices,

    cur_viewport: Viewport,

    vertex_list_primitive_type: PrimitiveType,
    vertex_list_size: u16,

    cur_tex_coords: Vectori16<2>,
    cur_tex_coord_trans_mode: TextureCoordTransMode,

    cur_vtx: Vertex,
    vertices: HeapMem<Vertex, VERTEX_LIMIT>,
    vertices_size: u16,
    vertices_flushed: HeapMem<Vertex, VERTEX_LIMIT>,
    pub vertices_flushed_size: u16,

    cur_polygon: Polygon,
    polygons: HeapMem<Polygon, POLYGON_LIMIT>,
    polygons_size: u16,
    polygons_flushed: HeapMem<Polygon, POLYGON_LIMIT>,
    pub polygons_flushed_size: u16,

    clip_mtx: Matrix,
    clip_mtx_dirty: bool,

    cur_polygon_attr: PolygonAttr,

    material_color0: MaterialColor0,
    material_color1: MaterialColor1,

    pub pos_result: Vectori32<4>,
    pub vec_result: Vectori16<3>,

    pub syncing: UnsafeCell<Syncing>,
    pub cmds: Vec<u32>,
    pub cmds_end: usize,
    pub processing: AtomicBool,
    pub swap_buffers: SwapBuffers,
    pub swapped: bool,
    pub needs_sync: bool,
    pub thread_id: i32
}

impl Gpu3DGeometry {
    pub fn run(&mut self) {
        // self.thread_id = unsafe { vitasdk_sys::sceKernelGetThreadId() };
        loop {
            // println!("gx: start");
            while !self.processing.load(Ordering::Acquire) {}
            // unsafe { vitasdk_sys::sceKernelWaitSignal(0, 0, ptr::null_mut()) };

            unsafe { assert_unchecked(self.cmds_end <= self.cmds.len()) };
            let mut process_offset = 0;
            while process_offset < self.cmds_end {
                let cmd = self.cmds[process_offset] as usize;
                process_offset += 1;

                let param_count = cmd >> 8;
                let cmd = cmd & 0x7F;

                // println!("gx: {} {cmd:x} {param_count}", unsafe { FUNC_NAME_LUT.get_unchecked(cmd) });

                let func = unsafe { FUNC_LUT.get_unchecked(cmd) };
                func(self, unsafe { (self.cmds[process_offset..].as_ptr() as *const [u32; 32]).as_ref_unchecked() });

                process_offset += param_count;
            }

            // println!("gx: end");
            self.needs_sync = true;
            self.processing.store(false, Ordering::Release);
        }
    }

    pub fn get_clip_mtx(&mut self) -> &Matrix {
        if self.clip_mtx_dirty {
            self.clip_mtx_dirty = false;
            self.clip_mtx = self.matrices.coord * &self.matrices.proj;
        }
        &self.clip_mtx
    }

    fn exe_empty(&mut self, _: &[u32; 32]) {}

    fn exe_mtx_mode(&mut self, params: &[u32; 32]) {
        self.mtx_mode = MtxMode::from((params[0] & 0x3) as u8);
    }

    fn exe_mtx_push(&mut self, _: &[u32; 32]) {
        self.executed_mtx_push_pop += 1;
        match self.mtx_mode {
            MtxMode::Projection => {
                if u8::from(self.gx_stat.proj_mtx_stack_lvl()) == 0 {
                    self.matrices.proj_stack = self.matrices.proj;
                    self.gx_stat.set_proj_mtx_stack_lvl(u1::new(1));
                } else {
                    self.gx_stat.set_mtx_stack_overflow_underflow_err(true);
                }
            }
            MtxMode::ModelView | MtxMode::ModelViewVec => {
                let ptr = u8::from(self.gx_stat.pos_vec_mtx_stack_lvl());

                if ptr >= 30 {
                    self.gx_stat.set_mtx_stack_overflow_underflow_err(true);
                }

                if ptr < 31 {
                    self.matrices.coord_stack[ptr as usize] = self.matrices.coord;
                    self.matrices.dir_stack[ptr as usize] = self.matrices.dir;
                    self.gx_stat.set_pos_vec_mtx_stack_lvl(u5::new(ptr + 1));
                }
            }
            MtxMode::Texture => self.matrices.tex_stack = self.matrices.tex,
        }
    }

    fn exe_mtx_pop(&mut self, params: &[u32; 32]) {
        self.executed_mtx_push_pop += 1;
        match self.mtx_mode {
            MtxMode::Projection => {
                if u8::from(self.gx_stat.proj_mtx_stack_lvl()) == 1 {
                    self.matrices.proj = self.matrices.proj_stack;
                    self.gx_stat.set_proj_mtx_stack_lvl(u1::new(0));
                    self.clip_mtx_dirty = true;
                } else {
                    self.gx_stat.set_mtx_stack_overflow_underflow_err(true);
                }
            }
            MtxMode::ModelView | MtxMode::ModelViewVec => {
                let ptr = (u8::from(self.gx_stat.pos_vec_mtx_stack_lvl()) as i8 - (((params[0] << 2) as i8) >> 2)) as u8;
                if ptr >= 30 {
                    self.gx_stat.set_mtx_stack_overflow_underflow_err(true);
                }

                if ptr < 31 {
                    self.gx_stat.set_pos_vec_mtx_stack_lvl(u5::new(ptr));
                    self.matrices.coord = self.matrices.coord_stack[ptr as usize];
                    self.matrices.dir = self.matrices.dir_stack[ptr as usize];
                    self.clip_mtx_dirty = true;
                }
            }
            MtxMode::Texture => self.matrices.tex = self.matrices.tex_stack,
        }
    }

    fn exe_mtx_store(&mut self, params: &[u32; 32]) {
        match self.mtx_mode {
            MtxMode::Projection => self.matrices.proj_stack = self.matrices.proj,
            MtxMode::ModelView | MtxMode::ModelViewVec => {
                let addr = params[0] & 0x1F;

                if addr == 31 {
                    self.gx_stat.set_mtx_stack_overflow_underflow_err(true);
                }

                self.matrices.coord_stack[addr as usize] = self.matrices.coord;
                self.matrices.dir_stack[addr as usize] = self.matrices.dir;
            }
            MtxMode::Texture => self.matrices.tex_stack = self.matrices.tex,
        }
    }

    fn exe_mtx_restore(&mut self, params: &[u32; 32]) {
        match self.mtx_mode {
            MtxMode::Projection => {
                self.matrices.proj = self.matrices.proj_stack;
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelView | MtxMode::ModelViewVec => {
                let addr = params[0] & 0x1F;

                if addr == 31 {
                    self.gx_stat.set_mtx_stack_overflow_underflow_err(true);
                }

                self.matrices.coord = self.matrices.coord_stack[addr as usize];
                self.matrices.dir = self.matrices.dir_stack[addr as usize];
                self.clip_mtx_dirty = true;
            }
            MtxMode::Texture => self.matrices.tex = self.matrices.tex_stack,
        }
    }

    fn exe_mtx_identity(&mut self, _: &[u32; 32]) {
        match self.mtx_mode {
            MtxMode::Projection => {
                self.matrices.proj = Matrix::default();
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelView => {
                self.matrices.coord = Matrix::default();
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelViewVec => {
                self.matrices.coord = Matrix::default();
                self.matrices.dir = Matrix::default();
                self.clip_mtx_dirty = true;
            }
            MtxMode::Texture => self.matrices.tex = Matrix::default(),
        }
    }

    fn mtx_load(&mut self, mtx: Matrix) {
        match self.mtx_mode {
            MtxMode::Projection => {
                self.matrices.proj = mtx;
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelView => {
                self.matrices.coord = mtx;
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelViewVec => {
                self.matrices.coord = mtx;
                self.matrices.dir = mtx;
                self.clip_mtx_dirty = true;
            }
            MtxMode::Texture => self.matrices.tex = mtx,
        }
    }

    fn exe_mtx_load44(&mut self, params: &[u32; 32]) {
        let params: &[u32; 16] = unsafe { mem::transmute(params) };
        self.mtx_load(unsafe { mem::transmute(*params) });
    }

    fn exe_mtx_load43(&mut self, params: &[u32; 32]) {
        let load = |mtx: &mut Matrix| {
            for i in 0..4 {
                mtx.as_mut()[i * 4..i * 4 + 3].copy_from_slice(unsafe { mem::transmute(&params[i * 3..i * 3 + 3]) });
            }
            mtx[3] = 0;
            mtx[7] = 0;
            mtx[11] = 0;
            mtx[15] = 1 << 12;
        };
        match self.mtx_mode {
            MtxMode::Projection => {
                load(&mut self.matrices.proj);
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelView => {
                load(&mut self.matrices.coord);
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelViewVec => {
                load(&mut self.matrices.coord);
                load(&mut self.matrices.dir);
                self.clip_mtx_dirty = true;
            }
            MtxMode::Texture => load(&mut self.matrices.tex),
        }
    }

    #[inline(always)]
    fn mtx_mult(&mut self, mtx: [int32x4_t; 4]) {
        match self.mtx_mode {
            MtxMode::Projection => {
                unsafe { vmult_mat4(mtx, self.matrices.proj.vld(), &mut self.matrices.proj.0) };
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelView => {
                unsafe { vmult_mat4(mtx, self.matrices.coord.vld(), &mut self.matrices.coord.0) };
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelViewVec => {
                unsafe {
                    vmult_mat4(mtx, self.matrices.coord.vld(), &mut self.matrices.coord.0);
                    vmult_mat4(mtx, self.matrices.dir.vld(), &mut self.matrices.dir.0);
                }
                self.clip_mtx_dirty = true;
            }
            MtxMode::Texture => unsafe { vmult_mat4(mtx, self.matrices.tex.vld(), &mut self.matrices.tex.0) },
        }
    }

    fn exe_mtx_mult44(&mut self, params: &[u32; 32]) {
        let mtx: &Matrix = unsafe { mem::transmute(params) };
        self.mtx_mult(unsafe { mtx.vld() });
    }

    fn exe_mtx_mult43(&mut self, params: &[u32; 32]) {
        unsafe {
            let r0 = vld1q_s32(params.as_ptr() as *const i32);
            let r0 = vsetq_lane_s32::<3>(0, r0);
            let r1 = vld1q_s32((params.as_ptr() as *const i32).add(3));
            let r1 = vsetq_lane_s32::<3>(0, r1);
            let r2 = vld1q_s32((params.as_ptr() as *const i32).add(6));
            let r2 = vsetq_lane_s32::<3>(0, r2);
            let r3 = vld1q_s32((params.as_ptr() as *const i32).add(9));
            let r3 = vsetq_lane_s32::<3>(1 << 12, r3);
            self.mtx_mult([r0, r1, r2, r3]);
        }
    }

    fn exe_mtx_mult33(&mut self, params: &[u32; 32]) {
        unsafe {
            let r0 = vld1q_s32(params.as_ptr() as *const i32);
            let r0 = vsetq_lane_s32::<3>(0, r0);
            let r1 = vld1q_s32((params.as_ptr() as *const i32).add(3));
            let r1 = vsetq_lane_s32::<3>(0, r1);
            let r2 = vld1q_s32((params.as_ptr() as *const i32).add(6));
            let r2 = vsetq_lane_s32::<3>(0, r2);
            let r3 = vld1q_s32([0, 0, 0, 1 << 12].as_ptr());
            self.mtx_mult([r0, r1, r2, r3]);
        }
    }

    fn exe_mtx_scale(&mut self, params: &[u32; 32]) {
        let mut mtx = Matrix::default();
        for i in 0..3 {
            mtx[i * 5] = params[i] as i32;
        }
        match self.mtx_mode {
            MtxMode::Projection => {
                self.matrices.proj = mtx * &self.matrices.proj;
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelView | MtxMode::ModelViewVec => {
                self.matrices.coord = mtx * &self.matrices.coord;
                self.clip_mtx_dirty = true;
            }
            MtxMode::Texture => self.matrices.tex = mtx * &self.matrices.tex,
        }
    }

    fn exe_mtx_trans(&mut self, params: &[u32; 32]) {
        let mtx = Matrix::default();
        let mtx = unsafe { vld1q_s32_x3(mtx.0.as_ptr()) };
        let trans_vector = unsafe {
            let vector = vld1q_s32(params.as_ptr() as _);
            vsetq_lane_s32::<3>(1 << 12, vector)
        };
        self.mtx_mult([mtx.0, mtx.1, mtx.2, trans_vector]);
    }

    fn exe_color(&mut self, params: &[u32; 32]) {
        self.cur_vtx.color = rgb5_to_rgb6(params[0]);
    }

    fn exe_normal(&mut self, params: &[u32; 32]) {
        let normal_vector_param = NormalVector::from(params[0]);
        let normal_vec = Vectori16([
            ((u16::from(normal_vector_param.x()) << 6) as i16) >> 3,
            ((u16::from(normal_vector_param.y()) << 6) as i16) >> 3,
            ((u16::from(normal_vector_param.z()) << 6) as i16) >> 3,
        ]);

        if self.cur_tex_coord_trans_mode == TextureCoordTransMode::Normal {
            let mut vector = Vectori32::new([normal_vec[0] as i32, normal_vec[1] as i32, normal_vec[2] as i32, 1 << 12]);

            let mut tex_mtx = self.matrices.tex;
            tex_mtx[12] = (self.cur_tex_coords[0] as i32) << 12;
            tex_mtx[13] = (self.cur_tex_coords[1] as i32) << 12;

            vector *= &tex_mtx;

            self.cur_vtx.tex_coords[0] = (vector[0] >> 12) as i16;
            self.cur_vtx.tex_coords[1] = (vector[1] >> 12) as i16;
        }
    }

    fn exe_tex_coord(&mut self, params: &[u32; 32]) {
        let tex_coord = TexCoord::from(params[0]);
        self.cur_tex_coords[0] = tex_coord.s() as i16;
        self.cur_tex_coords[1] = tex_coord.t() as i16;

        if self.cur_tex_coord_trans_mode == TextureCoordTransMode::TexCoord {
            let mut vector = Vectori32::new([(self.cur_tex_coords[0] as i32) << 8, (self.cur_tex_coords[1] as i32) << 8, 1 << 8, 1 << 8]);
            vector *= &self.matrices.tex;
            self.cur_vtx.tex_coords[0] = (vector[0] >> 8) as i16;
            self.cur_vtx.tex_coords[1] = (vector[1] >> 8) as i16;
        } else {
            self.cur_vtx.tex_coords = self.cur_tex_coords;
        }
    }

    fn exe_vtx16(&mut self, params: &[u32; 32]) {
        self.cur_vtx.coords[0] = params[0] as i16 as i32;
        self.cur_vtx.coords[1] = (params[0] >> 16) as i16 as i32;
        self.cur_vtx.coords[2] = params[1] as i16 as i32;
        self.add_vertex();
    }

    fn exe_vtx10(&mut self, params: &[u32; 32]) {
        self.cur_vtx.coords[0] = ((params[0] & 0x3FF) << 6) as i16 as i32;
        self.cur_vtx.coords[1] = ((params[0] & 0xFFC00) >> 4) as i16 as i32;
        self.cur_vtx.coords[2] = ((params[0] & 0x3FF00000) >> 14) as i16 as i32;
        self.add_vertex();
    }

    fn exe_vtx_x_y(&mut self, params: &[u32; 32]) {
        self.cur_vtx.coords[0] = params[0] as i16 as i32;
        self.cur_vtx.coords[1] = (params[0] >> 16) as i16 as i32;
        self.add_vertex();
    }

    fn exe_vtx_x_z(&mut self, params: &[u32; 32]) {
        self.cur_vtx.coords[0] = params[0] as i16 as i32;
        self.cur_vtx.coords[2] = (params[0] >> 16) as i16 as i32;
        self.add_vertex();
    }

    fn exe_vtx_y_z(&mut self, params: &[u32; 32]) {
        self.cur_vtx.coords[1] = params[0] as i16 as i32;
        self.cur_vtx.coords[2] = (params[0] >> 16) as i16 as i32;
        self.add_vertex();
    }

    fn exe_vtx_diff(&mut self, params: &[u32; 32]) {
        self.cur_vtx.coords[0] += (((params[0] & 0x3FF) << 6) as i16 as i32) >> 6;
        self.cur_vtx.coords[1] += (((params[0] & 0xFFC00) >> 4) as i16 as i32) >> 6;
        self.cur_vtx.coords[2] += (((params[0] & 0x3FF00000) >> 14) as i16 as i32) >> 6;
        self.add_vertex();
    }

    fn exe_polygon_attr(&mut self, params: &[u32; 32]) {
        self.cur_polygon_attr = params[0].into();
    }

    fn exe_tex_image_param(&mut self, params: &[u32; 32]) {
        self.cur_polygon.tex_image_param = TexImageParam::from(params[0]);
        self.cur_tex_coord_trans_mode = TextureCoordTransMode::from(u8::from(self.cur_polygon.tex_image_param.coord_trans_mode()));
    }

    fn exe_pltt_base(&mut self, params: &[u32; 32]) {
        self.cur_polygon.palette_addr = (params[0] & 0x1FFF) as u16;
    }

    fn exe_dif_amb(&mut self, params: &[u32; 32]) {
        self.material_color0 = MaterialColor0::from(params[0]);

        if self.material_color0.set_vertex_color() {
            self.cur_vtx.color = rgb5_to_rgb6(u32::from(self.material_color0.dif()));
        }
    }

    fn exe_spe_emi(&mut self, params: &[u32; 32]) {
        self.material_color1 = MaterialColor1::from(params[0]);
    }

    fn exe_light_vector(&mut self, params: &[u32; 32]) {
        // TODO
    }

    fn exe_light_color(&mut self, params: &[u32; 32]) {
        // TODO
    }

    fn exe_shininess(&mut self, params: &[u32; 32]) {
        // TODO
    }

    fn exe_begin_vtxs(&mut self, params: &[u32; 32]) {
        if self.vertex_list_size < self.vertex_list_primitive_type.vertex_count() as u16 {
            self.vertices_size -= self.vertex_list_size;
        }

        self.vertex_list_primitive_type = PrimitiveType::from((params[0] & 0x3) as u8);
        self.vertex_list_size = 0;
        self.cur_polygon.attr = self.cur_polygon_attr;
        self.cur_polygon.viewport = self.cur_viewport;
    }

    fn exe_swap_buffers(&mut self, params: &[u32; 32]) {
        let _lock = self.syncing.get_mut().swap_mutex.lock().unwrap();

        mem::swap(&mut self.vertices, &mut self.vertices_flushed);
        self.vertices_flushed_size = self.vertices_size;
        self.vertices_size = 0;

        mem::swap(&mut self.polygons, &mut self.polygons_flushed);
        self.polygons_flushed_size = self.polygons_size;
        self.polygons_size = 0;

        self.swap_buffers = SwapBuffers::from(params[0] as u8);
        self.swapped = true;
    }

    fn exe_viewport(&mut self, params: &[u32; 32]) {
        self.cur_viewport = Viewport::from(params[0]);
    }

    fn exe_box_test(&mut self, _: &[u32; 32]) {
        self.gx_stat.set_box_test_result(true);

        self.executed_tests += 1;
    }

    fn exe_pos_test(&mut self, params: &[u32; 32]) {
        self.cur_vtx.coords[0] = params[0] as i16 as i32;
        self.cur_vtx.coords[1] = (params[0] >> 16) as i16 as i32;
        self.cur_vtx.coords[2] = params[1] as i16 as i32;
        self.cur_vtx.coords[3] = 1 << 12;
        self.pos_result = self.cur_vtx.coords * self.get_clip_mtx();

        self.executed_tests += 1;
    }

    fn exe_vec_test(&mut self, params: &[u32; 32]) {
        let mut vector = Vectori32::<3>::new([
            (((params[0] & 0x000003FF) << 6) as i16 as i32) >> 3,
            (((params[0] & 0x000FFC00) >> 4) as i16 as i32) >> 3,
            (((params[0] & 0x3FF00000) >> 14) as i16 as i32) >> 3,
        ]);

        vector *= &self.matrices.dir;
        self.vec_result[0] = ((vector[0] << 3) as i16) >> 3;
        self.vec_result[1] = ((vector[1] << 3) as i16) >> 3;
        self.vec_result[2] = ((vector[2] << 3) as i16) >> 3;

        self.executed_tests += 1;
    }

    fn add_vertex(&mut self) {
        if self.vertices_size >= VERTEX_LIMIT as u16 {
            return;
        }

        self.cur_vtx.coords[3] = 1 << 12;

        if self.cur_tex_coord_trans_mode == TextureCoordTransMode::Vertex {
            let mut tex_mtx = self.matrices.tex;
            tex_mtx[12] = (self.cur_tex_coords[0] as i32) << 12;
            tex_mtx[13] = (self.cur_tex_coords[1] as i32) << 12;

            let vector = self.cur_vtx.coords * &tex_mtx;

            self.vertices[self.vertices_size as usize].tex_coords[0] = (vector[0] >> 12) as i16;
            self.vertices[self.vertices_size as usize].tex_coords[1] = (vector[1] >> 12) as i16;
        } else {
            self.vertices[self.vertices_size as usize].tex_coords = self.cur_vtx.tex_coords;
        }

        self.vertices[self.vertices_size as usize].coords = self.cur_vtx.coords * self.get_clip_mtx();
        self.vertices[self.vertices_size as usize].color = self.cur_vtx.color;

        self.vertices_size += 1;
        self.vertex_list_size += 1;

        if match self.vertex_list_primitive_type {
            PrimitiveType::SeparateTriangles => self.vertex_list_size % 3 == 0,
            PrimitiveType::SeparateQuadliterals => self.vertex_list_size % 4 == 0,
            PrimitiveType::TriangleStrips => self.vertex_list_size >= 3,
            PrimitiveType::QuadliteralStrips => self.vertex_list_size >= 4 && self.vertex_list_size % 2 == 0,
        } {
            self.add_polygon();
        }
    }

    fn add_polygon(&mut self) {
        if self.polygons_size as usize >= POLYGON_LIMIT {
            return;
        }

        let size = self.vertex_list_primitive_type.vertex_count();

        let polygon = &mut self.polygons[self.polygons_size as usize];
        *polygon = self.cur_polygon;
        polygon.polygon_type = self.vertex_list_primitive_type;
        polygon.vertices_index = self.vertices_size - size as u16;

        self.polygons_size += 1;
    }

    pub fn swap_to_renderer(&mut self, content: &mut Gpu3DRendererContent) {
        self.swapped = false;

        mem::swap(&mut self.vertices_flushed, &mut content.vertices);
        content.vertices_size = self.vertices_flushed_size;

        mem::swap(&mut self.polygons_flushed, &mut content.polygons);
        content.polygons_size = self.polygons_flushed_size;
    }
}
