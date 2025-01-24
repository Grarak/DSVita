use crate::core::cpu_regs::InterruptFlag;
use crate::core::emu::{get_cm_mut, get_cpu_regs_mut, get_mem_mut, io_dma, Emu};
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::memory::dma::DmaTransferMode;
use crate::core::CpuType::ARM9;
use crate::fixed_fifo::FixedFifo;
use crate::math::{vdot_vec3, Matrix, Vectorf32, Vectori16, Vectori32, Vectoru8};
use crate::utils::{rgb5_to_rgb6, HeapMem};
use bilge::prelude::*;
use std::arch::arm::{
    vcvtq_f32_s32, vget_high_s32, vget_low_s32, vgetq_lane_s16, vgetq_lane_s32, vld1q_s32, vmlal_n_s32, vmull_n_s32, vmulq_n_f32, vshrn_n_s64, vshrq_n_s64, vst1q_f32, vst1q_s32, vuzp_s32, vuzpq_s32,
};
use std::cmp::max;
use std::hint::unreachable_unchecked;
use std::intrinsics::unlikely;
use std::mem;
use std::mem::MaybeUninit;

pub const POLYGON_LIMIT: usize = 8192;
pub const VERTEX_LIMIT: usize = POLYGON_LIMIT * 4;

#[bitsize(32)]
#[derive(Copy, Clone, FromBits)]
pub struct GxStat {
    box_pos_vec_test_busy: bool,
    box_test_result: bool,
    not_used: u6,
    pos_vec_mtx_stack_lvl: u5,
    proj_mtx_stack_lvl: u1,
    mtx_stack_busy: bool,
    mtx_stack_overflow_underflow_err: bool,
    num_entries_cmd_fifo: u9,
    pub cmd_fifo_less_half_full: bool,
    cmd_fifo_empty: bool,
    geometry_busy: bool,
    not_used2: u2,
    cmd_fifo_irq: u2,
}

impl Default for GxStat {
    fn default() -> Self {
        0x04000000.into()
    }
}

#[bitsize(32)]
#[derive(Copy, Clone, FromBits)]
struct Viewport {
    x1: u8,
    y1: u8,
    x2: u8,
    y2: u8,
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
    coord_trans_mode: u2,
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
#[derive(Copy, Clone, Default, FromBits)]
struct Shininess {
    shininess0: u8,
    shininess1: u8,
    shininess2: u8,
    shininess3: u8,
}

#[bitsize(32)]
#[derive(Copy, Clone, Default, FromBits)]
struct PolygonAttr {
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
    alpha: u5,
    not_used2: u3,
    id: u6,
    not_used3: u2,
}

static FIFO_PARAM_COUNTS: [u8; 99] = [
    1, 0, 1, 1, 1, 0, 16, 12, 16, 12, 9, 3, 3, 0, 0, 0, // 0x10-0x1F
    1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, // 0x20-0x2F
    1, 1, 1, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x30-0x3F
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x40-0x4F
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x50-0x5F
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x60-0x6F
    3, 2, 1, // 0x70-0x72
];

static CYCLE_COUNTS: [u16; 99] = [
    1, 17, 36, 17, 36, 19, 34, 30, 35, 31, 28, 22, 22, 0, 0, 0, // 0x10-0x1F
    1, 9, 1, 9, 8, 8, 8, 8, 8, 1, 1, 1, 0, 0, 0, 0, // 0x20-0x2F
    4, 4, 6, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x30-0x3F
    1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x40-0x4F
    392, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x50-0x5F
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x60-0x6F
    103, 9, 5, // 0x70-0x72
];

static FUNC_LUT: [fn(&mut Gpu3DRegisters, params: &[u32; 32]); 99] = [
    Gpu3DRegisters::exe_mtx_mode,
    Gpu3DRegisters::exe_mtx_push,
    Gpu3DRegisters::exe_mtx_pop,
    Gpu3DRegisters::exe_mtx_store,
    Gpu3DRegisters::exe_mtx_restore,
    Gpu3DRegisters::exe_mtx_identity,
    Gpu3DRegisters::exe_mtx_load44,
    Gpu3DRegisters::exe_mtx_load43,
    Gpu3DRegisters::exe_mtx_mult44,
    Gpu3DRegisters::exe_mtx_mult43,
    Gpu3DRegisters::exe_mtx_mult33,
    Gpu3DRegisters::exe_mtx_scale,
    Gpu3DRegisters::exe_mtx_trans,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_color,
    Gpu3DRegisters::exe_normal,
    Gpu3DRegisters::exe_tex_coord,
    Gpu3DRegisters::exe_vtx16,
    Gpu3DRegisters::exe_vtx10,
    Gpu3DRegisters::exe_vtx_x_y,
    Gpu3DRegisters::exe_vtx_x_z,
    Gpu3DRegisters::exe_vtx_y_z,
    Gpu3DRegisters::exe_vtx_diff,
    Gpu3DRegisters::exe_polygon_attr,
    Gpu3DRegisters::exe_tex_image_param,
    Gpu3DRegisters::exe_pltt_base,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_dif_amb,
    Gpu3DRegisters::exe_spe_emi,
    Gpu3DRegisters::exe_light_vector,
    Gpu3DRegisters::exe_light_color,
    Gpu3DRegisters::exe_shininess,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_begin_vtxs,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_swap_buffers,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_viewport,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_empty,
    Gpu3DRegisters::exe_box_test,
    Gpu3DRegisters::exe_pos_test,
    Gpu3DRegisters::exe_vec_test,
];

#[derive(Copy, Clone, Default)]
struct Entry {
    cmd: u8,
    param_len: u8,
    param: u32,
}

impl Entry {
    fn new(cmd: u8, param: u32) -> Self {
        Self::new_with_len(cmd, unsafe { *FIFO_PARAM_COUNTS.get_unchecked(cmd as usize - 0x10) }, param)
    }

