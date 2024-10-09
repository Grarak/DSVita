use crate::core::cpu_regs::InterruptFlag;
use crate::core::emu::{get_cm_mut, get_cpu_regs_mut, get_mem_mut, io_dma, Emu};
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::memory::dma::DmaTransferMode;
use crate::core::CpuType::ARM9;
use crate::math::{Matrix, Vectori16, Vectori32, Vectoru16};
use crate::utils::{rgb5_to_rgb6, HeapMem};
use bilge::prelude::*;
use std::collections::VecDeque;
use std::hint::unreachable_unchecked;
use std::mem;

#[bitsize(32)]
#[derive(Copy, Clone, FromBits)]
pub struct GxStat {
    box_pos_vec_test_busy: bool,
    box_test_result: u1,
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

const FIFO_PARAM_COUNTS: [u8; 128] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x00-0x0F
    1, 0, 1, 1, 1, 0, 16, 12, 16, 12, 9, 3, 3, 0, 0, 0, // 0x10-0x1F
    1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, // 0x20-0x2F
    1, 1, 1, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x30-0x3F
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x40-0x4F
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x50-0x5F
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x60-0x6F
    3, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x70-0x7F
];

#[derive(Copy, Clone)]
struct Entry {
    cmd: u8,
    param: u32,
}

impl Entry {
    fn new(cmd: u8, param: u32) -> Self {
        Entry { cmd, param }
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
}

fn intersect(v1: &Vertex, v2: &Vertex, val1: i32, val2: i32) -> Vertex {
    let d1 = val1 as i64 + v1.coords[3] as i64;
    let d2 = val2 as i64 + v2.coords[3] as i64;
    if d2 == d1 {
        return *v1;
    }

    let mut vertex = Vertex::default();
    for i in 0..4 {
        vertex.coords[i] = v1.coords[i] + ((v2.coords[i] - v1.coords[i]) as i64 * -d1 / (d2 - d1)) as i32;
    }
    vertex
}

fn clip_polygon(unclipped: &[Vertex; 4], clipped: &mut [Vertex; 10], size: &mut usize) -> bool {
    let mut clip = false;

    let mut vertices = [Vertex::default(); 10];
    vertices[..4].copy_from_slice(unclipped);

    for i in 0..6 {
        let old_size = *size;
        *size = 0;

        for j in 0..old_size {
            let current = &vertices[j];
            let previous = &vertices[(j.wrapping_sub(1).wrapping_add(old_size)) % old_size];

            let (current_val, previous_val) = match i {
                0 => (current.coords[0], previous.coords[0]),
                1 => (-current.coords[0], -previous.coords[0]),
                2 => (current.coords[1], previous.coords[1]),
                3 => (-current.coords[1], -previous.coords[1]),
                4 => (current.coords[2], previous.coords[2]),
                5 => (-current.coords[2], -previous.coords[2]),
                _ => unsafe { unreachable_unchecked() },
            };

            if current_val >= -current.coords[3] {
                if previous_val < -previous.coords[3] {
                    clipped[*size] = intersect(current, previous, current_val, previous_val);
                    *size += 1;
                    clip = true;
                }

                clipped[*size] = *current;
                *size += 1;
            } else if previous_val >= -previous.coords[3] {
                clipped[*size] = intersect(current, previous, current_val, previous_val);
                *size += 1;
                clip = true;
            }
        }

        vertices[..*size].copy_from_slice(&clipped[..*size]);
    }

    clip
}

#[derive(Copy, Clone, Default)]
pub struct Polygon {
    pub size: usize,
    pub vertices_index: usize,
    crossed: bool,
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
    ins: HeapMem<Vertex, 6144>,
    pub outs: HeapMem<Vertex, 6144>,
    count_in: usize,
    pub count_out: usize,
    process_count: usize,
}

#[derive(Default)]
pub struct Polygons {
    ins: HeapMem<Polygon, 2048>,
    pub outs: HeapMem<Polygon, 2048>,
    count_in: usize,
    pub count_out: usize,
}

#[derive(Default)]
pub struct Gpu3DRegisters {
    cmd_fifo: VecDeque<Entry>,
    cmd_pipe_size: u8,
    mtx_queue: u32,
    test_queue: u32,

    cmd_fifo_param_count: u32,

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

    vertex_count: usize,
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

    viewport: Vectoru16<4>,
    viewport_next: Vectoru16<4>,

