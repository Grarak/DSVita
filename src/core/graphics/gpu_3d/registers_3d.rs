use crate::core::cpu_regs::InterruptFlag;
use crate::core::emu::{get_cm_mut, get_cpu_regs_mut, get_mem_mut, io_dma, Emu};
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_3d::renderer_3d::Gpu3DRendererContent;
use crate::core::memory::dma::DmaTransferMode;
use crate::core::CpuType::ARM9;
use crate::fixed_fifo::FixedFifo;
use crate::logging::debug_println;
use crate::math::{Matrix, Vectori16, Vectori32};
use crate::utils::{rgb5_to_rgb6, HeapMem};
use bilge::prelude::*;
use paste::paste;
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

const FUNC_NAME_LUT: [&str; 99] = [
    "exe_mtx_mode",
    "exe_mtx_push",
    "exe_mtx_pop",
    "exe_mtx_store",
    "exe_mtx_restore",
    "exe_mtx_identity",
    "exe_mtx_load44",
    "exe_mtx_load43",
    "exe_mtx_mult44",
    "exe_mtx_mult43",
    "exe_mtx_mult33",
    "exe_mtx_scale",
    "exe_mtx_trans",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_color",
    "exe_normal",
    "exe_tex_coord",
    "exe_vtx16",
    "exe_vtx10",
    "exe_vtx_x_y",
    "exe_vtx_x_z",
    "exe_vtx_y_z",
    "exe_vtx_diff",
    "exe_polygon_attr",
    "exe_tex_image_param",
    "exe_pltt_base",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_dif_amb",
    "exe_spe_emi",
    "exe_light_vector",
    "exe_light_color",
    "exe_shininess",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_begin_vtxs",
    "exe_end_vtxs",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_swap_buffers",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_viewport",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_empty",
    "exe_box_test",
    "exe_pos_test",
    "exe_vec_test",
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
    coord: Matrix,
    coord_stack: [Matrix; 32],
    dir: Matrix,
    dir_stack: [Matrix; 32],
    tex: Matrix,
    tex_stack: Matrix,
}

#[derive(Copy, Clone, Default)]
pub struct Vertex {
    pub coords: Vectori32<4>,
    pub tex_coords: Vectori16<2>,
    pub color: u32,
    pub tex_coord_trans_mode: TextureCoordTransMode,
    pub tex_matrix_index: u16,
    pub clip_matrix_index: u16,
}

#[derive(Copy, Clone, Default)]
pub struct Polygon {
    pub normal: Vectori16<3>,
    pub attr: PolygonAttr,
    pub tex_image_param: TexImageParam,
    pub palette_addr: u16,
    pub polygon_type: PrimitiveType,
    pub vertices_index: u16,
    pub viewport: Viewport,
}

#[derive(Default)]
pub struct Gpu3DRegisters {
    cmd_fifo: FixedFifo<Entry, 512>,
    cmd_fifo_param_count: u8,

    last_total_cycles: u64,
    pub flushed: bool,

    pub gx_stat: GxStat,
    gx_fifo: u32,

    mtx_mode: MtxMode,
    matrices: Matrices,

    cur_viewport: Viewport,

    vertex_list_primitive_type: PrimitiveType,
    vertex_list_size: u16,

    vertices: HeapMem<Vertex, VERTEX_LIMIT>,
    cur_vtx: Vertex,
    vertices_size: u16,

    polygons: HeapMem<Polygon, POLYGON_LIMIT>,
    cur_polygon: Polygon,
    polygons_size: u16,

    clip_matrix: Matrix,
    clip_mtx_dirty: bool,

    clip_matrices: Vec<Matrix>,
    clip_mtx_push: bool,

    tex_matrices: Vec<Matrix>,
    tex_mtx_push: bool,

    cur_polygon_attr: PolygonAttr,

    material_color0: MaterialColor0,
    material_color1: MaterialColor1,

    pos_result: Vectori32<4>,
    vec_result: Vectori16<3>,

    pub skip: bool,
    pub consume: bool,
}