    fn new_with_len(cmd: u8, param_len: u8, param: u32) -> Self {
        Entry { cmd, param_len, param }
    }
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
enum TextureCoordTransMode {
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

#[derive(Copy, Clone, Default)]
pub struct Vertex {
    pub coords: Vectori32<4>,
    pub tex_coords: Vectori16<2>,
    pub color: u32,
    pub viewport: Vectoru8<4>,
}

fn intersect(v1: &Vectorf32<4>, v2: &Vectorf32<4>, val1: f32, val2: f32) -> Vectorf32<4> {
    let d1 = val1 + v1[3];
    let d2 = val2 + v2[3];
    if (d2 - d1).abs() < f32::EPSILON {
        return *v1;
    }

    let mut vertex: Vectorf32<4> = unsafe { MaybeUninit::uninit().assume_init() };
    let dist_inverse = -d1 as f64 / (d2 - d1) as f64;
    vertex[0] = v1[0] + (((v2[0] - v1[0]) as f64 * dist_inverse) as f32);
    vertex[1] = v1[1] + (((v2[1] - v1[1]) as f64 * dist_inverse) as f32);
    vertex[2] = v1[2] + (((v2[2] - v1[2]) as f64 * dist_inverse) as f32);
    vertex[3] = v1[3] + (((v2[3] - v1[3]) as f64 * dist_inverse) as f32);
    vertex
}

fn clip_polygon(unclipped: &[Vectori32<4>; 4], clipped: &mut [Vectorf32<4>; 10], size: &mut usize) -> bool {
    let mut clip = false;

    let mut vertices = [Vectorf32::<4>::default(); 10];
    unsafe {
        let vertices0 = vld1q_s32(unclipped[0].as_ref().as_ptr());
        let vertices1 = vld1q_s32(unclipped[1].as_ref().as_ptr());
        let vertices2 = vld1q_s32(unclipped[2].as_ref().as_ptr());
        let vertices3 = vld1q_s32(unclipped[3].as_ref().as_ptr());

        let vertices0 = vcvtq_f32_s32(vertices0);
        let vertices1 = vcvtq_f32_s32(vertices1);
        let vertices2 = vcvtq_f32_s32(vertices2);
        let vertices3 = vcvtq_f32_s32(vertices3);

        let vertices0 = vmulq_n_f32(vertices0, 1f32 / 4096f32);
        let vertices1 = vmulq_n_f32(vertices1, 1f32 / 4096f32);
        let vertices2 = vmulq_n_f32(vertices2, 1f32 / 4096f32);
        let vertices3 = vmulq_n_f32(vertices3, 1f32 / 4096f32);

        vst1q_f32(vertices[0].as_mut().as_mut_ptr(), vertices0);
        vst1q_f32(vertices[1].as_mut().as_mut_ptr(), vertices1);
        vst1q_f32(vertices[2].as_mut().as_mut_ptr(), vertices2);
        vst1q_f32(vertices[3].as_mut().as_mut_ptr(), vertices3);
    }

    for i in 0..6 {
        let old_size = *size;
        *size = 0;

        for j in 0..old_size {
            let current = unsafe { vertices.get_unchecked(j) };
            let previous = unsafe { vertices.get_unchecked(if unlikely(j == 0) { old_size - 1 } else { j - 1 }) };

            let (mut current_val, mut previous_val) = (current[i >> 1], previous[i >> 1]);
            if i & 1 == 1 {
                current_val = -current_val;
                previous_val = -previous_val;
            }

            if current_val >= -current[3] {
                if previous_val < -previous[3] {
                    unsafe { *clipped.get_unchecked_mut(*size) = intersect(current, previous, current_val, previous_val) };
                    *size += 1;
                    return true;
                }

                unsafe { *clipped.get_unchecked_mut(*size) = *current };
                *size += 1;
            } else if previous_val >= -previous[3] {
                unsafe { *clipped.get_unchecked_mut(*size) = intersect(current, previous, current_val, previous_val) };
                *size += 1;
                return true;
            }
        }

        unsafe { vertices.as_mut_ptr().copy_from(clipped.as_ptr(), *size) };
    }

    clip
}

#[derive(Copy, Clone, Default)]
pub struct Polygon {
    pub vertices_index: u16,
    pub size: u8,
    pub crossed: bool,
    clockwise: bool,

    mode: PolygonMode,
    trans_new_depth: bool,
    depth_test_equal: bool,
    fog: bool,
    alpha: u8,
    id: u8,

    pub tex_image_param: TexImageParam,
    pub palette_addr: u16,
    transparent0: bool,

    pub w_buffer: bool,
    pub w_shift: i32,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
enum PolygonType {
    #[default]
    SeparateTriangles = 0,
    SeparateQuadliterals = 1,
    TriangleStrips = 2,
    QuadliteralStrips = 3,
}

impl PolygonType {
    fn vertex_count(self) -> u8 {
        match self {
            PolygonType::SeparateTriangles => 3,
            PolygonType::SeparateQuadliterals => 4,
            PolygonType::TriangleStrips => 3,
            PolygonType::QuadliteralStrips => 4,
        }
    }
}

impl From<u8> for PolygonType {
    fn from(value: u8) -> Self {
        debug_assert!(value <= PolygonType::QuadliteralStrips as u8);
        unsafe { mem::transmute(value) }
    }
}

#[derive(Default)]
#[repr(u8)]
enum MtxMode {
    #[default]
    Projection = 0,
    ModelView = 1,
    ModelViewVec = 2,
    Texture,
}

impl From<u8> for MtxMode {
    fn from(value: u8) -> Self {
        debug_assert!(value <= MtxMode::Texture as u8);
        unsafe { mem::transmute(value) }
    }
}

#[derive(Default)]
struct Matrices {
    proj: Matrix,
    proj_stack: Matrix,
    model: Matrix,
    model_stack: [Matrix; 32],
    vec: Matrix,
    vec_stack: [Matrix; 32],
    tex: Matrix,
    tex_stack: Matrix,
    clip: Matrix,
}

#[derive(Default)]
pub struct Vertices {
    ins: HeapMem<Vertex, VERTEX_LIMIT>,
    pub outs: HeapMem<Vertex, VERTEX_LIMIT>,
    count_in: u16,
    pub count_out: u16,
    process_count: u16,
}

#[derive(Default)]
pub struct Polygons {
    ins: HeapMem<Polygon, POLYGON_LIMIT>,
    pub outs: HeapMem<Polygon, POLYGON_LIMIT>,
    count_in: u16,
    pub count_out: u16,
}

#[derive(Default)]
pub struct Gpu3DRegisters {
    cmd_fifo: FixedFifo<Entry, 512>,
    cmd_fifo_param_count: u8,

