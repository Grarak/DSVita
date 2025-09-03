use crate::core::cpu_regs::InterruptFlag;
use crate::core::emu::Emu;
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_3d::matrix_vec::MatrixVec;
use crate::core::graphics::gpu_3d::renderer_3d::Gpu3DRendererContent;
use crate::core::memory::dma::DmaTransferMode;
use crate::core::CpuType::ARM9;
use crate::fixed_fifo::FixedFifo;
use crate::logging::debug_println;
use crate::math::{vmult_mat4, Matrix, Vectori16, Vectori32, MTX_IDENTITY};
use crate::utils::HeapMem;
use bilge::prelude::*;
use paste::paste;
use std::arch::arm::{int32x4_t, vld1q_s32, vld1q_s32_x3, vld1q_s32_x4, vsetq_lane_s32, vst1q_s32_x4};
use std::cmp::{max, min};
use std::hint::assert_unchecked;
use std::intrinsics::{likely, unlikely};
use std::mem::MaybeUninit;
use std::{mem, ptr};

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
    cmd_fifo_less_half_full: bool,
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
#[derive(Copy, Clone, DebugBits, Default, FromBits)]
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
#[derive(Copy, Clone, DebugBits, Default, FromBits)]
pub struct PolygonAttr {
    pub enable_lights: u4,
    pub mode: u2,
    pub render_back: bool,
    pub render_front: bool,
    not_used: u3,
    pub trans_new_depth: bool,
    pub render_far_plane: bool,
    pub render_1_bot_polygons: bool,
    pub depth_test_equal: bool,
    pub fog: bool,
    pub alpha: u5,
    not_used2: u3,
    pub id: u6,
    not_used3: u2,
}

#[bitsize(8)]
#[derive(Copy, Clone, Default, FromBits)]
pub struct SwapBuffers {
    pub manual_sort_translucent_polygon: bool,
    pub depth_buffering_w: bool,
    not_used: u6,
}

#[bitsize(8)]
#[derive(Copy, Clone, Default, FromBits)]
pub struct FifoParam {
    param_count: u6,
    is_test: bool,
    can_skip: bool,
}

const fn count(c: u8) -> u8 {
    assert!(c <= 32);
    c
}

const fn skip(c: u8) -> u8 {
    count(c) | (1 << 7)
}

const fn test(c: u8) -> u8 {
    count(c) | (1 << 6)
}

#[rustfmt::skip]
const FIFO_PARAM_COUNTS: [u8; 128] = [
    skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), // 0x00-0x0F
    count(1), count(0), count(1), count(1), count(1), count(0), count(16), count(12), count(16), count(12), count(9), count(3), count(3), skip(0), skip(0), skip(0), // 0x10-0x1F
    count(1), skip(1), skip(1), skip(2), skip(1), skip(1), skip(1), skip(1), skip(1), count(1), count(1), count(1), skip(0), skip(0), skip(0), skip(0), // 0x20-0x2F
    count(1), count(1), count(1), count(1), count(32), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), // 0x30-0x3F
    skip(1), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), // 0x40-0x4F
    count(1), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), // 0x50-0x5F
    count(1), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), // 0x60-0x6F
    test(3), test(2), test(1), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), skip(0), // 0x70-0x7F
];

const FUNC_GROUP_LUT: [fn(&mut Gpu3DRegisters, cmd: usize, params: &[u32; 32]); 8] = [
    Gpu3DRegisters::exe_empty_group,
    Gpu3DRegisters::exe_mat_group,
    Gpu3DRegisters::exe_vert_group,
    Gpu3DRegisters::exe_attr_group,
    Gpu3DRegisters::exe_begin_vtxs,
    Gpu3DRegisters::exe_swap_buffers,
    Gpu3DRegisters::exe_viewport,
    Gpu3DRegisters::exe_test_group,
];

#[derive(Copy, Clone, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum PolygonMode {
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

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
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

#[repr(C)]
#[derive(Default)]
struct Matrices {
    proj: Matrix,
    coord: Matrix,
    dir: Matrix,
    tex: Matrix,
    proj_stack: Matrix,
    coord_stack: [Matrix; 32],
    dir_stack: [Matrix; 32],
    tex_stack: Matrix,
}