macro_rules! unpacked_cmd {
    ($name:ident, $cmd:expr) => {
        paste! {
            pub fn [<set _ $name>](&mut self, mask: u32, value: u32, emu: &mut Emu) {
                self.queue_entry(Entry::new($cmd, value & mask));
                self.post_queue_entry(emu);
            }
        }
    };
}

impl Gpu3DRegisters {
    pub fn new() -> Self {
        Gpu3DRegisters {
            clip_mtx_push: true,
            tex_mtx_push: true,
            ..Gpu3DRegisters::default()
        }
    }

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

            debug_println!("gx: {}", unsafe { FUNC_NAME_LUT.get_unchecked(entry.cmd as usize - 0x10) });
            let func = unsafe { FUNC_LUT.get_unchecked(entry.cmd as usize - 0x10) };
            func(self, &params);

            if unlikely(self.flushed) {
                break;
            }
            executed_cycles += 8;
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

    fn get_clip_matrix(&mut self) -> &Matrix {
        if self.clip_mtx_dirty {
            self.clip_mtx_dirty = false;
            self.clip_mtx_push = true;
            self.clip_matrix = self.matrices.coord * &self.matrices.proj;
        }
        &self.clip_matrix
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
                    self.matrices.coord_stack[ptr as usize] = self.matrices.coord;
                    self.matrices.dir_stack[ptr as usize] = self.matrices.dir;
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
            MtxMode::Texture => {
                self.matrices.tex = self.matrices.tex_stack;
                self.tex_mtx_push = true;
            }
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
            MtxMode::Texture => {
                self.matrices.tex = self.matrices.tex_stack;
                self.tex_mtx_push = true;
            }
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
            MtxMode::Texture => {
                self.matrices.tex = Matrix::default();
                self.tex_mtx_push = true;
            }
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
            MtxMode::Texture => {
                self.matrices.tex = mtx;
                self.tex_mtx_push = true;
            }
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
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelView => {
                self.matrices.coord = mtx * &self.matrices.coord;
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelViewVec => {
                self.matrices.coord = mtx * &self.matrices.coord;
                self.matrices.dir = mtx * &self.matrices.dir;
                self.clip_mtx_dirty = true;
            }
            MtxMode::Texture => {
                self.matrices.tex = mtx * &self.matrices.tex;
                self.tex_mtx_push = true;
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
        match self.mtx_mode {
            MtxMode::Projection => {
                self.matrices.proj = mtx * &self.matrices.proj;
                self.clip_mtx_dirty = true;
            }
            MtxMode::ModelView | MtxMode::ModelViewVec => {
                self.matrices.coord = mtx * &self.matrices.coord;
                self.clip_mtx_dirty = true;
            }
            MtxMode::Texture => {
                self.matrices.tex = mtx * &self.matrices.tex;
                self.tex_mtx_push = true;
            }
        }
    }

    fn exe_mtx_trans(&mut self, params: &[u32; 32]) {
        let mut mtx = Matrix::default();
        let params: &[i32; 3] = unsafe { mem::transmute(params) };
        mtx.as_mut()[12..15].copy_from_slice(params);
        self.mtx_mult(mtx);
    }

    fn exe_color(&mut self, params: &[u32; 32]) {
        self.cur_vtx.color = rgb5_to_rgb6(params[0]);
    }

    fn exe_normal(&mut self, params: &[u32; 32]) {
        if self.skip {
            return;
        }

        let normal_vector_param = NormalVector::from(params[0]);
        self.cur_polygon.normal[0] = ((u16::from(normal_vector_param.x()) << 6) as i16) >> 3;
        self.cur_polygon.normal[1] = ((u16::from(normal_vector_param.y()) << 6) as i16) >> 3;
        self.cur_polygon.normal[2] = ((u16::from(normal_vector_param.z()) << 6) as i16) >> 3;
    }

    fn exe_tex_coord(&mut self, params: &[u32; 32]) {
        if self.skip {
            return;
        }

        let tex_coord = TexCoord::from(params[0]);
        self.cur_vtx.tex_coords[0] = tex_coord.s() as i16;
        self.cur_vtx.tex_coords[1] = tex_coord.t() as i16;
        if self.cur_vtx.tex_coord_trans_mode == TextureCoordTransMode::TexCoord && self.tex_mtx_push {
            self.tex_matrices.push(self.matrices.tex);
            self.tex_mtx_push = false;
        }
        self.cur_vtx.tex_matrix_index = (self.tex_matrices.len() as u16).wrapping_sub(1);
    }

    fn exe_vtx16(&mut self, params: &[u32; 32]) {
        if self.skip {
            return;
        }

        self.cur_vtx.coords[0] = params[0] as i16 as i32;
        self.cur_vtx.coords[1] = (params[0] >> 16) as i16 as i32;
        self.cur_vtx.coords[2] = params[1] as i16 as i32;
        self.add_vertex();
    }

    fn exe_vtx10(&mut self, params: &[u32; 32]) {
        if self.skip {
            return;
        }

        self.cur_vtx.coords[0] = ((params[0] & 0x3FF) << 6) as i16 as i32;
        self.cur_vtx.coords[1] = ((params[0] & 0xFFC00) >> 4) as i16 as i32;
        self.cur_vtx.coords[2] = ((params[0] & 0x3FF00000) >> 14) as i16 as i32;
        self.add_vertex();
    }

    fn exe_vtx_x_y(&mut self, params: &[u32; 32]) {
        if self.skip {
            return;
        }

        self.cur_vtx.coords[0] = params[0] as i16 as i32;
        self.cur_vtx.coords[1] = (params[0] >> 16) as i16 as i32;
        self.add_vertex();
    }

    fn exe_vtx_x_z(&mut self, params: &[u32; 32]) {
        if self.skip {
            return;
        }

        self.cur_vtx.coords[0] = params[0] as i16 as i32;
        self.cur_vtx.coords[2] = (params[0] >> 16) as i16 as i32;
        self.add_vertex();
    }

    fn exe_vtx_y_z(&mut self, params: &[u32; 32]) {
        if self.skip {
            return;
        }

        self.cur_vtx.coords[1] = params[0] as i16 as i32;
        self.cur_vtx.coords[2] = (params[0] >> 16) as i16 as i32;
        self.add_vertex();
    }

    fn exe_vtx_diff(&mut self, params: &[u32; 32]) {
        if self.skip {
            return;
        }

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
        self.cur_vtx.tex_coord_trans_mode = TextureCoordTransMode::from(u8::from(self.cur_polygon.tex_image_param.coord_trans_mode()));
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
        if self.skip {
            return;
        }

        if self.vertex_list_size < self.vertex_list_primitive_type.vertex_count() as u16 {
            self.vertices_size -= self.vertex_list_size;
        }

        self.vertex_list_primitive_type = PrimitiveType::from((params[0] & 0x3) as u8);
        self.vertex_list_size = 0;
        self.cur_polygon.attr = self.cur_polygon_attr;
        self.cur_polygon.viewport = self.cur_viewport;
    }

    fn exe_swap_buffers(&mut self, params: &[u32; 32]) {
        self.flushed = true;
    }

    fn exe_viewport(&mut self, params: &[u32; 32]) {
        self.cur_viewport = Viewport::from(params[0]);
    }

    fn exe_box_test(&mut self, _: &[u32; 32]) {
        self.gx_stat.set_box_test_result(true);
    }

    fn exe_pos_test(&mut self, params: &[u32; 32]) {
        self.cur_vtx.coords[0] = params[0] as i16 as i32;
        self.cur_vtx.coords[1] = (params[0] >> 16) as i16 as i32;
        self.cur_vtx.coords[2] = params[1] as i16 as i32;
        self.cur_vtx.coords[3] = 1 << 12;
        self.pos_result = self.cur_vtx.coords * self.get_clip_matrix();
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
    }

    pub fn swap_buffers(&mut self) {
        self.flushed = false;
        if !self.skip {
            self.consume = true;
        }
        self.skip = self.consume;
    }

    pub fn swap_to_renderer(&mut self, content: &mut Gpu3DRendererContent) {
        mem::swap(&mut self.vertices, &mut content.vertices);
        content.vertices_size = self.vertices_size;
        self.vertices_size = 0;
        self.vertex_list_size = 0;

        mem::swap(&mut self.polygons, &mut content.polygons);
        content.polygons_size = self.polygons_size;
        self.polygons_size = 0;

        mem::swap(&mut self.clip_matrices, &mut content.clip_matrices);
        self.clip_matrices.clear();
        self.clip_mtx_push = true;

        mem::swap(&mut self.tex_matrices, &mut content.tex_matrices);
        self.tex_matrices.clear();
        self.tex_mtx_push = true;
    }

    fn add_vertex(&mut self) {
        if self.vertices_size >= VERTEX_LIMIT as u16 {
            return;
        }

        self.get_clip_matrix();
        if self.clip_mtx_push {
            self.clip_matrices.push(self.clip_matrix);
            self.clip_mtx_push = false;
        }

        self.vertices[self.vertices_size as usize] = self.cur_vtx;
        self.vertices[self.vertices_size as usize].clip_matrix_index = self.clip_matrices.len() as u16 - 1;
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
        self.get_clip_matrix()[index] as u32
    }

    pub fn get_vec_mtx_result(&self, index: usize) -> u32 {
        self.matrices.dir[(index / 3) * 4 + index % 3] as u32
    }

    pub fn get_gx_stat(&self) -> u32 {
        u32::from(self.gx_stat)
    }

    pub fn get_ram_count(&self) -> u32 {
        ((self.vertices_size as u32) << 16) | (self.polygons_size as u32)
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

    unpacked_cmd!(mtx_mode, 0x10);
    unpacked_cmd!(mtx_push, 0x11);
    unpacked_cmd!(mtx_pop, 0x12);
    unpacked_cmd!(mtx_store, 0x13);
    unpacked_cmd!(mtx_restore, 0x14);
    unpacked_cmd!(mtx_identity, 0x15);
    unpacked_cmd!(mtx_load44, 0x16);
    unpacked_cmd!(mtx_load43, 0x17);
    unpacked_cmd!(mtx_mult44, 0x18);
    unpacked_cmd!(mtx_mult43, 0x19);
    unpacked_cmd!(mtx_mult33, 0x1A);
    unpacked_cmd!(mtx_scale, 0x1B);
    unpacked_cmd!(mtx_trans, 0x1C);
    unpacked_cmd!(color, 0x20);
    unpacked_cmd!(normal, 0x21);
    unpacked_cmd!(tex_coord, 0x22);
    unpacked_cmd!(vtx16, 0x23);
    unpacked_cmd!(vtx10, 0x24);
    unpacked_cmd!(vtx_x_y, 0x25);
    unpacked_cmd!(vtx_x_z, 0x26);
    unpacked_cmd!(vtx_y_z, 0x27);
    unpacked_cmd!(vtx_diff, 0x28);
    unpacked_cmd!(polygon_attr, 0x29);
    unpacked_cmd!(tex_image_param, 0x2A);
    unpacked_cmd!(pltt_base, 0x2B);
    unpacked_cmd!(dif_amb, 0x30);
    unpacked_cmd!(spe_emi, 0x31);
    unpacked_cmd!(light_vector, 0x32);
    unpacked_cmd!(light_color, 0x33);
    unpacked_cmd!(shininess, 0x34);
    unpacked_cmd!(begin_vtxs, 0x40);
    unpacked_cmd!(end_vtxs, 0x41);
    unpacked_cmd!(swap_buffers, 0x50);
    unpacked_cmd!(viewport, 0x60);
    unpacked_cmd!(box_test, 0x70);
    unpacked_cmd!(pos_test, 0x71);
    unpacked_cmd!(vec_test, 0x72);

    pub fn set_gx_stat(&mut self, mut mask: u32, value: u32) {
        if value & (1 << 15) != 0 {
            self.gx_stat = (u32::from(self.gx_stat) & !0xA000).into();
        }

        mask &= 0xC0000000;
        self.gx_stat = ((u32::from(self.gx_stat) & !mask) | (value & mask)).into();
    }
}