    last_total_cycles: u64,
    pub flushed: bool,

    mtx_mode: MtxMode,
    clip_dirty: bool,

    matrices: Matrices,
    pub vertices: Vertices,
    pub polygons: Polygons,

    saved_vertex: Vertex,
    saved_polygon: Polygon,
    s: i16,
    t: i16,

    vertex_count: u16,
    clockwise: bool,
    polygon_type: PolygonType,
    texture_coord_mode: TextureCoordTransMode,

    polygon_attr: PolygonAttr,
    enabled_lights: u8,
    render_back: bool,
    render_front: bool,

    diffuse_color: u32,
    ambient_color: u32,
    specular_color: u32,
    emission_color: u32,
    shininess_enabled: bool,
    light_vectors: [Vectori32<3>; 4],
    half_vectors: [Vectori32<3>; 4],
    light_colors: [u32; 4],
    shininess: [u8; 4],

    viewport: Vectoru8<4>,
    viewport_next: Vectoru8<4>,

    pub gx_stat: GxStat,
    gx_fifo: u32,
    pos_result: Vectori32<4>,
    vec_result: Vectori16<3>,

    vtx_begin: bool,
}

impl Gpu3DRegisters {
    fn is_cmd_fifo_full(&self) -> bool {
        self.cmd_fifo.len() >= 260
    }

    fn is_cmd_fifo_half_full(&self) -> bool {
        self.cmd_fifo.len() >= 132
    }

    fn is_cmd_fifo_empty(&self) -> bool {
        self.cmd_fifo.len() <= 4
    }

    fn get_cmd_fifo_len(&self) -> usize {
        max(self.cmd_fifo.len() as isize - 4, 0) as usize
    }

    pub fn run_cmds(&mut self, total_cycles: u64, emu: &mut Emu) {
        if self.cmd_fifo.is_empty() || !self.gx_stat.geometry_busy() || self.flushed {
            self.last_total_cycles = total_cycles;
            return;
        }

        let cycle_diff = (total_cycles - self.last_total_cycles) as u32;
        self.last_total_cycles = total_cycles;
        let mut executed_cycles = 0;

        let mut params: [u32; 32] = unsafe { MaybeUninit::uninit().assume_init() };

        while !self.cmd_fifo.is_empty() && executed_cycles < cycle_diff {
            let entry = *self.cmd_fifo.front();
            let param_count = entry.param_len;

            if unlikely(param_count as usize > self.cmd_fifo.len()) {
                break;
            }

            unsafe { *params.get_unchecked_mut(0) = entry.param };
            self.cmd_fifo.pop_front();

            for i in 1..param_count {
                unsafe { *params.get_unchecked_mut(i as usize) = self.cmd_fifo.front().param };
                self.cmd_fifo.pop_front();
            }

            let func = unsafe { FUNC_LUT.get_unchecked(entry.cmd as usize - 0x10) };
            func(self, &params);

            if unlikely(self.flushed) {
                break;
            }
            executed_cycles += unsafe { *CYCLE_COUNTS.get_unchecked(entry.cmd as usize - 0x10) as u32 };
        }

        self.gx_stat.set_num_entries_cmd_fifo(u9::new(self.get_cmd_fifo_len() as u16));
        self.gx_stat.set_cmd_fifo_empty(self.is_cmd_fifo_empty());
        self.gx_stat.set_geometry_busy(!self.cmd_fifo.is_empty());

        if !self.gx_stat.cmd_fifo_less_half_full() && !self.is_cmd_fifo_half_full() {
            self.gx_stat.set_cmd_fifo_less_half_full(true);
            io_dma!(emu, ARM9).trigger_all(DmaTransferMode::GeometryCmdFifo, get_cm_mut!(emu));
        }

        match u8::from(self.gx_stat.cmd_fifo_irq()) {
            0 | 3 => {}
            1 => {
                if self.gx_stat.cmd_fifo_less_half_full() {
                    get_cpu_regs_mut!(emu, ARM9).send_interrupt(InterruptFlag::GeometryCmdFifo, emu);
                }
            }
            2 => {
                if self.gx_stat.cmd_fifo_empty() {
                    get_cpu_regs_mut!(emu, ARM9).send_interrupt(InterruptFlag::GeometryCmdFifo, emu);
                }
            }
            _ => unsafe { unreachable_unchecked() },
        }

        if !self.is_cmd_fifo_full() {
            get_cpu_regs_mut!(emu, ARM9).unhalt(1);
        }
    }

    fn exe_empty(&mut self, _: &[u32; 32]) {}

    fn exe_mtx_mode(&mut self, params: &[u32; 32]) {
        self.mtx_mode = MtxMode::from((params[0] & 0x3) as u8);
    }

    fn exe_mtx_push(&mut self, _: &[u32; 32]) {
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
                    self.matrices.model_stack[ptr as usize] = self.matrices.model;
                    self.matrices.vec_stack[ptr as usize] = self.matrices.vec;
                    self.gx_stat.set_pos_vec_mtx_stack_lvl(u5::new(ptr + 1));
                }
            }
            MtxMode::Texture => self.matrices.tex_stack = self.matrices.tex,
        }
    }