#[derive(Copy, Clone, Default)]
pub struct Vertex {
    pub coords: Vectori32<4>,
    pub tex_coords: Vectori16<2>,
    pub color: u16,
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

#[bitsize(8)]
#[derive(Copy, Clone, Default, FromBits)]
struct MatrixFlags {
    clip_dirty: u2,
    tex_push: bool,
    clip_push: bool,
    unused: u4,
}

impl MatrixFlags {
    fn is_clip_dirty(self) -> bool {
        u8::from(self.clip_dirty()) != 0
    }

    fn set_clip_dirty_bool(&mut self, dirty: bool) {
        self.set_clip_dirty(u2::new(dirty as u8));
    }
}

#[derive(Default)]
pub struct Gpu3DRegisters {
    cmd_fifo: FixedFifo<u32, 1024>,
    cmd_remaining_params: u8,

    test_queue: u8,

    pub last_total_cycles: u32,
    pub flushed: bool,
    swap_buffers: SwapBuffers,

    pub gx_stat: GxStat,

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
    clip_matrices: MatrixVec,
    tex_matrices: MatrixVec,
    mtx_flags: MatrixFlags,

    cur_polygon_attr: PolygonAttr,

    material_color0: MaterialColor0,
    material_color1: MaterialColor1,

    pos_result: Vectori32<4>,
    vec_result: Vectori16<3>,

    pub skip: bool,
    pub consume: bool,
    pub pow_cnt1: u16,
    pub current_pow_cnt1: u16,
}

macro_rules! unpacked_cmd {
    ($name:ident, $cmd:expr) => {
        paste! {
            pub fn [<regs _ 3d _ set _ $name>](&mut self, mask: u32, value: u32) {
                self.regs_3d_queue_unpacked_value::<$cmd>(value & mask);
            }
        }
    };
}

impl Emu {
    pub fn regs_3d_run_cmds(&mut self, total_cycles: u32) {
        let regs_3d = &mut self.gpu.gpu_3d_regs;
        if unlikely(regs_3d.cmd_fifo.is_empty() || regs_3d.flushed) {
            regs_3d.last_total_cycles = total_cycles;
            return;
        }

        let is_cmd_fifo_half_full = regs_3d.is_cmd_fifo_half_full();

        let cycle_diff = total_cycles - regs_3d.last_total_cycles;
        regs_3d.last_total_cycles = total_cycles;
        let mut executed_cycles = 0;

        let mut params: [u32; 32] = unsafe { MaybeUninit::uninit().assume_init() };

        'outer: while !regs_3d.cmd_fifo.is_empty() {
            let value = *regs_3d.cmd_fifo.front();
            regs_3d.cmd_fifo.pop_front();

            for i in 0..4 {
                let cmd = ((value as usize) >> (i << 3)) & 0x7F;
                if unlikely(cmd == 0) {
                    break;
                }
                let param_count = FifoParam::from(unsafe { *FIFO_PARAM_COUNTS.get_unchecked(cmd) });
                let count = u8::from(param_count.param_count()) as usize;

                if unlikely(count > regs_3d.cmd_fifo.len()) {
                    regs_3d.cmd_fifo.push_front(value >> (i << 3));
                    break 'outer;
                }

                let skippable = param_count.can_skip();
                if !regs_3d.skip || likely(!skippable) {
                    for i in 0..count {
                        unsafe { *params.get_unchecked_mut(i) = *regs_3d.cmd_fifo.front() };
                        regs_3d.cmd_fifo.pop_front();
                    }

                    let func = unsafe { FUNC_GROUP_LUT.get_unchecked(cmd >> 4) };
                    func(regs_3d, cmd & 0xF, &params);
                } else {
                    regs_3d.cmd_fifo.pop_front_multiple(count);
                }

                executed_cycles += 4;
                if executed_cycles >= cycle_diff || cmd == 0x50 {
                    let remaining_cmds = value.unbounded_shr((i + 1) << 3);
                    if remaining_cmds != 0 {
                        regs_3d.cmd_fifo.push_front(remaining_cmds);
                    }
                    break 'outer;
                }
            }
        }