    pub gx_stat: GxStat,
    gx_fifo: u32,
    pos_result: [i32; 4],
    vec_result: [i16; 3],
}

impl Gpu3DRegisters {
    fn is_cmd_fifo_full(&self) -> bool {
        self.cmd_fifo.len() - self.cmd_pipe_size as usize >= 256
    }

    fn is_cmd_fifo_half_full(&self) -> bool {
        self.cmd_fifo.len() - self.cmd_pipe_size as usize >= 128
    }

    fn is_cmd_fifo_empty(&self) -> bool {
        self.cmd_fifo.len() <= 4
    }

    fn is_cmd_pipe_full(&self) -> bool {
        self.cmd_pipe_size == 4
    }

    pub fn run_cmds(&mut self, total_cycles: u64, emu: &mut Emu) {
        if self.cmd_fifo.is_empty() || !self.gx_stat.geometry_busy() || self.flushed {
            self.last_total_cycles = total_cycles;
            return;
        }

        let cycle_diff = (total_cycles - self.last_total_cycles) as u32;
        self.last_total_cycles = total_cycles;
        let mut executed_cycles = 0;

        let mut refresh_state = |gpu_3d: &mut Self| {
            gpu_3d.gx_stat.set_num_entries_cmd_fifo(u9::new(gpu_3d.cmd_fifo.len() as u16 - gpu_3d.cmd_pipe_size as u16));
            gpu_3d.gx_stat.set_cmd_fifo_empty(gpu_3d.is_cmd_fifo_empty());
            gpu_3d.gx_stat.set_geometry_busy(!gpu_3d.cmd_fifo.is_empty());

            if !gpu_3d.gx_stat.cmd_fifo_less_half_full() && !gpu_3d.is_cmd_fifo_half_full() {
                gpu_3d.gx_stat.set_cmd_fifo_less_half_full(true);
                io_dma!(emu, ARM9).trigger_all(DmaTransferMode::GeometryCmdFifo, get_cm_mut!(emu));
            }

            match u8::from(gpu_3d.gx_stat.cmd_fifo_irq()) {
                0 | 3 => {}
                1 => {
                    if gpu_3d.gx_stat.cmd_fifo_less_half_full() {
                        get_cpu_regs_mut!(emu, ARM9).send_interrupt(InterruptFlag::GeometryCmdFifo, emu);
                    }
                }
                2 => {
                    if gpu_3d.gx_stat.cmd_fifo_empty() {
                        get_cpu_regs_mut!(emu, ARM9).send_interrupt(InterruptFlag::GeometryCmdFifo, emu);
                    }
                }
                _ => unsafe { unreachable_unchecked() },
            }
        };

        while !self.cmd_fifo.is_empty() && executed_cycles < cycle_diff && !self.flushed {
            let mut params = Vec::new();
            let entry = unsafe { *self.cmd_fifo.front().unwrap_unchecked() };
            let mut param_count = FIFO_PARAM_COUNTS[entry.cmd as usize];
            if param_count > 1 {
                if param_count as usize > self.cmd_fifo.len() {
                    break;
                }

                params.reserve(param_count as usize);
                for _ in 0..param_count {
                    params.push(unsafe { self.cmd_fifo.pop_front().unwrap_unchecked().param });
                }
            } else {
                param_count = 1;
                self.cmd_fifo.pop_front();
            }

            match entry.cmd {
                0x10 => self.exe_mtx_mode(entry.param),
                0x11 => self.exe_mtx_push(),
                0x12 => self.exe_mtx_pop(entry.param),
                0x13 => self.exe_mtx_store(entry.param),
                0x14 => self.exe_mtx_restore(entry.param),
                0x15 => self.exe_mtx_identity(),
                0x16 => self.exe_mtx_load44(unsafe { params.try_into().unwrap_unchecked() }),
                0x17 => self.exe_mtx_load43(unsafe { params.try_into().unwrap_unchecked() }),
                0x18 => self.exe_mtx_mult44(unsafe { params.try_into().unwrap_unchecked() }),
                0x19 => self.exe_mtx_mult43(unsafe { params.try_into().unwrap_unchecked() }),
                0x1A => self.exe_mtx_mult33(unsafe { params.try_into().unwrap_unchecked() }),
                0x1B => self.exe_mtx_scale(unsafe { params.try_into().unwrap_unchecked() }),
                0x1C => self.exe_mtx_trans(unsafe { params.try_into().unwrap_unchecked() }),
                0x20 => self.exe_color(entry.param),
                0x21 => self.exe_normal(entry.param),
                0x22 => self.exe_tex_coord(entry.param),
                0x23 => self.exe_vtx16(unsafe { params.try_into().unwrap_unchecked() }),
                0x24 => self.exe_vtx10(entry.param),
                0x25 => self.exe_vtx_x_y(entry.param),
                0x26 => self.exe_vtx_x_z(entry.param),
                0x27 => self.exe_vtx_y_z(entry.param),
                0x28 => self.exe_vtx_diff(entry.param),
                0x29 => self.exe_polygon_attr(entry.param),
                0x2A => self.exe_tex_image_param(entry.param),
                0x2B => self.exe_pltt_base(entry.param),
                0x30 => self.exe_dif_amb(entry.param),
                0x31 => self.exe_spe_emi(entry.param),
                0x32 => self.exe_light_vector(entry.param),
                0x33 => self.exe_light_color(entry.param),
                0x34 => self.exe_shininess(entry.param),
                0x40 => self.exe_begin_vtxs(entry.param),
                0x41 => {}
                0x50 => self.exe_swap_buffers(entry.param),
                0x60 => self.exe_viewport(entry.param),
                0x70 => {
                    // todo!()
                }
                0x71 => {
                    // todo!()
                }
                0x72 => {
                    // todo!()
                }
                _ => {
                    todo!("{:x}", entry.cmd);
                }
            }
            executed_cycles += 4;

            self.cmd_pipe_size = 4 - ((self.cmd_pipe_size + param_count) & 1);
            if self.cmd_pipe_size as usize > self.cmd_fifo.len() {
                self.cmd_pipe_size = self.cmd_fifo.len() as u8;
            }
        }

        refresh_state(self);

        if !self.is_cmd_fifo_full() {
            get_cpu_regs_mut!(emu, ARM9).unhalt(1);
        }
    }