    fn exe_mtx_pop(&mut self, params: &[u32; 32]) {
        match self.mtx_mode {
            MtxMode::Projection => {
                if u8::from(self.gx_stat.proj_mtx_stack_lvl()) == 1 {
                    self.matrices.proj = self.matrices.proj_stack;
                    self.gx_stat.set_proj_mtx_stack_lvl(u1::new(0));
                    self.clip_dirty = true;
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
                    self.matrices.model = self.matrices.model_stack[ptr as usize];
                    self.matrices.vec = self.matrices.vec_stack[ptr as usize];
                    self.clip_dirty = true;
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

                self.matrices.model_stack[addr as usize] = self.matrices.model;
                self.matrices.vec_stack[addr as usize] = self.matrices.vec;
            }
            MtxMode::Texture => self.matrices.tex_stack = self.matrices.tex,
        }
    }

    fn exe_mtx_restore(&mut self, params: &[u32; 32]) {
        match self.mtx_mode {
            MtxMode::Projection => {
                self.matrices.proj = self.matrices.proj_stack;
                self.clip_dirty = true;
            }
            MtxMode::ModelView | MtxMode::ModelViewVec => {
                let addr = params[0] & 0x1F;

                if addr == 31 {
                    self.gx_stat.set_mtx_stack_overflow_underflow_err(true);
                }

                self.matrices.model = self.matrices.model_stack[addr as usize];
                self.matrices.vec = self.matrices.vec_stack[addr as usize];
                self.clip_dirty = true;
            }
            MtxMode::Texture => self.matrices.tex = self.matrices.tex_stack,
        }
    }

    fn exe_mtx_identity(&mut self, _: &[u32; 32]) {
        match self.mtx_mode {
            MtxMode::Projection => self.matrices.proj = Matrix::default(),
            MtxMode::ModelView => self.matrices.model = Matrix::default(),
            MtxMode::ModelViewVec => {
                self.matrices.model = Matrix::default();
                self.matrices.vec = Matrix::default();
            }
            MtxMode::Texture => self.matrices.tex = Matrix::default(),
        }
    }

    fn mtx_load(&mut self, mtx: Matrix) {
        match self.mtx_mode {
            MtxMode::Projection => {
                self.matrices.proj = mtx;
                self.clip_dirty = true;
            }
            MtxMode::ModelView => {
                self.matrices.model = mtx;
                self.clip_dirty = true;
            }
            MtxMode::ModelViewVec => {
                self.matrices.model = mtx;
                self.matrices.vec = mtx;
                self.clip_dirty = true;
            }
            MtxMode::Texture => self.matrices.tex = mtx,
        }
    }

    fn exe_mtx_load44(&mut self, params: &[u32; 32]) {
        let params: &[u32; 16] = unsafe { mem::transmute(params) };
        self.mtx_load(unsafe { mem::transmute(*params) });
    }

    fn exe_mtx_load43(&mut self, params: &[u32; 32]) {
        let mut mtx = Matrix::default();
        for i in 0..4 {
            mtx.as_mut()[i * 4..i * 4 + 3].copy_from_slice(unsafe { mem::transmute(&params[i * 3..i * 3 + 3]) });
        }
        self.mtx_load(mtx);
    }

    fn mtx_mult(&mut self, mtx: Matrix) {
        match self.mtx_mode {
            MtxMode::Projection => {
                self.matrices.proj = mtx * &self.matrices.proj;
                self.clip_dirty = true;
            }
            MtxMode::ModelView => {
                self.matrices.model = mtx * &self.matrices.model;
                self.clip_dirty = true;
            }
            MtxMode::ModelViewVec => {
                self.matrices.model = mtx * &self.matrices.model;
                self.matrices.vec = mtx * &self.matrices.vec;
                self.clip_dirty = true;
            }
            MtxMode::Texture => {
                self.matrices.tex = mtx * &self.matrices.tex;
            }
        }
    }

    fn exe_mtx_mult44(&mut self, params: &[u32; 32]) {
        let params: &[u32; 16] = unsafe { mem::transmute(params) };
        self.mtx_mult(unsafe { mem::transmute(*params) });
    }

    fn exe_mtx_mult43(&mut self, params: &[u32; 32]) {
        let mut mtx = Matrix::default();
        for i in 0..4 {
            mtx.as_mut()[i * 4..i * 4 + 3].copy_from_slice(unsafe { mem::transmute(&params[i * 3..i * 3 + 3]) });
        }
        self.mtx_mult(mtx);
    }

    fn exe_mtx_mult33(&mut self, params: &[u32; 32]) {
        let mut mtx = Matrix::default();
        for i in 0..3 {
            mtx.as_mut()[i * 4..i * 4 + 3].copy_from_slice(unsafe { mem::transmute(&params[i * 3..i * 3 + 3]) });
        }
        self.mtx_mult(mtx);
    }

    fn exe_mtx_scale(&mut self, params: &[u32; 32]) {
        let mut mtx = Matrix::default();
        for i in 0..3 {
            mtx[i * 5] = params[i] as i32;
        }
        self.mtx_mult(mtx);
    }

    fn exe_mtx_trans(&mut self, params: &[u32; 32]) {
        let mut mtx = Matrix::default();
        let params: &[i32; 3] = unsafe { mem::transmute(params) };
        mtx.as_mut()[12..15].copy_from_slice(params);
        self.mtx_mult(mtx);
    }

    fn exe_color(&mut self, params: &[u32; 32]) {
        self.saved_vertex.color = rgb5_to_rgb6(params[0]);
    }

    fn exe_normal(&mut self, params: &[u32; 32]) {
        let normal_vector_param = NormalVector::from(params[0]);
        let normal_vector = Vectori32::<3>::new([
            (((u16::from(normal_vector_param.x()) << 6) as i16) >> 3) as i32,
            (((u16::from(normal_vector_param.y()) << 6) as i16) >> 3) as i32,
            (((u16::from(normal_vector_param.z()) << 6) as i16) >> 3) as i32,
        ]);

        // if self.texture_coord_mode == TextureCoordTransMode::Normal {
        //     let mut vector = Vectori32::<4>::from(normal_vector);
        //     vector[3] = 1 << 12;
        //
        //     let mut matrix = self.matrices.tex;
        //     matrix[12] = (self.s as i32) << 12;
        //     matrix[13] = (self.t as i32) << 12;
        //
        //     vector *= &matrix;
        //
        //     self.saved_vertex.tex_coords[0] = (vector[0] >> 12) as i16;
        //     self.saved_vertex.tex_coords[1] = (vector[1] >> 12) as i16;
        // }

        self.saved_vertex.color = self.emission_color;

        return;

        if self.enabled_lights == 0 {
            return;
        }

        unsafe {
            let mtx_vec_0 = vld1q_s32(self.matrices.vec.as_ref().as_ptr());
            let mtx_vec_1 = vld1q_s32(self.matrices.vec.as_ref().as_ptr().add(4));
            let mtx_vec_2 = vld1q_s32(self.matrices.vec.as_ref().as_ptr().add(8));

            let lower_result = vmull_n_s32(vget_low_s32(mtx_vec_0), normal_vector[0]);
            let lower_result = vmlal_n_s32(lower_result, vget_low_s32(mtx_vec_1), normal_vector[1]);
            let lower_result = vmlal_n_s32(lower_result, vget_low_s32(mtx_vec_2), normal_vector[2]);

            let higher_result = vmull_n_s32(vget_high_s32(mtx_vec_0), normal_vector[0]);
            let higher_result = vmlal_n_s32(higher_result, vget_high_s32(mtx_vec_1), normal_vector[1]);
            let higher_result = vmlal_n_s32(higher_result, vget_high_s32(mtx_vec_2), normal_vector[2]);

            let lower_result = vshrn_n_s64::<12>(lower_result);
            let higher_result = vshrn_n_s64::<12>(higher_result);

            let normal_vector = vuzp_s32(lower_result, higher_result);

            for i in 0..4 {
                if self.enabled_lights & (1 << i) == 0 {
                    continue;
                }

                let light_vector = vld1q_s32(self.light_vectors[i].as_ref().as_ptr());
                let diffuse_level = -vdot_vec3(light_vector, mem::transmute(normal_vector));
                let diffuse_level = diffuse_level.clamp(0, 1 << 12) as u32;

                let half_vector = vld1q_s32(self.half_vectors[i].as_ref().as_ptr());
                let shininess_level = -vdot_vec3(half_vector, mem::transmute(normal_vector));
                let shininess_level = shininess_level.clamp(0, 1 << 12) as u32;
                let mut shininess_level = (shininess_level * shininess_level) >> 12;

                if self.shininess_enabled {
                    shininess_level = (self.shininess[((shininess_level >> 5) as usize) & 3] << 4) as u32;
                }

                let mut r = self.saved_vertex.color & 0x3F;
                let mut g = (self.saved_vertex.color >> 6) & 0x3F;
                let mut b = (self.saved_vertex.color >> 12) & 0x3F;

                r += ((self.specular_color & 0x3F) * (self.light_colors[i] & 0x3F) * shininess_level) >> 18;
                g += (((self.specular_color >> 6) & 0x3F) * ((self.light_colors[i] >> 6) & 0x3F) * shininess_level) >> 18;
                b += (((self.specular_color >> 12) & 0x3F) * ((self.light_colors[i] >> 12) & 0x3F) * shininess_level) >> 18;

                r += ((self.diffuse_color & 0x3F) * (self.light_colors[i] & 0x3F) * diffuse_level) >> 18;
                g += (((self.diffuse_color >> 6) & 0x3F) * ((self.light_colors[i] >> 6) & 0x3F) * diffuse_level) >> 18;
                b += (((self.diffuse_color >> 12) & 0x3F) * ((self.light_colors[i] >> 12) & 0x3F) * diffuse_level) >> 18;

                r += ((self.ambient_color & 0x3F) * (self.light_colors[i] & 0x3F)) >> 6;
                g += (((self.ambient_color >> 6) & 0x3F) * ((self.light_colors[i] >> 6) & 0x3F)) >> 6;
                b += (((self.ambient_color >> 12) & 0x3F) * ((self.light_colors[i] >> 12) & 0x3F)) >> 6;

                let r = r.clamp(0, 0x3F);
                let g = g.clamp(0, 0x3F);
                let b = b.clamp(0, 0x3F);

                self.saved_vertex.color = (b << 12) | (g << 6) | r;
            }
        }
    }

    fn exe_tex_coord(&mut self, params: &[u32; 32]) {
        let tex_coord = TexCoord::from(params[0]);
        self.s = tex_coord.s() as i16;
        self.t = tex_coord.t() as i16;

        if self.texture_coord_mode == TextureCoordTransMode::TexCoord {
            unsafe {
                let mtx_tex_0 = vld1q_s32(self.matrices.tex.as_ref().as_ptr());
                let mtx_tex_1 = vld1q_s32(self.matrices.tex.as_ref().as_ptr().add(4));
                let mtx_tex_2 = vld1q_s32(self.matrices.tex.as_ref().as_ptr().add(8));
                let mtx_tex_3 = vld1q_s32(self.matrices.tex.as_ref().as_ptr().add(12));

                let lower_result = vmull_n_s32(vget_low_s32(mtx_tex_0), (self.s as i32) << 8);
                let lower_result = vmlal_n_s32(lower_result, vget_low_s32(mtx_tex_1), (self.t as i32) << 8);
                let lower_result = vmlal_n_s32(lower_result, vget_low_s32(mtx_tex_2), 1 << 8);
                let lower_result = vmlal_n_s32(lower_result, vget_low_s32(mtx_tex_3), 1 << 8);

                let lower_result = vshrq_n_s64::<20>(lower_result);
                self.saved_vertex.tex_coords[0] = vgetq_lane_s16::<0>(mem::transmute(lower_result));
                self.saved_vertex.tex_coords[1] = vgetq_lane_s16::<4>(mem::transmute(lower_result));
            }
        } else {
            self.saved_vertex.tex_coords[0] = self.s;
            self.saved_vertex.tex_coords[1] = self.t;
        }
    }

    fn exe_vtx16(&mut self, params: &[u32; 32]) {
        self.saved_vertex.coords[0] = params[0] as i16 as i32;
        self.saved_vertex.coords[1] = (params[0] >> 16) as i16 as i32;
        self.saved_vertex.coords[2] = params[1] as i16 as i32;

        self.add_vertex();
    }

    fn exe_vtx10(&mut self, params: &[u32; 32]) {
        self.saved_vertex.coords[0] = ((params[0] & 0x3FF) << 6) as i16 as i32;
        self.saved_vertex.coords[1] = ((params[0] & 0xFFC00) >> 4) as i16 as i32;
        self.saved_vertex.coords[2] = ((params[0] & 0x3FF00000) >> 14) as i16 as i32;

        self.add_vertex();
    }

    fn exe_vtx_x_y(&mut self, params: &[u32; 32]) {
        self.saved_vertex.coords[0] = params[0] as i16 as i32;
        self.saved_vertex.coords[1] = (params[0] >> 16) as i16 as i32;

        self.add_vertex();
    }

    fn exe_vtx_x_z(&mut self, params: &[u32; 32]) {
        self.saved_vertex.coords[0] = params[0] as i16 as i32;
        self.saved_vertex.coords[2] = (params[0] >> 16) as i16 as i32;

        self.add_vertex();
    }

    fn exe_vtx_y_z(&mut self, params: &[u32; 32]) {
        self.saved_vertex.coords[1] = params[0] as i16 as i32;
        self.saved_vertex.coords[2] = (params[0] >> 16) as i16 as i32;

        self.add_vertex();
    }

    fn exe_vtx_diff(&mut self, params: &[u32; 32]) {
        self.saved_vertex.coords[0] += (((params[0] & 0x3FF) << 6) as i16 as i32) >> 6;
        self.saved_vertex.coords[1] += (((params[0] & 0xFFC00) >> 4) as i16 as i32) >> 6;
        self.saved_vertex.coords[2] += (((params[0] & 0x3FF00000) >> 14) as i16 as i32) >> 6;

        self.add_vertex();
    }

    fn exe_polygon_attr(&mut self, params: &[u32; 32]) {
        self.polygon_attr = params[0].into();
    }

    fn exe_tex_image_param(&mut self, params: &[u32; 32]) {
        let Self {
            saved_polygon, texture_coord_mode, ..
        } = self;
        saved_polygon.tex_image_param = TexImageParam::from(params[0]);
        *texture_coord_mode = TextureCoordTransMode::from(u8::from(saved_polygon.tex_image_param.coord_trans_mode()));
    }

    fn exe_pltt_base(&mut self, params: &[u32; 32]) {
        self.saved_polygon.palette_addr = (params[0] & 0x1FFF) as u16;
    }

    fn exe_dif_amb(&mut self, params: &[u32; 32]) {
        let material_color0 = MaterialColor0::from(params[0]);
        self.diffuse_color = rgb5_to_rgb6(u32::from(material_color0.dif()));
        self.ambient_color = rgb5_to_rgb6(u32::from(material_color0.amb()));

        if material_color0.set_vertex_color() {
            self.saved_vertex.color = self.diffuse_color;
        }
    }

    fn exe_spe_emi(&mut self, params: &[u32; 32]) {
        let material_color1 = MaterialColor1::from(params[0]);
        self.specular_color = rgb5_to_rgb6(u32::from(material_color1.spe()));
        self.emission_color = rgb5_to_rgb6(u32::from(material_color1.em()));
        self.shininess_enabled = material_color1.set_shininess();
    }

    fn exe_light_vector(&mut self, params: &[u32; 32]) {
        let light_vector = LightVector::from(params[0]);
        let num = u8::from(light_vector.num()) as usize;
        // shift left for signedness
        // shift right to convert 9 to 12 fractional bits
        self.light_vectors[num][0] = (((u16::from(light_vector.x()) << 6) as i16) >> 3) as i32;
        self.light_vectors[num][1] = (((u16::from(light_vector.y()) << 6) as i16) >> 3) as i32;
        self.light_vectors[num][2] = (((u16::from(light_vector.z()) << 6) as i16) >> 3) as i32;

        self.light_vectors[num] *= &self.matrices.vec;

        self.half_vectors[num][0] = self.light_vectors[num][0] >> 1;
        self.half_vectors[num][1] = self.light_vectors[num][1] >> 1;
        self.half_vectors[num][2] = (self.light_vectors[num][2] - (1 << 12)) >> 1;
    }

    fn exe_light_color(&mut self, params: &[u32; 32]) {
        let light_color = LightColor::from(params[0]);
        self.light_colors[u8::from(light_color.num()) as usize] = rgb5_to_rgb6(u32::from(light_color.color()));
    }

    fn exe_shininess(&mut self, params: &[u32; 32]) {
        let shininess = Shininess::from(params[0]);
        self.shininess[0] = shininess.shininess0();
        self.shininess[1] = shininess.shininess1();
        self.shininess[2] = shininess.shininess2();
        self.shininess[3] = shininess.shininess3();
    }

    fn exe_begin_vtxs(&mut self, params: &[u32; 32]) {
        if self.vertex_count < self.polygon_type.vertex_count() as u16 {
            self.vertices.count_in -= self.vertex_count;
        }

        self.process_vertices();
        self.polygon_type = PolygonType::from((params[0] & 0x3) as u8);
        self.vertex_count = 0;
        self.clockwise = false;

        self.enabled_lights = u8::from(self.polygon_attr.enable_lights());
        self.saved_polygon.mode = PolygonMode::from(u8::from(self.polygon_attr.mode()));
        self.render_back = self.polygon_attr.render_back();
        self.render_front = self.polygon_attr.render_front();
        self.saved_polygon.trans_new_depth = self.polygon_attr.trans_new_depth();
        self.saved_polygon.depth_test_equal = self.polygon_attr.depth_test_equal();
        self.saved_polygon.fog = self.polygon_attr.fog();
        self.saved_polygon.alpha = u8::from(self.polygon_attr.alpha());
        if self.saved_polygon.alpha > 0 {
            self.saved_polygon.alpha += 1;
        }
        self.saved_polygon.id = u8::from(self.polygon_attr.id());
    }

    fn exe_swap_buffers(&mut self, params: &[u32; 32]) {
        self.saved_polygon.w_buffer = (params[0] & 0x2) != 0;
        self.flushed = true;
    }

    fn exe_viewport(&mut self, params: &[u32; 32]) {
        let viewport = Viewport::from(params[0]);
        self.viewport_next[0] = viewport.x1();
        self.viewport_next[1] = viewport.y1();
        self.viewport_next[2] = viewport.x2();
        self.viewport_next[3] = viewport.y2();
    }

    fn exe_box_test(&mut self, params: &[u32; 32]) {
        // let mut box_test_coords = [
        //     params[0] as i16,
        //     (params[0] >> 16) as i16,
        //     params[1] as i16,
        //     (params[1] >> 16) as i16,
        //     params[2] as i16,
        //     (params[2] >> 16) as i16,
        // ];
        //
        // box_test_coords[3] += box_test_coords[0];
        // box_test_coords[4] += box_test_coords[1];
        // box_test_coords[5] += box_test_coords[2];
        //
        // const INDICES: [u8; 8 * 3] = [0, 1, 2, 3, 1, 2, 0, 4, 2, 0, 1, 5, 3, 4, 2, 3, 1, 5, 0, 4, 5, 3, 4, 5];
        //
        // if self.clip_dirty {
        //     self.matrices.clip = self.matrices.model * &self.matrices.proj;
        //     self.clip_dirty = false;
        // }
        //
        // let mut vertices = [Vectori32::<4>::default(); 8];
        // for i in 0..8 {
        //     vertices[i][0] = box_test_coords[INDICES[i * 3 + 0] as usize] as i32;
        //     vertices[i][1] = box_test_coords[INDICES[i * 3 + 1] as usize] as i32;
        //     vertices[i][2] = box_test_coords[INDICES[i * 3 + 2] as usize] as i32;
        //     vertices[i][3] = 1 << 12;
        //     vertices[i] *= &self.matrices.clip;
        // }
        //
        // let faces = [
        //     [vertices[0], vertices[1], vertices[4], vertices[2]],
        //     [vertices[3], vertices[5], vertices[7], vertices[6]],
        //     [vertices[3], vertices[5], vertices[1], vertices[0]],
        //     [vertices[6], vertices[7], vertices[4], vertices[2]],
        //     [vertices[0], vertices[3], vertices[6], vertices[2]],
        //     [vertices[1], vertices[5], vertices[7], vertices[4]],
        // ];
        //
        // for i in 0..6 {
        //     let mut size = 4;
        //     let mut clipped = [Vectorf32::<4>::default(); 10];
        //
        //     clip_polygon(&faces[i], &mut clipped, &mut size);
        //
        //     if size > 0 {
        //         self.gx_stat.set_box_test_result(true);
        //         return;
        //     }
        // }
        //
        // self.gx_stat.set_box_test_result(false);
        self.gx_stat.set_box_test_result(true);
    }

    fn exe_pos_test(&mut self, params: &[u32; 32]) {
        self.saved_vertex.coords[0] = params[0] as i16 as i32;
        self.saved_vertex.coords[1] = (params[0] >> 16) as i16 as i32;
        self.saved_vertex.coords[2] = params[1] as i16 as i32;
        self.saved_vertex.coords[3] = 1 << 12;

        if self.clip_dirty {
            self.matrices.clip = self.matrices.model * &self.matrices.proj;
            self.clip_dirty = false;
        }

        self.pos_result = self.saved_vertex.coords * &self.matrices.clip;
    }

    fn exe_vec_test(&mut self, params: &[u32; 32]) {
        let mut vector = Vectori32::<3>::new([
            (((params[0] & 0x000003FF) << 6) as i16 as i32) >> 3,
            (((params[0] & 0x000FFC00) >> 4) as i16 as i32) >> 3,
            (((params[0] & 0x3FF00000) >> 14) as i16 as i32) >> 3,
        ]);

        vector *= &self.matrices.vec;
        self.vec_result[0] = ((vector[0] << 3) as i16) >> 3;
        self.vec_result[1] = ((vector[1] << 3) as i16) >> 3;
        self.vec_result[2] = ((vector[2] << 3) as i16) >> 3;
    }

    fn process_vertices(&mut self) {
        for i in self.vertices.process_count..self.vertices.count_in {
            unsafe { self.vertices.ins.get_unchecked_mut(i as usize).viewport = self.viewport };
        }

        self.vertices.process_count = self.vertices.count_in;
        self.viewport = self.viewport_next;
    }

    pub fn swap_buffers(&mut self) {
        self.flushed = false;
        self.process_vertices();
        self.vertices.process_count = 0;

        mem::swap(&mut self.vertices.ins, &mut self.vertices.outs);
        self.vertices.count_out = self.vertices.count_in;
        self.vertices.count_in = 0;
        self.vertex_count = 0;

        mem::swap(&mut self.polygons.ins, &mut self.polygons.outs);
        self.polygons.count_out = self.polygons.count_in;
        self.polygons.count_in = 0;
    }

    fn add_vertex(&mut self) {
        let Self { vertices, matrices, .. } = self;

        if vertices.count_in as usize >= vertices.ins.len() {
            return;
        }

        // if self.texture_coord_mode == TextureCoordTransMode::Vertex {
        //     let mut matrix = matrices.tex;
        //     matrix[12] = (self.s as i32) << 12;
        //     matrix[13] = (self.t as i32) << 12;
        //
        //     let vector = vertices.ins[vertices.count_in as usize].coords * &matrix;
        //
        //     vertices.ins[vertices.count_in as usize].tex_coords[0] = (vector[0] >> 12) as i16;
        //     vertices.ins[vertices.count_in as usize].tex_coords[1] = (vector[1] >> 12) as i16;
        // }

        if self.clip_dirty {
            matrices.clip = matrices.model * &matrices.proj;
            self.clip_dirty = false;
        }

        self.saved_vertex.coords[3] = 1 << 12;

        unsafe {
            let vertex_coords = vld1q_s32(self.saved_vertex.coords.as_ref().as_ptr());
            let mtx_clip_0 = vld1q_s32(matrices.clip.as_ref().as_ptr());
            let mtx_clip_1 = vld1q_s32(matrices.clip.as_ref().as_ptr().add(4));
            let mtx_clip_2 = vld1q_s32(matrices.clip.as_ref().as_ptr().add(8));
            let mtx_clip_3 = vld1q_s32(matrices.clip.as_ref().as_ptr().add(12));

            let lower_result = vmull_n_s32(vget_low_s32(mtx_clip_0), vgetq_lane_s32::<0>(vertex_coords));
            let lower_result = vmlal_n_s32(lower_result, vget_low_s32(mtx_clip_1), vgetq_lane_s32::<1>(vertex_coords));
            let lower_result = vmlal_n_s32(lower_result, vget_low_s32(mtx_clip_2), vgetq_lane_s32::<2>(vertex_coords));
            let lower_result = vmlal_n_s32(lower_result, vget_low_s32(mtx_clip_3), vgetq_lane_s32::<3>(vertex_coords));

            let higher_result = vmull_n_s32(vget_high_s32(mtx_clip_0), vgetq_lane_s32::<0>(vertex_coords));
            let higher_result = vmlal_n_s32(higher_result, vget_high_s32(mtx_clip_1), vgetq_lane_s32::<1>(vertex_coords));
            let higher_result = vmlal_n_s32(higher_result, vget_high_s32(mtx_clip_2), vgetq_lane_s32::<2>(vertex_coords));
            let higher_result = vmlal_n_s32(higher_result, vget_high_s32(mtx_clip_3), vgetq_lane_s32::<3>(vertex_coords));

            let lower_result = vshrq_n_s64::<12>(lower_result);
            let higher_result = vshrq_n_s64::<12>(higher_result);

            let vertex_coords = vuzpq_s32(mem::transmute(lower_result), mem::transmute(higher_result));
            vst1q_s32(vertices.ins[vertices.count_in as usize].coords.as_mut().as_mut_ptr(), vertex_coords.0);
        }

        vertices.ins[vertices.count_in as usize].color = self.saved_vertex.color;
        vertices.ins[vertices.count_in as usize].tex_coords = self.saved_vertex.tex_coords;

        vertices.count_in += 1;
        self.vertex_count += 1;

        if match self.polygon_type {
            PolygonType::SeparateTriangles => self.vertex_count % 3 == 0,
            PolygonType::SeparateQuadliterals => self.vertex_count % 4 == 0,
            PolygonType::TriangleStrips => self.vertex_count >= 3,
            PolygonType::QuadliteralStrips => self.vertex_count >= 4 && self.vertex_count % 2 == 0,
        } {
            self.add_polygon()
        }
    }

    fn add_polygon(&mut self) {
        if self.polygons.count_in as usize >= self.polygons.ins.len() {
            return;
        }

        let size = self.polygon_type.vertex_count();
        self.saved_polygon.size = size;
        self.saved_polygon.vertices_index = self.vertices.count_in - size as u16;

        self.polygons.ins[self.polygons.count_in as usize] = self.saved_polygon;
        self.polygons.ins[self.polygons.count_in as usize].crossed = self.polygon_type == PolygonType::QuadliteralStrips;

        self.polygons.count_in += 1;
    }

    fn queue_entry(&mut self, entry: Entry) {
        self.cmd_fifo.push_back(entry);
    }

    #[inline(never)]
    fn post_queue_entry(&mut self, emu: &mut Emu) {
        self.gx_stat.set_geometry_busy(!self.cmd_fifo.is_empty());

        self.gx_stat.set_num_entries_cmd_fifo(u9::new(self.get_cmd_fifo_len() as u16));
        self.gx_stat.set_cmd_fifo_empty(self.is_cmd_fifo_empty());

        self.gx_stat.set_cmd_fifo_less_half_full(!self.is_cmd_fifo_half_full());

        if unlikely(self.is_cmd_fifo_full()) {
            get_mem_mut!(emu).breakout_imm = true;
            get_cpu_regs_mut!(emu, ARM9).halt(1);
        }
    }

    pub fn get_clip_mtx_result(&mut self, index: usize) -> u32 {
        if self.clip_dirty {
            self.matrices.clip = self.matrices.model * &self.matrices.proj;
            self.clip_dirty = false;
        }
        self.matrices.clip[index] as u32
    }

    pub fn get_vec_mtx_result(&self, index: usize) -> u32 {
        self.matrices.vec[(index / 3) * 4 + index % 3] as u32
    }

    pub fn get_gx_stat(&self) -> u32 {
        u32::from(self.gx_stat)
    }

    pub fn get_ram_count(&self) -> u32 {
        ((self.vertices.count_in as u32) << 16) | (self.polygons.count_in as u32)
    }

    pub fn get_pos_result(&self, index: usize) -> u32 {
        self.pos_result[index] as u32
    }

    pub fn get_vec_result(&self, index: usize) -> u16 {
        self.vec_result[index] as u16
    }

    fn queue_packed_value(&mut self, value: u32) {
        if self.gx_fifo == 0 {
            self.gx_fifo = value;
        } else {
            let mut param_count = self.cmd_fifo_param_count;
            let len = unsafe { *FIFO_PARAM_COUNTS.get_unchecked((self.gx_fifo & 0x7F) as usize - 0x10) };
            self.queue_entry(Entry::new_with_len(self.gx_fifo as u8, len, value));
            param_count += 1;

            if param_count == len {
                self.gx_fifo >>= 8;
                self.cmd_fifo_param_count = 0;
            } else {
                self.cmd_fifo_param_count = param_count;
            }
        }

        for _ in 0..4 - (self.gx_fifo.leading_zeros() >> 3) {
            if unsafe { *FIFO_PARAM_COUNTS.get_unchecked((self.gx_fifo & 0x7F) as usize - 0x10) } != 0 {
                break;
            }
            self.queue_entry(Entry::new_with_len(self.gx_fifo as u8, 0, 0));
            self.gx_fifo >>= 8;
        }
    }

    pub fn set_gx_fifo(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_packed_value(value & mask);
        self.post_queue_entry(emu);
    }

    pub fn set_gx_fifo_multiple(&mut self, values: &[u32], emu: &mut Emu) {
        for &value in values {
            self.queue_packed_value(value);
        }
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_mode(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x10, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_push(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x11, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_pop(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x12, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_store(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x13, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_restore(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x14, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_identity(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x15, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_load44(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x16, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_load43(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x17, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_mult44(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x18, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_mult43(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x19, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_mult33(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x1A, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_scale(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x1B, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_mtx_trans(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x1C, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_color(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x20, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_normal(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x21, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_tex_coord(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x22, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_vtx16(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x23, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_vtx10(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x24, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_vtx_x_y(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x25, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_vtx_x_z(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x26, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_vtx_y_z(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x27, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_vtx_diff(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x28, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_polygon_attr(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x29, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_tex_image_param(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x2A, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_pltt_base(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x2B, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_dif_amb(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x30, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_spe_emi(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x31, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_light_vector(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x32, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_light_color(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x33, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_shininess(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x34, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_begin_vtxs(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x40, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_end_vtxs(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x41, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_swap_buffers(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x50, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_viewport(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x60, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_box_test(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x70, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_pos_test(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x71, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_vec_test(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x72, value & mask));
        self.post_queue_entry(emu);
    }

    pub fn set_gx_stat(&mut self, mut mask: u32, value: u32) {
        if value & (1 << 15) != 0 {
            self.gx_stat = (u32::from(self.gx_stat) & !0xA000).into();
        }

        mask &= 0xC0000000;
        self.gx_stat = ((u32::from(self.gx_stat) & !mask) | (value & mask)).into();
    }
}