        if is_cmd_fifo_half_full && !regs_3d.is_cmd_fifo_half_full() {
            self.dma_trigger_all(ARM9, DmaTransferMode::GeometryCmdFifo);
        }

        let regs_3d = &mut self.gpu.gpu_3d_regs;
        let irq = u8::from(regs_3d.gx_stat.cmd_fifo_irq());
        if (irq == 1 && !regs_3d.is_cmd_fifo_half_full()) || (irq == 2 && regs_3d.is_cmd_fifo_empty()) {
            self.cpu_send_interrupt(ARM9, InterruptFlag::GeometryCmdFifo);
        }

        let regs_3d = &mut self.gpu.gpu_3d_regs;
        if !regs_3d.is_cmd_fifo_full() {
            self.cpu_unhalt(ARM9, 1);
        }

        let regs_3d = &mut self.gpu.gpu_3d_regs;
        if !regs_3d.skip && regs_3d.flushed {
            regs_3d.pow_cnt1 = regs_3d.current_pow_cnt1;
        }
    }

    #[inline(always)]
    fn regs_3d_post_queue_entry(&mut self) {
        if unlikely(self.gpu.gpu_3d_regs.is_cmd_fifo_full()) {
            self.breakout_imm = true;
            self.cpu_halt(ARM9, 1);
        }
    }

    pub fn regs_3d_get_clip_mtx_result(&mut self, index: usize) -> u32 {
        self.gpu.gpu_3d_regs.get_clip_matrix()[index] as u32
    }

    pub fn regs_3d_get_vec_mtx_result(&self, index: usize) -> u32 {
        self.gpu.gpu_3d_regs.matrices.dir[(index / 3) * 4 + index % 3] as u32
    }

    pub fn regs_3d_get_gx_stat(&self) -> u32 {
        let regs_3d = &self.gpu.gpu_3d_regs;
        let mut gx_stat = regs_3d.gx_stat;
        gx_stat.set_geometry_busy(!regs_3d.cmd_fifo.is_empty());
        gx_stat.set_num_entries_cmd_fifo(u9::new(regs_3d.get_cmd_fifo_len() as u16));
        gx_stat.set_cmd_fifo_less_half_full(!regs_3d.is_cmd_fifo_half_full());
        gx_stat.set_cmd_fifo_empty(regs_3d.is_cmd_fifo_empty());
        gx_stat.set_box_pos_vec_test_busy(regs_3d.test_queue != 0);
        u32::from(gx_stat)
    }

    pub fn regs_3d_get_ram_count(&self) -> u32 {
        ((self.gpu.gpu_3d_regs.vertices_size as u32) << 16) | (self.gpu.gpu_3d_regs.polygons_size as u32)
    }

    pub fn regs_3d_get_pos_result(&self, index: usize) -> u32 {
        self.gpu.gpu_3d_regs.pos_result[index] as u32
    }

    pub fn regs_3d_get_vec_result(&self, index: usize) -> u16 {
        self.gpu.gpu_3d_regs.vec_result[index] as u16
    }

    #[inline(always)]
    pub fn regs_3d_set_gx_fifo(&mut self, mask: u32, value: u32) {
        let value = value & mask;
        let regs_3d = &mut self.gpu.gpu_3d_regs;
        let mut remaining_params = regs_3d.cmd_remaining_params;
        let mut test_queue = regs_3d.test_queue;
        if remaining_params == 0 {
            for i in 0..4 {
                let cmd = ((value as usize) >> (i << 3)) & 0x7F;
                let param = unsafe { *FIFO_PARAM_COUNTS.get_unchecked(cmd) };
                remaining_params += param & 0x3F;
                test_queue += (param >> 6) & 1;
            }
        } else {
            remaining_params -= 1;
        }
        regs_3d.cmd_remaining_params = remaining_params;
        regs_3d.test_queue = test_queue;
        regs_3d.cmd_fifo.push_back(value);
        self.regs_3d_post_queue_entry();
    }