    fn exe_mtx_mode(&mut self, param: u32) {
        self.mtx_mode = MtxMode::from((param & 0x3) as u8);
    }

    fn decrease_mtx_queue(&mut self) {
        self.mtx_queue -= 1;
        if self.mtx_queue == 0 {
            self.gx_stat.set_mtx_stack_busy(false);
        }
    }

    fn exe_mtx_push(&mut self) {
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

        self.decrease_mtx_queue();
    }

    fn exe_mtx_pop(&mut self, param: u32) {
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
                let ptr = (u8::from(self.gx_stat.pos_vec_mtx_stack_lvl()) as i8 - (((param << 2) as i8) >> 2)) as u8;
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

        self.decrease_mtx_queue();
    }

    fn exe_mtx_store(&mut self, param: u32) {
        match self.mtx_mode {
            MtxMode::Projection => self.matrices.proj_stack = self.matrices.proj,
            MtxMode::ModelView | MtxMode::ModelViewVec => {
                let addr = param & 0x1F;

                if addr == 31 {
                    self.gx_stat.set_mtx_stack_overflow_underflow_err(true);
                }

                self.matrices.model_stack[addr as usize] = self.matrices.model;
                self.matrices.vec_stack[addr as usize] = self.matrices.vec;
            }
            MtxMode::Texture => self.matrices.tex_stack = self.matrices.tex,
        }
    }