    #[inline(always)]
    pub fn regs_3d_set_gx_fifo_multiple(&mut self, values: &[u32]) {
        unsafe { assert_unchecked(!values.is_empty()) };
        let regs_3d = &mut self.gpu.gpu_3d_regs;
        let mut remaining_params = regs_3d.cmd_remaining_params;
        let mut test_queue = regs_3d.test_queue;
        let mut consumed = 0;
        while consumed < values.len() {
            if remaining_params == 0 {
                let value = values[consumed];
                for i in 0..4 {
                    let cmd = ((value as usize) >> (i << 3)) & 0x7F;
                    let param = unsafe { *FIFO_PARAM_COUNTS.get_unchecked(cmd) };
                    remaining_params += param & 0x3F;
                    test_queue += (param >> 6) & 1;
                }
                consumed += 1;
                regs_3d.cmd_fifo.push_back(value);
            }

            if unlikely(consumed >= values.len()) {
                break;
            }

            let values = &values[consumed..];

            let can_consume = min(remaining_params as usize, values.len());
            regs_3d.cmd_fifo.push_back_multiple(&values[..can_consume]);
            remaining_params -= can_consume as u8;
            consumed += can_consume;
        }
        regs_3d.cmd_remaining_params = remaining_params;
        regs_3d.test_queue = test_queue;
        self.regs_3d_post_queue_entry();
    }

    fn regs_3d_queue_unpacked_value<const CMD: u8>(&mut self, value: u32) {
        let regs_3d = &mut self.gpu.gpu_3d_regs;
        if regs_3d.cmd_remaining_params == 0 {
            regs_3d.cmd_remaining_params = u8::from(FifoParam::from(FIFO_PARAM_COUNTS[CMD as usize]).param_count());
            regs_3d.cmd_fifo.push_back(CMD as u32);
            if regs_3d.cmd_remaining_params > 0 {
                regs_3d.cmd_remaining_params -= 1;
                regs_3d.cmd_fifo.push_back(value);
            }

            match CMD {
                0x70 | 0x71 | 0x72 => regs_3d.test_queue += 1,
                _ => {}
            }
        } else {
            regs_3d.cmd_remaining_params -= 1;
            regs_3d.cmd_fifo.push_back(value);
        }
        self.regs_3d_post_queue_entry();
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

    pub fn regs_3d_set_gx_stat(&mut self, mut mask: u32, value: u32) {
        if value & (1 << 15) != 0 {
            self.gpu.gpu_3d_regs.gx_stat = (u32::from(self.gpu.gpu_3d_regs.gx_stat) & !0xA000).into();
        }

        mask &= 0xC0000000;
        self.gpu.gpu_3d_regs.gx_stat = ((u32::from(self.gpu.gpu_3d_regs.gx_stat) & !mask) | (value & mask)).into();
    }
}

impl Gpu3DRegisters {
    pub fn new() -> Self {
        let mut mtx_flags = MatrixFlags::from(0);
        mtx_flags.set_clip_push(true);
        mtx_flags.set_tex_push(true);
        Gpu3DRegisters {
            mtx_flags,
            ..Gpu3DRegisters::default()
        }
    }

    fn is_cmd_fifo_full(&self) -> bool {
        self.cmd_fifo.len() >= 260
    }

    pub fn is_cmd_fifo_half_full(&self) -> bool {
        self.cmd_fifo.len() >= 132
    }

    fn is_cmd_fifo_empty(&self) -> bool {
        self.cmd_fifo.len() <= 4
    }

    fn get_cmd_fifo_len(&self) -> usize {
        max(self.cmd_fifo.len() as isize - 4, 0) as usize
    }

    fn get_clip_matrix(&mut self) -> &Matrix {
        if unlikely(self.mtx_flags.is_clip_dirty()) {
            self.mtx_flags.set_clip_dirty_bool(false);
            self.mtx_flags.set_clip_push(true);
            unsafe { vmult_mat4(self.matrices.coord.vld(), self.matrices.proj.vld(), &mut self.clip_matrix.0) };
        }
        &self.clip_matrix
    }

    fn exe_mat_group(&mut self, cmd: usize, params: &[u32; 32]) {
        unsafe { assert_unchecked(cmd <= 0xF) };
        const FUNC_LUT: [fn(&mut Gpu3DRegisters, params: &[u32; 32]); 16] = [
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
        ];
        const FUNC_NAMES_LUT: [&str; 16] = [
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
        ];
        debug_println!("execute mat group {cmd:x}: {}", FUNC_NAMES_LUT[cmd]);
        FUNC_LUT[cmd](self, params);
    }

    fn exe_vert_group(&mut self, cmd: usize, params: &[u32; 32]) {
        unsafe { assert_unchecked(cmd <= 0xF) };
        const FUNC_LUT: [fn(&mut Gpu3DRegisters, params: &[u32; 32]); 16] = [
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
        ];
        const FUNC_NAMES_LUT: [&str; 16] = [
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
        ];
        debug_println!("execute vert group {cmd:x}: {}", FUNC_NAMES_LUT[cmd]);
        FUNC_LUT[cmd](self, params);
    }

    fn exe_attr_group(&mut self, cmd: usize, params: &[u32; 32]) {
        unsafe { assert_unchecked(cmd <= 0xF) };
        match cmd {
            0 => {
                debug_println!("execute attr group {cmd:x}: exe_dif_amb");
                self.exe_dif_amb(params);
            }
            1 => {
                debug_println!("execute attr group {cmd:x}: exe_spe_emi");
                self.exe_spe_emi(params);
            }
            2 => {
                debug_println!("execute attr group {cmd:x}: exe_light_vector");
                self.exe_light_vector(params);
            }
            3 => {
                debug_println!("execute attr group {cmd:x}: exe_light_color");
                self.exe_light_color(params);
            }
            4 => {
                debug_println!("execute attr group {cmd:x}: exe_shininess");
                self.exe_shininess(params);
            }
            _ => debug_println!("execute attr group {cmd:x}: exe_empty"),
        }
    }

    fn exe_test_group(&mut self, cmd: usize, params: &[u32; 32]) {
        unsafe { assert_unchecked(cmd <= 0xF) };
        match cmd {
            0 => {
                debug_println!("execute test group {cmd:x}: exe_box_test");
                self.exe_box_test(params);
            }
            1 => {
                debug_println!("execute test group {cmd:x}: exe_pos_test");
                self.exe_pos_test(params);
            }
            2 => {
                debug_println!("execute test group {cmd:x}: exe_vec_test");
                self.exe_vec_test(params);
            }
            _ => debug_println!("execute test group {cmd:x}: exe_empty"),
        }
    }

    fn exe_empty_group(&mut self, _: usize, _: &[u32; 32]) {}

    fn exe_empty(&mut self, _: &[u32; 32]) {}

    fn exe_mtx_mode(&mut self, params: &[u32; 32]) {
        self.mtx_mode = MtxMode::from((params[0] & 0x3) as u8);
    }

    fn exe_mtx_push(&mut self, _: &[u32; 32]) {
        match self.mtx_mode {
            MtxMode::Projection => {
                if u8::from(self.gx_stat.proj_mtx_stack_lvl()) == 0 {
                    self.matrices.proj_stack = self.matrices.proj.clone();
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
                    self.matrices.coord_stack[ptr as usize] = self.matrices.coord.clone();
                    self.matrices.dir_stack[ptr as usize] = self.matrices.dir.clone();
                    self.gx_stat.set_pos_vec_mtx_stack_lvl(u5::new(ptr + 1));
                }
            }
            MtxMode::Texture => self.matrices.tex_stack = self.matrices.tex.clone(),
        }
    }

    fn exe_mtx_pop(&mut self, params: &[u32; 32]) {
        match self.mtx_mode {
            MtxMode::Projection => {
                if u8::from(self.gx_stat.proj_mtx_stack_lvl()) == 1 {
                    self.matrices.proj = self.matrices.proj_stack.clone();
                    self.gx_stat.set_proj_mtx_stack_lvl(u1::new(0));
                    self.mtx_flags.set_clip_dirty_bool(true);
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
                    self.matrices.coord = self.matrices.coord_stack[ptr as usize].clone();
                    self.matrices.dir = self.matrices.dir_stack[ptr as usize].clone();
                    self.mtx_flags.set_clip_dirty_bool(true);
                }
            }
            MtxMode::Texture => {
                self.matrices.tex = self.matrices.tex_stack.clone();
                self.mtx_flags.set_tex_push(true);
            }
        }
    }