    fn exe_mtx_restore(&mut self, param: u32) {
        match self.mtx_mode {
            MtxMode::Projection => {
                self.matrices.proj = self.matrices.proj_stack;
                self.clip_dirty = true;
            }
            MtxMode::ModelView | MtxMode::ModelViewVec => {
                let addr = param & 0x1F;

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

    fn exe_mtx_identity(&mut self) {
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

    fn exe_mtx_load44(&mut self, param: [u32; 16]) {
        self.mtx_load(unsafe { mem::transmute(param) });
    }

    fn exe_mtx_load43(&mut self, param: [u32; 12]) {
        let mut mtx = Matrix::default();
        for i in 0..4 {
            mtx.as_mut()[i * 4..i * 4 + 3].copy_from_slice(unsafe { mem::transmute(&param[i * 3..i * 3 + 3]) });
        }
        self.mtx_load(mtx);
    }

    fn mtx_mult(&mut self, mtx: Matrix) {
        match self.mtx_mode {
            MtxMode::Projection => {
                self.matrices.proj = mtx * self.matrices.proj;
                self.clip_dirty = true;
            }
            MtxMode::ModelView => {
                self.matrices.model = mtx * self.matrices.model;
                self.clip_dirty = true;
            }
            MtxMode::ModelViewVec => {
                self.matrices.model = mtx * self.matrices.model;
                self.matrices.vec = mtx * self.matrices.vec;
                self.clip_dirty = true;
            }
            MtxMode::Texture => {
                self.matrices.tex = mtx * self.matrices.tex;
            }
        }
    }

    fn exe_mtx_mult44(&mut self, param: [u32; 16]) {
        self.mtx_mult(unsafe { mem::transmute(param) });
    }

    fn exe_mtx_mult43(&mut self, param: [u32; 12]) {
        let mut mtx = Matrix::default();
        for i in 0..4 {
            mtx.as_mut()[i * 4..i * 4 + 3].copy_from_slice(unsafe { mem::transmute(&param[i * 3..i * 3 + 3]) });
        }
        self.mtx_mult(mtx);
    }

    fn exe_mtx_mult33(&mut self, param: [u32; 9]) {
        let mut mtx = Matrix::default();
        for i in 0..3 {
            mtx.as_mut()[i * 4..i * 4 + 3].copy_from_slice(unsafe { mem::transmute(&param[i * 3..i * 3 + 3]) });
        }
        self.mtx_mult(mtx);
    }

    fn exe_mtx_scale(&mut self, param: [u32; 3]) {
        let mut mtx = Matrix::default();
        for i in 0..3 {
            mtx[i * 5] = param[i] as i32;
        }
        self.mtx_mult(mtx);
    }

    fn exe_mtx_trans(&mut self, param: [u32; 3]) {
        let mut mtx = Matrix::default();
        mtx.as_mut()[12..15].copy_from_slice(unsafe { mem::transmute(param.as_slice()) });
        self.mtx_mult(mtx);
    }

    fn exe_color(&mut self, param: u32) {
        self.saved_vertex.color = rgb5_to_rgb6(param);
    }

    fn exe_normal(&mut self, param: u32) {
        let normal_vector_param = NormalVector::from(param);
        let mut normal_vector = Vectori32::<3>::default();
        normal_vector[0] = (((u16::from(normal_vector_param.x()) << 6) as i16) >> 3) as i32;
        normal_vector[1] = (((u16::from(normal_vector_param.y()) << 6) as i16) >> 3) as i32;
        normal_vector[2] = (((u16::from(normal_vector_param.z()) << 6) as i16) >> 3) as i32;

        if self.texture_coord_mode == TextureCoordTransMode::Normal {
            let mut vector = Vectori32::<4>::from(normal_vector);
            vector[3] = 1 << 12;

            let mut matrix = self.matrices.tex;
            matrix[12] = (self.s as i32) << 12;
            matrix[13] = (self.t as i32) << 12;

            vector *= matrix;

            self.saved_vertex.tex_coords[0] = (vector[0] >> 12) as i16;
            self.saved_vertex.tex_coords[1] = (vector[1] >> 12) as i16;
        }

        normal_vector *= self.matrices.vec;

        self.saved_vertex.color = self.emission_color;

        for i in 0..4 {
            if self.enabled_lights & (1 << i) == 0 {
                continue;
            }

            let diffuse_level = -(self.light_vectors[i] * normal_vector);
            let diffuse_level = diffuse_level.clamp(0, 1 << 12) as u32;

            let shininess_level = -(self.half_vectors[i] * normal_vector);
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

    fn exe_tex_coord(&mut self, param: u32) {
        let tex_coord = TexCoord::from(param);
        if self.texture_coord_mode == TextureCoordTransMode::TexCoord {
            let mut vector = Vectori32::<4>::default();
            vector[0] = (tex_coord.s() as i16 as i32) << 8;
            vector[1] = (tex_coord.t() as i16 as i32) << 8;
            vector[2] = 1 << 8;
            vector[3] = 1 << 8;

            vector *= self.matrices.tex;

            self.saved_vertex.tex_coords[0] = (vector[0] >> 8) as i16;
            self.saved_vertex.tex_coords[1] = (vector[1] >> 8) as i16;
        } else {
            self.saved_vertex.tex_coords[0] = tex_coord.s() as i16;
            self.saved_vertex.tex_coords[1] = tex_coord.t() as i16;
        }
    }

    fn exe_vtx16(&mut self, params: [u32; 2]) {
        self.saved_vertex.coords[0] = params[0] as i16 as i32;
        self.saved_vertex.coords[1] = (params[0] >> 16) as i16 as i32;
        self.saved_vertex.coords[2] = params[1] as i16 as i32;

        self.add_vertex();
    }

    fn exe_vtx10(&mut self, param: u32) {
        self.saved_vertex.coords[0] = ((param & 0x3FF) << 6) as i16 as i32;
        self.saved_vertex.coords[1] = ((param & 0xFFC00) >> 4) as i16 as i32;
        self.saved_vertex.coords[2] = ((param & 0x3FF00000) >> 14) as i16 as i32;

        self.add_vertex();
    }

    fn exe_vtx_x_y(&mut self, param: u32) {
        self.saved_vertex.coords[0] = param as i16 as i32;
        self.saved_vertex.coords[1] = (param >> 16) as i16 as i32;

        self.add_vertex();
    }

    fn exe_vtx_x_z(&mut self, param: u32) {
        self.saved_vertex.coords[0] = param as i16 as i32;
        self.saved_vertex.coords[2] = (param >> 16) as i16 as i32;

        self.add_vertex();
    }

    fn exe_vtx_y_z(&mut self, param: u32) {
        self.saved_vertex.coords[1] = param as i16 as i32;
        self.saved_vertex.coords[2] = (param >> 16) as i16 as i32;

        self.add_vertex();
    }

    fn exe_vtx_diff(&mut self, param: u32) {
        self.saved_vertex.coords[0] += (((param & 0x3FF) << 6) as i16 as i32) >> 6;
        self.saved_vertex.coords[1] += (((param & 0xFFC00) >> 4) as i16 as i32) >> 6;
        self.saved_vertex.coords[2] += (((param & 0x3FF00000) >> 14) as i16 as i32) >> 6;

        self.add_vertex();
    }

    fn exe_polygon_attr(&mut self, param: u32) {
        self.polygon_attr = param.into();
    }

    fn exe_tex_image_param(&mut self, param: u32) {
        let Self {
            saved_polygon, texture_coord_mode, ..
        } = self;
        saved_polygon.tex_image_param = TexImageParam::from(param);
        *texture_coord_mode = TextureCoordTransMode::from(u8::from(saved_polygon.tex_image_param.coord_trans_mode()));
    }

    fn exe_pltt_base(&mut self, param: u32) {
        self.saved_polygon.palette_addr = (param & 0x1FFF) as u16;
    }

    fn exe_dif_amb(&mut self, param: u32) {
        let material_color0 = MaterialColor0::from(param);
        self.diffuse_color = rgb5_to_rgb6(u32::from(material_color0.dif()));
        self.ambient_color = rgb5_to_rgb6(u32::from(material_color0.amb()));

        if material_color0.set_vertex_color() {
            self.saved_vertex.color = self.diffuse_color;
        }
    }

    fn exe_spe_emi(&mut self, param: u32) {
        let material_color1 = MaterialColor1::from(param);
        self.specular_color = rgb5_to_rgb6(u32::from(material_color1.spe()));
        self.emission_color = rgb5_to_rgb6(u32::from(material_color1.em()));
        self.shininess_enabled = material_color1.set_shininess();
    }

    fn exe_light_vector(&mut self, param: u32) {
        let light_vector = LightVector::from(param);
        let num = u8::from(light_vector.num()) as usize;
        // shift left for signedness
        // shift right to convert 9 to 12 fractional bits
        self.light_vectors[num][0] = (((u16::from(light_vector.x()) << 6) as i16) >> 3) as i32;
        self.light_vectors[num][1] = (((u16::from(light_vector.y()) << 6) as i16) >> 3) as i32;
        self.light_vectors[num][2] = (((u16::from(light_vector.z()) << 6) as i16) >> 3) as i32;

        self.light_vectors[num] *= self.matrices.vec;

        self.half_vectors[num][0] = self.light_vectors[num][0] >> 1;
        self.half_vectors[num][1] = self.light_vectors[num][1] >> 1;
        self.half_vectors[num][2] = (self.light_vectors[num][2] - (1 << 12)) >> 1;
    }

    fn exe_light_color(&mut self, param: u32) {
        let light_color = LightColor::from(param);
        self.light_colors[u8::from(light_color.num()) as usize] = rgb5_to_rgb6(u32::from(light_color.color()));
    }

    fn exe_shininess(&mut self, param: u32) {
        let shininess = Shininess::from(param);
        self.shininess[0] = shininess.shininess0();
        self.shininess[1] = shininess.shininess1();
        self.shininess[2] = shininess.shininess2();
        self.shininess[3] = shininess.shininess3();
    }

    fn exe_begin_vtxs(&mut self, param: u32) {
        if self.vertex_count < self.polygon_type.vertex_count() as usize {
            self.vertices.count_in -= self.vertex_count;
        }

        self.process_vertices();
        self.polygon_type = PolygonType::from((param & 0x3) as u8);
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

    fn exe_swap_buffers(&mut self, param: u32) {
        self.saved_polygon.w_buffer = (param & 0x2) != 0;
        self.flushed = true;
    }

    fn exe_viewport(&mut self, param: u32) {
        let viewport = Viewport::from(param);
        self.viewport_next[0] = viewport.x1() as u16;
        self.viewport_next[1] = 191 - viewport.y2() as u16;
        self.viewport_next[2] = viewport.x2() as u16 - self.viewport_next[0] + 1;
        self.viewport_next[3] = (191 - viewport.y1() as u16) - self.viewport_next[1] + 1;
    }

    fn process_vertices(&mut self) {
        let [x, y, w, h] = *self.viewport.as_ref();
        let Self { vertices, .. } = self;

        for i in vertices.process_count..vertices.count_in {
            let coords = &mut vertices.ins[i].coords;
            if coords[3] != 0 {
                coords[0] = ((coords[0] as i64 + coords[3] as i64) * w as i64 / (coords[3] as i64 * 2) + x as i64) as i32;
                coords[1] = ((-coords[1] as i64 + coords[3] as i64) * h as i64 / (coords[3] as i64 * 2) + y as i64) as i32;
                coords[2] = (((coords[2] as i64) << 12) / coords[3] as i64) as i32;
            }
        }

        vertices.process_count = vertices.count_in;
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

        if vertices.count_in >= 6144 {
            return;
        }

        vertices.ins[vertices.count_in] = self.saved_vertex;
        vertices.ins[vertices.count_in].coords[3] = 1 << 12;

        if self.texture_coord_mode == TextureCoordTransMode::Vertex {
            let mut matrix = matrices.tex;
            matrix[12] = (self.s as i32) << 12;
            matrix[13] = (self.t as i32) << 12;

            let vector = vertices.ins[vertices.count_in].coords * matrix;

            vertices.ins[vertices.count_in].tex_coords[0] = (vector[0] >> 12) as i16;
            vertices.ins[vertices.count_in].tex_coords[1] = (vector[1] >> 12) as i16;
        }

        if self.clip_dirty {
            matrices.clip = matrices.model * matrices.proj;
            self.clip_dirty = false;
        }

        vertices.ins[vertices.count_in].coords *= matrices.clip;

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
        if self.polygons.count_in >= 2048 {
            return;
        }

        let size = self.polygon_type.vertex_count() as usize;
        self.saved_polygon.size = size;
        self.saved_polygon.vertices_index = self.vertices.count_in - size;

        let mut unclipped = [Vertex::default(); 4];
        unclipped[..size].copy_from_slice(&self.vertices.ins[self.saved_polygon.vertices_index..self.saved_polygon.vertices_index + size]);

        if self.polygon_type == PolygonType::QuadliteralStrips {
            unclipped.swap(2, 3);
            self.vertices.ins.swap(self.saved_polygon.vertices_index + 2, self.saved_polygon.vertices_index + 3);
        }

        let x1 = (unclipped[1].coords[0] - unclipped[0].coords[0]) as i64;
        let y1 = (unclipped[1].coords[1] - unclipped[0].coords[1]) as i64;
        let w1 = (unclipped[1].coords[3] - unclipped[0].coords[3]) as i64;
        let x2 = (unclipped[2].coords[0] - unclipped[0].coords[0]) as i64;
        let y2 = (unclipped[2].coords[1] - unclipped[0].coords[1]) as i64;
        let w2 = (unclipped[2].coords[3] - unclipped[0].coords[3]) as i64;

        let mut xc = y1 * w2 - w1 * y2;
        let mut yc = w1 * x2 - x1 * w2;
        let mut wc = x1 * y2 - y1 * x2;

        while xc != xc as i32 as i64 || yc != yc as i32 as i64 || wc != wc as i32 as i64 {
            xc >>= 4;
            yc >>= 4;
            wc >>= 4;
        }

        let mut dot = xc * unclipped[0].coords[0] as i64 + yc * unclipped[0].coords[1] as i64 + wc * unclipped[0].coords[3] as i64;

        self.saved_polygon.clockwise = dot < 0;

        if self.polygon_type == PolygonType::TriangleStrips {
            if self.clockwise {
                dot = -dot;
            }
            self.clockwise = !self.clockwise;
        }

        let mut clipped = [Vertex::default(); 10];
        let cull = (!self.render_front && dot > 0) || (!self.render_back && dot < 0);
        let mut clipped_size = self.saved_polygon.size;
        let clip = if cull { false } else { clip_polygon(&unclipped, &mut clipped, &mut clipped_size) };

        if cull || clipped_size == 0 {
            match self.polygon_type {
                PolygonType::SeparateTriangles | PolygonType::SeparateQuadliterals => {
                    self.vertices.count_in -= size;
                }
                PolygonType::TriangleStrips => {
                    if self.vertex_count == 3 {
                        self.vertices.ins[self.vertices.count_in - 3] = self.vertices.ins[self.vertices.count_in - 2];
                        self.vertices.ins[self.vertices.count_in - 2] = self.vertices.ins[self.vertices.count_in - 1];
                        self.vertices.count_in -= 1;
                        self.vertex_count -= 1;
                    } else if self.vertices.count_in < 6144 {
                        self.vertices.ins[self.vertices.count_in] = self.vertices.ins[self.vertices.count_in - 1];
                        self.vertices.ins[self.vertices.count_in - 1] = self.vertices.ins[self.vertices.count_in - 2];
                        self.vertices.count_in += 1;
                        self.vertex_count = 2;
                    }
                }
                PolygonType::QuadliteralStrips => {
                    if self.vertex_count == 4 {
                        self.vertices.ins[self.vertices.count_in - 4] = self.vertices.ins[self.vertices.count_in - 2];
                        self.vertices.ins[self.vertices.count_in - 3] = self.vertices.ins[self.vertices.count_in - 1];
                        self.vertices.count_in -= 2;
                        self.vertex_count -= 2;
                    } else {
                        self.vertex_count = 2;
                    }
                }
            }
            return;
        }

        // if clip {
        //     match self.polygon_type {
        //         PolygonType::SeparateTriangles | PolygonType::SeparateQuadliterals => {
        //             self.vertices.count_in -= size;
        //
        //             for i in 0..self.saved_polygon.size {
        //                 if self.vertices.count_in >= 6144 {
        //                     return;
        //                 }
        //                 self.vertices.ins[self.vertices.count_in] = clipped[i];
        //                 self.vertices.count_in += 1;
        //             }
        //         }
        //         PolygonType::TriangleStrips => {
        //             self.vertices.count_in -= if self.vertex_count == 3 { 3 } else { 1 };
        //             self.saved_polygon.vertices_index = self.vertices.count_in;
        //
        //             for i in 0..self.saved_polygon.size {
        //                 if self.vertices.count_in >= 6144 {
        //                     return;
        //                 }
        //                 self.vertices.ins[self.vertices.count_in] = clipped[i];
        //                 self.vertices.count_in += 1;
        //             }
        //
        //             for i in 0..2 {
        //                 if self.vertices.count_in >= 6144 {
        //                     return;
        //                 }
        //                 self.vertices.ins[self.vertices.count_in] = clipped[i];
        //                 self.vertices.count_in += 1;
        //             }
        //             self.vertex_count = 2;
        //         }
        //         PolygonType::QuadliteralStrips => {
        //             self.vertices.count_in -= if self.vertex_count == 4 { 4 } else { 2 };
        //             self.saved_polygon.vertices_index = self.vertices.count_in;
        //
        //             for i in 0..self.saved_polygon.size {
        //                 if self.vertices.count_in >= 6144 {
        //                     return;
        //                 }
        //                 self.vertices.ins[self.vertices.count_in] = clipped[i];
        //                 self.vertices.count_in += 1;
        //             }
        //
        //             for i in 0..2 {
        //                 if self.vertices.count_in >= 6144 {
        //                     return;
        //                 }
        //                 self.vertices.ins[self.vertices.count_in] = clipped[3 - i];
        //                 self.vertices.count_in += 1;
        //             }
        //             self.vertex_count = 2;
        //         }
        //     }
        // }

        self.polygons.ins[self.polygons.count_in] = self.saved_polygon;
        self.polygons.ins[self.polygons.count_in].crossed = self.polygon_type == PolygonType::QuadliteralStrips && !clip;

        self.polygons.count_in += 1;
    }

    fn queue_entry(&mut self, entry: Entry, emu: &mut Emu) {
        if self.cmd_fifo.is_empty() && !self.is_cmd_pipe_full() {
            self.cmd_fifo.push_back(entry);
            self.cmd_pipe_size += 1;
            self.gx_stat.set_geometry_busy(true);
        } else {
            if self.is_cmd_fifo_full() {
                get_mem_mut!(emu).breakout_imm = true;
                get_cpu_regs_mut!(emu, ARM9).halt(1);
            }

            self.cmd_fifo.push_back(entry);
            self.gx_stat.set_num_entries_cmd_fifo(u9::new(self.cmd_fifo.len() as u16 - self.cmd_pipe_size as u16));
            self.gx_stat.set_cmd_fifo_empty(false);

            self.gx_stat.set_cmd_fifo_less_half_full(!self.is_cmd_fifo_half_full());
        }

        match entry.cmd {
            0x11 | 0x12 => {
                self.mtx_queue += 1;
                self.gx_stat.set_mtx_stack_busy(true);
            }
            0x70 | 0x71 | 0x72 => {
                self.test_queue += 1;
            }
            _ => {}
        }
    }

    pub fn get_clip_mtx_result(&mut self, index: usize) -> u32 {
        if self.clip_dirty {
            self.matrices.clip = self.matrices.model * self.matrices.proj;
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

    pub fn set_gx_fifo(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        if self.gx_fifo == 0 {
            self.gx_fifo = value & mask;
        } else {
            self.queue_entry(Entry::new(self.gx_fifo as u8, value & mask), emu);
            self.cmd_fifo_param_count += 1;

            if self.cmd_fifo_param_count == FIFO_PARAM_COUNTS[(self.gx_fifo & 0xFF) as usize] as u32 {
                self.gx_fifo >>= 8;
                self.cmd_fifo_param_count = 0;
            }
        }

        while self.gx_fifo != 0 && FIFO_PARAM_COUNTS[(self.gx_fifo & 0xFF) as usize] == 0 {
            self.queue_entry(Entry::new(self.gx_fifo as u8, value & mask), emu);
            self.gx_fifo >>= 8;
        }
    }

    pub fn set_mtx_mode(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x10, value & mask), emu);
    }

    pub fn set_mtx_push(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x11, value & mask), emu);
    }

    pub fn set_mtx_pop(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x12, value & mask), emu);
    }

    pub fn set_mtx_store(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x13, value & mask), emu);
    }

    pub fn set_mtx_restore(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x14, value & mask), emu);
    }

    pub fn set_mtx_identity(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x15, value & mask), emu);
    }

    pub fn set_mtx_load44(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x16, value & mask), emu);
    }

    pub fn set_mtx_load43(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x17, value & mask), emu);
    }

    pub fn set_mtx_mult44(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x18, value & mask), emu);
    }

    pub fn set_mtx_mult43(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x19, value & mask), emu);
    }

    pub fn set_mtx_mult33(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x1A, value & mask), emu);
    }

    pub fn set_mtx_scale(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x1B, value & mask), emu);
    }

    pub fn set_mtx_trans(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x1C, value & mask), emu);
    }

    pub fn set_color(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x20, value & mask), emu);
    }

    pub fn set_normal(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x21, value & mask), emu);
    }

    pub fn set_tex_coord(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x22, value & mask), emu);
    }

    pub fn set_vtx16(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x23, value & mask), emu);
    }

    pub fn set_vtx10(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x24, value & mask), emu);
    }

    pub fn set_vtx_x_y(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x25, value & mask), emu);
    }

    pub fn set_vtx_x_z(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x26, value & mask), emu);
    }

    pub fn set_vtx_y_z(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x27, value & mask), emu);
    }

    pub fn set_vtx_diff(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x28, value & mask), emu);
    }

    pub fn set_polygon_attr(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x29, value & mask), emu);
    }

    pub fn set_tex_image_param(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x2A, value & mask), emu);
    }

    pub fn set_pltt_base(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x2B, value & mask), emu);
    }

    pub fn set_dif_amb(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x30, value & mask), emu);
    }

    pub fn set_spe_emi(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x31, value & mask), emu);
    }

    pub fn set_light_vector(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x32, value & mask), emu);
    }

    pub fn set_light_color(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x33, value & mask), emu);
    }

    pub fn set_shininess(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x34, value & mask), emu);
    }

    pub fn set_begin_vtxs(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x40, value & mask), emu);
    }

    pub fn set_end_vtxs(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x41, value & mask), emu);
    }

    pub fn set_swap_buffers(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x50, value & mask), emu);
    }

    pub fn set_viewport(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x60, value & mask), emu);
    }

    pub fn set_box_test(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x70, value & mask), emu);
    }

    pub fn set_pos_test(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x71, value & mask), emu);
    }

    pub fn set_vec_test(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.queue_entry(Entry::new(0x72, value & mask), emu);
    }

    pub fn set_gx_stat(&mut self, mut mask: u32, value: u32) {
        if value & (1 << 15) != 0 {
            self.gx_stat = (u32::from(self.gx_stat) & !0xA000).into();
        }

        mask &= 0xC0000000;
        self.gx_stat = ((u32::from(self.gx_stat) & !mask) | (value & mask)).into();
    }
}