    fn exe_mtx_store(&mut self, params: &[u32; 32]) {
        match self.mtx_mode {
            MtxMode::Projection => self.matrices.proj_stack = self.matrices.proj.clone(),
            MtxMode::ModelView | MtxMode::ModelViewVec => {
                let addr = params[0] & 0x1F;

                if addr == 31 {
                    self.gx_stat.set_mtx_stack_overflow_underflow_err(true);
                }

                self.matrices.coord_stack[addr as usize] = self.matrices.coord.clone();
                self.matrices.dir_stack[addr as usize] = self.matrices.dir.clone();
            }
            MtxMode::Texture => self.matrices.tex_stack = self.matrices.tex.clone(),
        }
    }

    fn exe_mtx_restore(&mut self, params: &[u32; 32]) {
        let mode = self.mtx_mode as u8 + 1;
        self.mtx_flags.value |= mode; // Overflows to 4 when mode is texture, which sets the appropriate flags

        match self.mtx_mode {
            MtxMode::Projection => self.matrices.proj = self.matrices.proj_stack.clone(),
            MtxMode::ModelView | MtxMode::ModelViewVec => {
                let addr = params[0] & 0x1F;

                if addr == 31 {
                    self.gx_stat.set_mtx_stack_overflow_underflow_err(true);
                }

                self.matrices.coord = self.matrices.coord_stack[addr as usize].clone();
                self.matrices.dir = self.matrices.dir_stack[addr as usize].clone();
            }
            MtxMode::Texture => self.matrices.tex = self.matrices.tex_stack.clone(),
        }
    }

    fn exe_mtx_identity(&mut self, _: &[u32; 32]) {
        let mode = self.mtx_mode as u8 + 1;
        self.mtx_flags.value |= mode;

        let dst = unsafe { ptr::addr_of_mut!(self.matrices.proj).add(self.mtx_mode as usize).as_mut_unchecked() };
        unsafe {
            let mtx = Matrix::vld_identity();
            vst1q_s32_x4(dst.0.as_mut_ptr(), mem::transmute(mtx));
            if unlikely(self.mtx_mode == MtxMode::ModelViewVec) {
                vst1q_s32_x4(self.matrices.coord.0.as_mut_ptr(), mem::transmute(mtx));
            }
        }
    }

    fn exe_mtx_load44(&mut self, params: &[u32; 32]) {
        let mode = self.mtx_mode as u8 + 1;
        self.mtx_flags.value |= mode;

        let params: &[u32; 16] = unsafe { mem::transmute(params) };
        let mtx: &Matrix = unsafe { mem::transmute(params) };

        let dst = unsafe { ptr::addr_of_mut!(self.matrices.proj).add(self.mtx_mode as usize).as_mut_unchecked() };
        unsafe {
            let mtx = mtx.vld();
            vst1q_s32_x4(dst.0.as_mut_ptr(), mem::transmute(mtx));
            if unlikely(self.mtx_mode == MtxMode::ModelViewVec) {
                vst1q_s32_x4(self.matrices.coord.0.as_mut_ptr(), mem::transmute(mtx));
            }
        }
    }

    fn exe_mtx_load43(&mut self, params: &[u32; 32]) {
        let mode = self.mtx_mode as u8 + 1;
        self.mtx_flags.value |= mode;

        let load = |mtx: &mut Matrix| {
            for i in 0..4 {
                mtx.as_mut()[i * 4..i * 4 + 3].copy_from_slice(unsafe { mem::transmute(&params[i * 3..i * 3 + 3]) });
            }
            mtx[3] = 0;
            mtx[7] = 0;
            mtx[11] = 0;
            mtx[15] = 1 << 12;
        };

        let dst = unsafe { ptr::addr_of_mut!(self.matrices.proj).add(self.mtx_mode as usize).as_mut_unchecked() };
        load(dst);
        if unlikely(self.mtx_mode == MtxMode::ModelViewVec) {
            load(&mut self.matrices.coord);
        }
    }

    #[inline(always)]
    fn mtx_mult(&mut self, mtx: [int32x4_t; 4]) {
        let mode = self.mtx_mode as u8 + 1;
        self.mtx_flags.value |= mode;

        let dst = unsafe { ptr::addr_of_mut!(self.matrices.proj).add(self.mtx_mode as usize).as_mut_unchecked() };
        unsafe { vmult_mat4(mtx, dst.vld(), &mut dst.0) };
        if unlikely(self.mtx_mode == MtxMode::ModelViewVec) {
            unsafe { vmult_mat4(mtx, self.matrices.coord.vld(), &mut self.matrices.coord.0) };
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
        let mode = self.mtx_mode as u8 + 1;
        self.mtx_flags.value |= mode;

        static mut SCALE_MTX: [i32; 16] = MTX_IDENTITY;
        unsafe {
            SCALE_MTX[0] = params[0] as i32;
            SCALE_MTX[5] = params[1] as i32;
            SCALE_MTX[10] = params[2] as i32;
            let mtx = vld1q_s32_x4(SCALE_MTX.as_ptr());
            let rm = if self.mtx_mode == MtxMode::ModelViewVec {
                &mut self.matrices.coord
            } else {
                ptr::addr_of_mut!(self.matrices.proj).add(self.mtx_mode as usize).as_mut_unchecked()
            };
            vmult_mat4(mem::transmute(mtx), rm.vld(), &mut rm.0);
        }
    }

    fn exe_mtx_trans(&mut self, params: &[u32; 32]) {
        unsafe {
            (params.as_ptr().add(3) as *mut u32).write_volatile(1 << 12);
            let mtx = vld1q_s32_x3(MTX_IDENTITY.as_ptr());
            let trans_vector = vld1q_s32(params.as_ptr() as _);
            self.mtx_mult([mtx.0, mtx.1, mtx.2, trans_vector]);
        }
    }

    fn exe_color(&mut self, params: &[u32; 32]) {
        self.cur_vtx.color = params[0] as u16;
    }

    fn exe_normal(&mut self, params: &[u32; 32]) {
        let normal_vector_param = NormalVector::from(params[0]);
        self.cur_polygon.normal[0] = ((u16::from(normal_vector_param.x()) << 6) as i16) >> 3;
        self.cur_polygon.normal[1] = ((u16::from(normal_vector_param.y()) << 6) as i16) >> 3;
        self.cur_polygon.normal[2] = ((u16::from(normal_vector_param.z()) << 6) as i16) >> 3;

        if self.cur_vtx.tex_coord_trans_mode == TextureCoordTransMode::Normal && self.mtx_flags.tex_push() {
            self.tex_matrices.push(&self.matrices.tex);
            self.mtx_flags.set_tex_push(false);
        }
        self.cur_vtx.tex_matrix_index = (self.tex_matrices.len() as u16).wrapping_sub(1);
    }

    fn exe_tex_coord(&mut self, params: &[u32; 32]) {
        let tex_coord = TexCoord::from(params[0]);
        self.cur_vtx.tex_coords[0] = tex_coord.s() as i16;
        self.cur_vtx.tex_coords[1] = tex_coord.t() as i16;

        if self.cur_vtx.tex_coord_trans_mode == TextureCoordTransMode::TexCoord && self.mtx_flags.tex_push() {
            self.tex_matrices.push(&self.matrices.tex);
            self.mtx_flags.set_tex_push(false);
        }
        self.cur_vtx.tex_matrix_index = (self.tex_matrices.len() as u16).wrapping_sub(1);
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
        self.cur_vtx.tex_coord_trans_mode = TextureCoordTransMode::from(u8::from(self.cur_polygon.tex_image_param.coord_trans_mode()));
    }

    fn exe_pltt_base(&mut self, params: &[u32; 32]) {
        self.cur_polygon.palette_addr = (params[0] & 0x1FFF) as u16;
    }

    fn exe_dif_amb(&mut self, params: &[u32; 32]) {
        self.material_color0 = MaterialColor0::from(params[0]);

        if self.material_color0.set_vertex_color() {
            self.cur_vtx.color = u16::from(self.material_color0.dif());
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

    fn exe_begin_vtxs(&mut self, cmd: usize, params: &[u32; 32]) {
        if unlikely(cmd != 0) {
            return;
        }

        debug_println!("execute exe_begin_vtxs {cmd:x}");

        if self.vertex_list_size < self.vertex_list_primitive_type.vertex_count() as u16 {
            self.vertices_size -= self.vertex_list_size;
        }

        self.vertex_list_primitive_type = PrimitiveType::from((params[0] & 0x3) as u8);
        self.vertex_list_size = 0;
        self.cur_polygon.attr = self.cur_polygon_attr;
        self.cur_polygon.viewport = self.cur_viewport;
    }

    fn exe_swap_buffers(&mut self, cmd: usize, params: &[u32; 32]) {
        if unlikely(cmd != 0) {
            return;
        }

        debug_println!("execute exe_swap_buffers {cmd:x}");

        self.flushed = true;
        self.swap_buffers = SwapBuffers::from(params[0] as u8)
    }

    fn exe_viewport(&mut self, cmd: usize, params: &[u32; 32]) {
        if unlikely(cmd != 0) {
            return;
        }

        debug_println!("execute exe_viewport {cmd:x}");

        self.cur_viewport = Viewport::from(params[0]);
    }

    fn exe_box_test(&mut self, _: &[u32; 32]) {
        self.gx_stat.set_box_test_result(true);

        self.test_queue -= 1;
    }

    fn exe_pos_test(&mut self, params: &[u32; 32]) {
        self.cur_vtx.coords[0] = params[0] as i16 as i32;
        self.cur_vtx.coords[1] = (params[0] >> 16) as i16 as i32;
        self.cur_vtx.coords[2] = params[1] as i16 as i32;
        self.cur_vtx.coords[3] = 1 << 12;
        self.pos_result = self.cur_vtx.coords * self.get_clip_matrix();

        self.test_queue -= 1;
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

        self.test_queue -= 1;
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
        self.mtx_flags.set_clip_push(true);

        mem::swap(&mut self.tex_matrices, &mut content.tex_matrices);
        self.tex_matrices.clear();
        self.mtx_flags.set_tex_push(true);

        content.swap_buffers = self.swap_buffers;
        content.pow_cnt1 = self.pow_cnt1;
    }

    fn add_vertex(&mut self) {
        if self.vertices_size >= VERTEX_LIMIT as u16 {
            return;
        }

        self.get_clip_matrix();
        if unlikely(self.mtx_flags.clip_push()) {
            self.clip_matrices.push(&self.clip_matrix);
            self.mtx_flags.set_clip_push(false);
        }

        if self.cur_vtx.tex_coord_trans_mode == TextureCoordTransMode::Vertex && unlikely(self.mtx_flags.tex_push()) {
            self.tex_matrices.push(&self.matrices.tex);
            self.mtx_flags.set_tex_push(false);
        }
        self.cur_vtx.tex_matrix_index = (self.tex_matrices.len() as u16).wrapping_sub(1);

        unsafe {
            *self.vertices.get_unchecked_mut(self.vertices_size as usize) = self.cur_vtx;
            self.vertices.get_unchecked_mut(self.vertices_size as usize).clip_matrix_index = self.clip_matrices.len() as u16 - 1;
        }
        self.vertices_size += 1;
        self.vertex_list_size += 1;

        if unlikely(match self.vertex_list_primitive_type {
            PrimitiveType::SeparateTriangles => self.vertex_list_size % 3 == 0,
            PrimitiveType::SeparateQuadliterals => self.vertex_list_size % 4 == 0,
            PrimitiveType::TriangleStrips => self.vertex_list_size >= 3,
            PrimitiveType::QuadliteralStrips => self.vertex_list_size >= 4 && self.vertex_list_size % 2 == 0,
        }) {
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
}
