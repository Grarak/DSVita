use crate::core::graphics::gl_utils::GpuFbo;
use crate::core::graphics::gpu::{PowCnt1, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_3d::registers_3d::{Gpu3DBuffer, Gpu3DRegisters, PolygonAttr, PolygonMode, PrimitiveType, TexImageParam, TextureCoordTransMode, TextureFormat, Vertex, Viewport};
use crate::core::graphics::gpu_3d::registers_3d::{POLYGON_LIMIT, VERTEX_LIMIT};
use crate::core::graphics::gpu_3d::texture_cache::{Texture3D, Texture3DCache};
use crate::core::graphics::gpu_mem_buf::{GpuMemBuf, GpuMemRefs};
use crate::core::graphics::gpu_renderer::GpuRendererCommon;
use crate::core::graphics::gpu_shaders::{Gpu3DShaderDepthPrograms, Gpu3DShaderProgram, GpuShadersPrograms};
use crate::core::memory::vram;
use crate::math::{vmult_vec4_mat4_no_store, Vectori32};
use crate::utils::{rgb5_to_float8, HeapArray, HeapArrayU16, HeapArrayU8, HeapMem, PtrWrapper};
use bilge::prelude::*;
use gl::types::GLuint;
use static_assertions::const_assert_eq;
use std::arch::arm::{vcvt_n_f32_s32, vcvtq_n_f32_s32, vget_low_s32, vsetq_lane_s32, vshr_n_s32, vst1_f32, vst1q_f32};
use std::hint::{assert_unchecked, unreachable_unchecked};
use std::intrinsics::unlikely;
use std::mem::{self, MaybeUninit};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

pub const WIDTH_3D: usize = DISPLAY_WIDTH;
pub const HEIGHT_3D: usize = DISPLAY_HEIGHT;
pub const UPSCALE_WIDTH_3D: usize = DISPLAY_WIDTH * 2;
pub const UPSCALE_HEIGHT_3D: usize = DISPLAY_HEIGHT * 2;

#[bitsize(32)]
#[derive(Copy, Clone, FromBits)]
struct ClearColor {
    color: u15,
    fog: bool,
    alpha: u5,
    not_used: u3,
    clear_polygon_id: u6,
    not_used1: u2,
}

impl Default for ClearColor {
    fn default() -> Self {
        ClearColor::from(0)
    }
}

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

impl Default for Disp3DCnt {
    fn default() -> Self {
        Disp3DCnt::from(0)
    }
}

#[derive(Clone)]
struct Gpu3DRendererInner {
    disp_cnt: Disp3DCnt,
    edge_colors: [u16; 8],
    clear_color: ClearColor,
    clear_colorf: [f32; 4],
    clear_depth: u16,
    clear_depthf: f32,
    fog_color: u32,
    fog_offset: u16,
    fog_table: [u8; 32],
    toon_table: [u16; 32],
}

impl Default for Gpu3DRendererInner {
    fn default() -> Self {
        Gpu3DRendererInner {
            disp_cnt: Default::default(),
            edge_colors: [0; 8],
            clear_color: Default::default(),
            clear_colorf: [0.0; 4],
            clear_depth: 0x7FFF,
            clear_depthf: 1.0,
            fog_color: 0,
            fog_offset: 0,
            fog_table: [0; 32],
            toon_table: [0; 32],
        }
    }
}

pub struct Gpu3DGl {
    vertices_buf: GLuint,
    program: Gpu3DShaderDepthPrograms,
    top_fbo: [GpuFbo; 2],
    bottom_fbo: [GpuFbo; 2],
}

impl Gpu3DGl {
    fn new(gpu_programs: &GpuShadersPrograms) -> Self {
        unsafe {
            let mut vertices_buf = 0;
            gl::GenBuffers(1, &mut vertices_buf);
            gl::BindBuffer(gl::ARRAY_BUFFER, vertices_buf);

            gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::UseProgram(0);

            Gpu3DGl {
                vertices_buf,
                program: gpu_programs.render_3d,
                top_fbo: [
                    GpuFbo::new(WIDTH_3D as _, HEIGHT_3D as _, true, true).unwrap(),
                    GpuFbo::new(UPSCALE_WIDTH_3D as _, UPSCALE_HEIGHT_3D as _, true, true).unwrap(),
                ],
                bottom_fbo: [
                    GpuFbo::new(WIDTH_3D as _, HEIGHT_3D as _, true, true).unwrap(),
                    GpuFbo::new(UPSCALE_WIDTH_3D as _, UPSCALE_HEIGHT_3D as _, true, true).unwrap(),
                ],
            }
        }
    }
}

#[derive(Default)]
#[repr(C)]
struct Gpu3dPolygonAttr {
    tex_image_param: u32,
    pal_addr_poly_attr: u32,
}

const_assert_eq!(size_of::<Gpu3dPolygonAttr>(), 8);

#[derive(Default)]
struct Gpu3DTexMem {
    tex: HeapArrayU8<{ vram::TEX_REAR_PLANE_IMAGE_SIZE }>,
    pal: HeapArrayU8<{ vram::TEX_PAL_SIZE }>,
    vertices_buf: HeapArray<Gpu3DVertex, VERTEX_LIMIT>,
}

#[derive(Default, Clone)]
#[repr(C)]
struct Gpu3DVertex {
    coords: [f32; 4],
    tex_coords: [f32; 2],
    viewport: [u8; 4],
    color: [u8; 4],
    tex_size: [u8; 2],
}

#[bitsize(32)]
#[derive(Copy, Clone, DebugBits, Default, FromBits)]
struct Gpu3DDrawAttr {
    mode: PolygonMode,
    render_back: bool,
    render_front: bool,
    trans_new_depth: bool,
    render_far_plane: bool,
    render_1_dot_polygons: bool,
    depth_test_equal: bool,
    fog: bool,
    id: u6,
    not_used: u1,
    pal_addr: u16,
}

impl From<PolygonAttr> for Gpu3DDrawAttr {
    fn from(value: PolygonAttr) -> Self {
        Gpu3DDrawAttr::new(
            value.mode(),
            value.render_back(),
            value.render_front(),
            value.trans_new_depth(),
            value.render_far_plane(),
            value.render_1_dot_polygons(),
            value.depth_test_equal(),
            value.fog(),
            value.id(),
            u1::new(0),
            0,
        )
    }
}

#[derive(Default)]
pub struct Gpu3DDraw {
    vertex_start_index: u16,
    vertex_count: u16,
    attr: PolygonAttr,
    pub tex_image_param: TexImageParam,
    pub pal_addr: u16,
    viewport: Viewport,
    texture_3d_ptr: *mut Texture3D,
}

impl Gpu3DDraw {
    pub fn key(&self) -> u64 {
        self.tex_image_param.key() as u64 | ((self.pal_addr as u64) << 32)
    }
}

struct IndicesBatch {
    indices_offset: usize,
    tex: GLuint,
    tex_image_param: TexImageParam,
    attr: Gpu3DDrawAttr,
}

pub struct Gpu3DRenderer {
    pub dirty: bool,

    inners: [Gpu3DRendererInner; 2],
    buffer: HeapMem<Gpu3DBuffer>,
    gl: Gpu3DGl,

    assembled_draws: HeapArray<Gpu3DDraw, POLYGON_LIMIT>,
    assembled_draw_count: u16,

    translucent_polygons: Vec<u16>,

    vertices_buf: PtrWrapper<[Gpu3DVertex; VERTEX_LIMIT]>,
    vertices_buf_count: u16,

    indices_opaque: Vec<u16>,
    indices_translucent: Vec<u16>,

    polygon_vertices_mapping: HeapArrayU16<POLYGON_LIMIT>,

    texture_cache: Texture3DCache,
    texture_ids_to_delete: Vec<GLuint>,
    active_texture_id: GLuint,
    active_polygon_attr: Gpu3DDrawAttr,
    active_tex_image_param: TexImageParam,
    indices_opaque_batches: Vec<IndicesBatch>,
    indices_translucent_batches: Vec<IndicesBatch>,
    vram_ready: AtomicBool,

    mem: Gpu3DTexMem,
}

impl Gpu3DRenderer {
    pub fn new(gpu_programs: &GpuShadersPrograms) -> Self {
        Gpu3DRenderer {
            dirty: false,
            inners: [Gpu3DRendererInner::default(), Gpu3DRendererInner::default()],
            buffer: Default::default(),
            gl: Gpu3DGl::new(gpu_programs),

            assembled_draws: HeapArray::default(),
            assembled_draw_count: 0,

            translucent_polygons: Vec::new(),

            #[cfg(target_os = "linux")]
            vertices_buf: PtrWrapper::null(),
            #[cfg(target_os = "vita")]
            vertices_buf: unsafe { PtrWrapper::new(crate::presenter::Presenter::gl_mem_align_ram(16, size_of::<Gpu3DVertex>() * VERTEX_LIMIT) as _) },
            vertices_buf_count: 0,

            indices_opaque: Vec::new(),
            indices_translucent: Vec::new(),

            polygon_vertices_mapping: Default::default(),

            texture_ids_to_delete: Vec::new(),
            texture_cache: Texture3DCache::new(),
            active_texture_id: u32::MAX,
            active_polygon_attr: Gpu3DDrawAttr::default(),
            active_tex_image_param: TexImageParam::default(),
            indices_opaque_batches: Vec::new(),
            indices_translucent_batches: Vec::new(),
            vram_ready: AtomicBool::new(false),

            mem: Default::default(),
        }
    }

    pub fn init(&mut self) {
        self.dirty = false;
        self.inners[0] = Gpu3DRendererInner::default();
        self.inners[1] = Gpu3DRendererInner::default();
        self.buffer.reset_all();
        self.buffer.pow_cnt1 = PowCnt1::from(0);
        self.texture_cache.clear();

        unsafe {
            for fbo in [self.gl.top_fbo[0].fbo, self.gl.bottom_fbo[0].fbo] {
                gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
                gl::Viewport(0, 0, WIDTH_3D as _, HEIGHT_3D as _);
                gl::ClearColor(0.0, 0.0, 0.0, 0.0);

                gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT);
            }

            for fbo in [self.gl.top_fbo[1].fbo, self.gl.bottom_fbo[1].fbo] {
                gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
                gl::Viewport(0, 0, UPSCALE_WIDTH_3D as _, UPSCALE_HEIGHT_3D as _);
                gl::ClearColor(0.0, 0.0, 0.0, 0.0);

                gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT);
            }

            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }
    }

    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    pub fn get_disp_3d_cnt(&self) -> u16 {
        self.inners[1].disp_cnt.into()
    }

    pub fn set_disp_3d_cnt(&mut self, mut mask: u16, value: u16) {
        let new_cnt = Disp3DCnt::from(value);
        if new_cnt.color_buf_rdlines_underflow() {
            self.inners[1].disp_cnt.set_color_buf_rdlines_underflow(false);
        }
        if new_cnt.polygon_vertex_ram_overflow() {
            self.inners[1].disp_cnt.set_polygon_vertex_ram_overflow(false);
        }

        mask &= 0x4FFF;
        let new_value = (u16::from(self.inners[1].disp_cnt) & !mask) | (value & mask);
        if u16::from(self.inners[1].disp_cnt) != new_value {
            self.inners[1].disp_cnt = new_value.into();
            self.invalidate();
        }
    }

    pub fn set_edge_color(&mut self, index: usize, mut mask: u16, value: u16) {
        mask &= 0x7FFF;
        if value & mask == self.inners[1].edge_colors[index] & mask {
            return;
        }
        self.inners[1].edge_colors[index] = (self.inners[1].edge_colors[index] & !mask) | (value & mask);
        self.invalidate();
    }

    pub fn set_clear_color(&mut self, mut mask: u32, value: u32) {
        mask &= 0x3F1FFFFF;
        if value & mask == self.inners[1].clear_color.value & mask {
            return;
        }
        self.inners[1].clear_color.value = (self.inners[1].clear_color.value & !mask) | (value & mask);
        let [r, g, b] = rgb5_to_float8(u16::from(self.inners[1].clear_color.color()));
        self.inners[1].clear_colorf = [r, g, b, u8::from(self.inners[1].clear_color.alpha()) as f32 / 31f32];
        self.invalidate();
    }

    pub fn set_clear_depth(&mut self, mut mask: u16, value: u16) {
        mask &= 0x7FFF;
        if value & mask == self.inners[1].clear_depth & mask {
            return;
        }
        self.inners[1].clear_depth = (self.inners[1].clear_depth & !mask) | (value & mask);
        let depth = self.inners[1].clear_depth as u32;
        let expanded_depth = depth * 0x200 + ((depth + 1) / 0x8000) * 0x1FF;
        self.inners[1].clear_depthf = expanded_depth as f32 / 0xFFFFFF as f32;
        const TOLERANCE: f32 = 0x200 as f32 / 0xFFFFFF as f32;
        self.inners[1].clear_depthf += TOLERANCE;
        if self.inners[1].clear_depthf > 1.0 {
            self.inners[1].clear_depthf = 1.0;
        }
        self.invalidate();
    }

    pub fn set_toon_table(&mut self, index: usize, mut mask: u16, value: u16) {
        mask &= 0x7FFF;
        if value & mask == self.inners[1].toon_table[index] & mask {
            return;
        }
        self.inners[1].toon_table[index] = (self.inners[1].toon_table[index] & !mask) | (value & mask);
        self.invalidate();
    }

    pub fn set_fog_color(&mut self, mut mask: u32, value: u32) {
        mask &= 0x001F7FFF;
        if value & mask == self.inners[1].fog_color & mask {
            return;
        }
        self.inners[1].fog_color = (self.inners[1].fog_color & !mask) | (value & mask);
        self.invalidate();
    }

    pub fn set_fog_offset(&mut self, mut mask: u16, value: u16) {
        mask &= 0x7FFF;
        if value & mask == self.inners[1].fog_offset & mask {
            return;
        }
        self.inners[1].fog_offset = (self.inners[1].fog_offset & !mask) | (value & mask);
        self.invalidate();
    }

    pub fn set_fog_table(&mut self, index: usize, value: u8) {
        if value & 0x7F == self.inners[1].fog_table[index] & 0x7F {
            return;
        }
        self.inners[1].fog_table[index] = value & 0x7F;
        self.invalidate();
    }

    pub fn finish_scanline(&mut self, registers: &mut Gpu3DRegisters) {
        self.inners[0] = self.inners[1].clone();

        if registers.can_consume() {
            registers.swap_to_renderer(&mut self.buffer);
        }
    }

    unsafe fn process_vertices(&mut self) {
        let mut clip_matrix_index = usize::MAX;
        let mut clip_matrix = MaybeUninit::uninit().assume_init();

        for i in 0..self.buffer.vertices_count {
            let vertex: &mut Vertex = mem::transmute(self.buffer.vertices.get_unchecked_mut(i as usize));
            let coords = vertex.coords.fixed.vld();

            assert_unchecked(vertex.s.indices.clip_matrix as usize != usize::MAX);
            if clip_matrix_index != vertex.s.indices.clip_matrix as usize {
                clip_matrix_index = vertex.s.indices.clip_matrix as usize;
                clip_matrix = self.buffer.clip_matrices[clip_matrix_index].vld();
            }
            let trans_coords = vmult_vec4_mat4_no_store(coords, clip_matrix);
            let trans_coords_float = vcvtq_n_f32_s32::<12>(trans_coords);
            vst1q_f32(vertex.coords.float.0.as_mut_ptr(), trans_coords_float);

            let tex_coord_trans_mode = vertex.data.coord_trans_mode();
            if tex_coord_trans_mode != TextureCoordTransMode::None && (vertex.s.indices.tex_matrix as usize) < self.buffer.tex_matrices.len() {
                let mut tex_matrix = self.buffer.tex_matrices[vertex.s.indices.tex_matrix as usize].vld();

                let ret = match tex_coord_trans_mode {
                    TextureCoordTransMode::TexCoord => {
                        let vector = Vectori32::<4>::new([(vertex.s.indices.tex_coords[0] as i32) << 8, (vertex.s.indices.tex_coords[1] as i32) << 8, 1 << 8, 1 << 8]);
                        let ret = vmult_vec4_mat4_no_store(vector.vld(), tex_matrix);
                        vshr_n_s32::<8>(vget_low_s32(ret))
                    }
                    TextureCoordTransMode::Normal => {
                        tex_matrix[3] = vsetq_lane_s32::<0>((vertex.s.indices.tex_coords[0] as i32) << 12, tex_matrix[3]);
                        tex_matrix[3] = vsetq_lane_s32::<1>((vertex.s.indices.tex_coords[1] as i32) << 12, tex_matrix[3]);
                        let normal = Vectori32::<4>::new([vertex.normal[0] as i32, vertex.normal[1] as i32, vertex.normal[2] as i32, 1 << 12]);
                        let ret = vmult_vec4_mat4_no_store(normal.vld(), tex_matrix);
                        vshr_n_s32::<12>(vget_low_s32(ret))
                    }
                    TextureCoordTransMode::Vertex => {
                        tex_matrix[3] = vsetq_lane_s32::<0>((vertex.s.indices.tex_coords[0] as i32) << 12, tex_matrix[3]);
                        tex_matrix[3] = vsetq_lane_s32::<1>((vertex.s.indices.tex_coords[1] as i32) << 12, tex_matrix[3]);
                        let ret = vmult_vec4_mat4_no_store(trans_coords, tex_matrix);
                        vshr_n_s32::<12>(vget_low_s32(ret))
                    }
                    _ => unreachable_unchecked(),
                };

                let ret = vcvt_n_f32_s32::<4>(ret);
                vst1_f32(vertex.s.trans_tex_coords.0.as_mut_ptr(), ret);
            } else {
                let tex_coords = vertex.s.indices.tex_coords;
                vertex.s.trans_tex_coords[0] = tex_coords[0] as f32 / 16.0;
                vertex.s.trans_tex_coords[1] = tex_coords[1] as f32 / 16.0;
            }
        }
    }

    unsafe fn assemble_draws(&mut self) {
        self.assembled_draw_count = 0;

        let add_draw = |instance: &mut Self, vertex_start_index, vertex_count, polygon_attr, tex_image_param, pal_addr, viewport| {
            *instance.assembled_draws.get_unchecked_mut(instance.assembled_draw_count as usize) = Gpu3DDraw {
                vertex_start_index,
                vertex_count,
                attr: polygon_attr,
                tex_image_param,
                pal_addr,
                viewport,
                texture_3d_ptr: ptr::null_mut(),
            };

            instance.assembled_draw_count += 1;
            instance.assembled_draw_count != POLYGON_LIMIT as u16
        };

        let mut viewport = MaybeUninit::uninit().assume_init();
        let mut polygon_attr: PolygonAttr = MaybeUninit::uninit().assume_init();
        let mut draw_vertex_count: u16 = 0;
        let mut tex_image_param = MaybeUninit::uninit().assume_init();
        let mut pal_addr = MaybeUninit::uninit().assume_init();

        for i in 0..self.buffer.vertices_count {
            let vertex = self.buffer.vertices.get_unchecked(i as usize);
            assert_unchecked(i != 0 || vertex.data.begin_vtxs());

            let begin_vtxs = vertex.data.begin_vtxs();
            let polygon_index = u16::from(vertex.data.polygon_index());

            if begin_vtxs {
                let draw_complete = match polygon_attr.primitive_type() {
                    PrimitiveType::TriangleStrips => draw_vertex_count >= 3,
                    PrimitiveType::QuadliteralStrips => draw_vertex_count >= 4 && draw_vertex_count % 2 == 0,
                    _ => false,
                };
                if draw_complete && !add_draw(self, i - draw_vertex_count, draw_vertex_count, polygon_attr, tex_image_param, pal_addr, viewport) {
                    return;
                }

                let polygon = self.buffer.polygons.get_unchecked(polygon_index as usize);
                viewport = polygon.viewport;
                polygon_attr = polygon.attr;
                draw_vertex_count = 0;
                tex_image_param = polygon.tex_image_param;
                pal_addr = polygon.palette_addr;
            }

            draw_vertex_count += 1;
            let draw_complete = match polygon_attr.primitive_type() {
                PrimitiveType::SeparateTriangles => {
                    let ret = draw_vertex_count == 3;
                    if ret {
                        draw_vertex_count = 0;
                    }
                    ret
                }
                PrimitiveType::SeparateQuadliterals => draw_vertex_count % 4 == 0,
                _ => false,
            };
            if draw_complete {
                let polygon = self.buffer.polygons.get_unchecked(polygon_index as usize);
                if !add_draw(
                    self,
                    i + 1 - polygon_attr.primitive_type().vertex_count() as u16,
                    polygon_attr.primitive_type().vertex_count() as u16,
                    polygon_attr,
                    polygon.tex_image_param,
                    polygon.palette_addr,
                    viewport,
                ) {
                    return;
                }
            }
        }

        let draw_complete = match polygon_attr.primitive_type() {
            PrimitiveType::TriangleStrips => draw_vertex_count >= 3,
            PrimitiveType::QuadliteralStrips => draw_vertex_count >= 4 && draw_vertex_count % 2 == 0,
            _ => false,
        };
        if draw_complete {
            add_draw(
                self,
                self.buffer.vertices_count - draw_vertex_count,
                draw_vertex_count,
                polygon_attr,
                tex_image_param,
                pal_addr,
                viewport,
            );
        }
    }

    unsafe fn add_indices_batch<const TRANSLUCENT_ONLY: bool>(&mut self) {
        let (indices_len, indices_batch) = if TRANSLUCENT_ONLY {
            (self.indices_translucent.len(), &mut self.indices_translucent_batches)
        } else {
            (self.indices_opaque.len(), &mut self.indices_opaque_batches)
        };
        if indices_len != 0 {
            indices_batch.push(IndicesBatch {
                indices_offset: indices_len,
                tex: self.active_texture_id,
                tex_image_param: self.active_tex_image_param,
                attr: self.active_polygon_attr,
            });
        }
    }

    unsafe fn add_vertices<const TRANSLUCENT_ONLY: bool>(&mut self, draw_index: u16) {
        assert_unchecked((draw_index as usize) < POLYGON_LIMIT);
        let draw = &self.assembled_draws[draw_index as usize];
        let primitive_type = draw.attr.primitive_type();

        // println!(
        //     "renderer: translucent only {TRANSLUCENT_ONLY} polygon {polygon_index} type {:?} pal addr {:x} tex image param {:?} attr {:?}",
        //     polygon.polygon_type,
        //     (polygon.palette_addr as u32) << 3,
        //     polygon.tex_image_param,
        //     polygon.attr
        // );

        let texture_id = if draw.tex_image_param.format() != TextureFormat::None {
            draw.texture_3d_ptr.as_mut_unchecked().get_texture_id()
        } else {
            u32::MAX
        };

        let draw_attr = Gpu3DDrawAttr::from(draw.attr);
        let tex_image_param = u32::from(draw.tex_image_param) & 0x1C0F0000;
        if self.active_texture_id != texture_id || u32::from(self.active_tex_image_param) != tex_image_param || self.active_polygon_attr.value != draw_attr.value {
            self.add_indices_batch::<TRANSLUCENT_ONLY>();
            self.active_texture_id = texture_id;
            self.active_tex_image_param = TexImageParam::from(tex_image_param);
            self.active_polygon_attr = draw_attr;
        }

        let draw = &self.assembled_draws[draw_index as usize];

        let push_indices = |indices_buf: &mut Vec<u16>, vertex_index: u16, vertex_count: u16| match primitive_type {
            PrimitiveType::SeparateTriangles => indices_buf.extend(&[vertex_index, vertex_index + 1, vertex_index + 2]),
            PrimitiveType::SeparateQuadliterals => indices_buf.extend(&[vertex_index, vertex_index + 1, vertex_index + 2, vertex_index, vertex_index + 2, vertex_index + 3]),
            PrimitiveType::TriangleStrips => {
                indices_buf.extend(&[vertex_index, vertex_index + 1, vertex_index + 2]);
                for i in 3..vertex_count {
                    let vertex_index = i + vertex_index;
                    indices_buf.extend(&[vertex_index - 2, vertex_index - (!i & 1), vertex_index - (i & 1)]);
                }
            }
            PrimitiveType::QuadliteralStrips => {
                indices_buf.extend(&[vertex_index, vertex_index + 1, vertex_index + 3, vertex_index, vertex_index + 3, vertex_index + 2]);
                for i in (vertex_index + 4..vertex_index + vertex_count).step_by(2) {
                    indices_buf.extend(&[i - 2, i - 1, i + 1, i - 2, i + 1, i]);
                }
            }
        };

        if TRANSLUCENT_ONLY {
            if (draw.attr.is_translucent() && draw.attr.trans_new_depth()) || (!draw.attr.is_translucent() && draw.tex_image_param.is_translucent()) {
                push_indices(&mut self.indices_translucent, self.polygon_vertices_mapping[draw_index as usize], draw.vertex_count);
                return;
            } else {
                push_indices(&mut self.indices_translucent, self.vertices_buf_count, draw.vertex_count);
            }
        } else {
            push_indices(&mut self.indices_opaque, self.vertices_buf_count, draw.vertex_count);
            self.polygon_vertices_mapping[draw_index as usize] = self.vertices_buf_count;
        }

        // println!("add draw {:?}", draw.attr.primitive_type());

        for i in draw.vertex_start_index..draw.vertex_start_index + draw.vertex_count {
            // println!("add vertex {i}");
            let vertex = self.buffer.vertices.get_unchecked(i as usize);

            let color = u16::from(vertex.data.color());

            let gpu_vertex = Gpu3DVertex {
                coords: vertex.coords.float.0,
                tex_coords: [vertex.s.trans_tex_coords[0], vertex.s.trans_tex_coords[1]],
                tex_size: [1 << u8::from(draw.tex_image_param.size_s_shift()), 1 << u8::from(draw.tex_image_param.size_t_shift())],
                viewport: [draw.viewport.x1(), draw.viewport.y1(), draw.viewport.x2(), draw.viewport.y2()],
                color: [(color & 0x1F) as u8, ((color >> 5) & 0x1F) as u8, ((color >> 10) & 0x1F) as u8, u8::from(draw.attr.alpha())],
            };

            // println!("add vertex {i} {:?}", gpu_vertex.coords);

            *self.vertices_buf.get_unchecked_mut(self.vertices_buf_count as usize) = gpu_vertex;
            self.vertices_buf_count += 1;
        }
    }

    pub unsafe fn populate_tex_cache(&mut self, mem_buf: &mut GpuMemBuf, mem_refs: &GpuMemRefs) {
        self.texture_cache.mark_dirty(mem_buf, mem_refs);

        let mut last_value = u64::MAX;
        let mut last_texture_3d_ptr = ptr::null_mut();
        for i in 0..self.assembled_draw_count {
            let draw = self.assembled_draws.get_unchecked_mut(i as usize);
            if unlikely(draw.tex_image_param.format() == TextureFormat::None) {
                continue;
            }

            let key = draw.key();
            if key != last_value {
                let texture_3d = self.texture_cache.get(draw, mem_buf, mem_refs, &mut self.texture_ids_to_delete);
                last_value = key;
                last_texture_3d_ptr = texture_3d as _;
            }
            draw.texture_3d_ptr = last_texture_3d_ptr;
        }
        self.texture_cache.reset_usage();
        mem_buf.vram_banks.dirty_sections.clear();
    }

    pub unsafe fn process_polygons(&mut self, common: &mut GpuRendererCommon, mem_refs: &GpuMemRefs) {
        if self.buffer.pow_cnt1 != common.pow_cnt1[0] {
            return;
        }

        self.process_vertices();
        self.assemble_draws();
        self.buffer.vertices_count = 0;

        while !self.vram_ready.load(Ordering::SeqCst) {}
        self.populate_tex_cache(&mut common.mem_buf, mem_refs);
    }

    pub fn on_render_start(&self) {
        self.vram_ready.store(false, Ordering::SeqCst);
    }

    pub fn set_tex_ptrs(&mut self, refs: &mut GpuMemRefs) {
        unsafe {
            refs.tex_rear_plane_image = PtrWrapper::new(mem::transmute(self.mem.tex.as_mut_ptr()));
            refs.tex_pal = PtrWrapper::new(mem::transmute(self.mem.pal.as_mut_ptr()));
        }
    }

    pub fn on_vram_ready(&self) {
        self.vram_ready.store(true, Ordering::SeqCst);
    }

    pub fn get_fbo(&self, swap: bool, upscale: bool) -> &GpuFbo {
        if swap {
            &self.gl.top_fbo[upscale as usize]
        } else {
            &self.gl.bottom_fbo[upscale as usize]
        }
    }

    unsafe fn draw_elements(translucent_only: bool, program: &Gpu3DShaderProgram, indices: &[u16], indices_batch: &[IndicesBatch]) {
        let mut previous_offset = 0;
        for batch in indices_batch {
            // println!(
            //     "draw elements {translucent_only} {previous_offset} {} {:?}",
            //     batch.indices_offset - previous_offset,
            //     batch.attr.primitive_type()
            // );
            // println!("{:?}", &indices[previous_offset..batch.indices_offset]);

            if batch.tex_image_param.format() != TextureFormat::None {
                debug_assert_ne!(batch.tex, u32::MAX);
                gl::ActiveTexture(gl::TEXTURE0);
                gl::BindTexture(gl::TEXTURE_2D, batch.tex);
                gl::TexParameteri(
                    gl::TEXTURE_2D,
                    gl::TEXTURE_WRAP_S,
                    if batch.tex_image_param.repeat_s() {
                        if batch.tex_image_param.flip_s() {
                            gl::MIRRORED_REPEAT
                        } else {
                            gl::REPEAT
                        }
                    } else {
                        gl::CLAMP_TO_EDGE
                    } as _,
                );
                gl::TexParameteri(
                    gl::TEXTURE_2D,
                    gl::TEXTURE_WRAP_T,
                    if batch.tex_image_param.repeat_t() {
                        if batch.tex_image_param.flip_t() {
                            gl::MIRRORED_REPEAT
                        } else {
                            gl::REPEAT
                        }
                    } else {
                        gl::CLAMP_TO_EDGE
                    } as _,
                );
            }

            let tex_image_param = [u32::from(batch.tex_image_param)];
            gl::Uniform1fv(program.tex_image_param, 1, tex_image_param.as_ptr() as _);

            if batch.attr.depth_test_equal() {
                gl::DepthFunc(gl::EQUAL);
            } else {
                gl::DepthFunc(gl::LEQUAL);
            }

            gl::ColorMask(gl::TRUE, gl::TRUE, gl::TRUE, gl::TRUE);
            gl::StencilFunc(gl::ALWAYS, u8::from(batch.attr.id()) as _, 0x3F);
            gl::StencilOp(gl::KEEP, gl::KEEP, gl::REPLACE);
            gl::StencilMask(0x3F);

            if translucent_only {
                if batch.attr.trans_new_depth() {
                    gl::DepthFunc(gl::EQUAL);
                }

                if batch.attr.mode() == PolygonMode::Shadow {
                    gl::ColorMask(gl::FALSE, gl::FALSE, gl::FALSE, gl::FALSE);
                    gl::StencilMask(0x80);
                    if u8::from(batch.attr.id()) == 0 {
                        gl::StencilFunc(gl::ALWAYS, 0x80, 0x80);
                        gl::StencilOp(gl::KEEP, gl::REPLACE, gl::KEEP);
                    } else {
                        gl::StencilFunc(gl::NOTEQUAL, u8::from(batch.attr.id()) as _, 0x3F);
                        gl::StencilOp(gl::ZERO, gl::KEEP, gl::KEEP);
                    }
                }
            } else if batch.attr.trans_new_depth() {
                gl::Enable(gl::BLEND);
                gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            } else {
                gl::Disable(gl::BLEND);
            }

            if !batch.attr.render_back() || !batch.attr.render_front() {
                gl::Enable(gl::CULL_FACE);
                gl::CullFace(match (batch.attr.render_back(), batch.attr.render_front()) {
                    (false, false) => gl::FRONT_AND_BACK,
                    (true, false) => gl::FRONT,
                    (false, true) => gl::BACK,
                    _ => unreachable_unchecked(),
                })
            } else {
                gl::Disable(gl::CULL_FACE);
            }

            let attr = [u32::from(batch.attr)];
            gl::Uniform1fv(program.polygon_attrs, 1, attr.as_ptr() as _);

            let count = batch.indices_offset - previous_offset;
            let ptr = indices.as_ptr().add(previous_offset);
            gl::DrawElements(gl::TRIANGLES, count as _, gl::UNSIGNED_SHORT, ptr as _);

            if translucent_only && batch.attr.mode() == PolygonMode::Shadow && u8::from(batch.attr.id()) != 0 {
                gl::ColorMask(gl::TRUE, gl::TRUE, gl::TRUE, gl::TRUE);

                gl::StencilFunc(gl::EQUAL, 0x80, 0x80);
                gl::StencilOp(gl::KEEP, gl::KEEP, gl::KEEP);

                gl::DrawElements(gl::TRIANGLES, count as _, gl::UNSIGNED_SHORT, ptr as _);

                gl::StencilMask(0x80);
                gl::Clear(gl::STENCIL_BUFFER_BIT);
            }

            previous_offset = batch.indices_offset
        }
    }

    pub unsafe fn render(&mut self, common: &GpuRendererCommon, upscale: bool) {
        if self.buffer.pow_cnt1 != common.pow_cnt1[0] {
            return;
        }

        if !self.texture_ids_to_delete.is_empty() {
            gl::DeleteTextures(self.texture_ids_to_delete.len() as _, self.texture_ids_to_delete.as_ptr());
            self.texture_ids_to_delete.clear();
        }

        let fbo = self.get_fbo(self.buffer.pow_cnt1.display_swap(), upscale);
        gl::BindFramebuffer(gl::FRAMEBUFFER, fbo.fbo);
        gl::Viewport(0, 0, fbo.width as _, fbo.height as _);

        let [r, g, b, a] = self.inners[0].clear_colorf;
        gl::ClearColor(r, g, b, a);

        // gl::ClearDepth(self.inners[0].clear_depthf as _);
        gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT);

        self.vertices_buf_count = 0;
        if self.assembled_draw_count == 0 {
            return;
        }

        self.indices_opaque.clear();
        self.indices_translucent.clear();
        self.translucent_polygons.clear();
        self.indices_opaque_batches.clear();
        self.indices_translucent_batches.clear();

        #[cfg(target_os = "linux")]
        {
            self.vertices_buf = PtrWrapper::new(self.mem.vertices_buf.as_mut_ptr() as _);
        }

        self.active_texture_id = u32::MAX;
        self.active_tex_image_param = TexImageParam::default();
        self.active_polygon_attr = Gpu3DDrawAttr::default();
        for i in 0..self.assembled_draw_count {
            let draw = self.assembled_draws.get_unchecked(i as usize);
            if unlikely(draw.attr.is_translucent()) {
                if draw.attr.trans_new_depth() {
                    self.add_vertices::<false>(i);
                }
                self.translucent_polygons.push(i);
            } else {
                if draw.tex_image_param.is_translucent() {
                    self.translucent_polygons.push(i);
                }
                self.add_vertices::<false>(i);
            }
        }
        self.add_indices_batch::<false>();

        self.active_texture_id = u32::MAX;
        self.active_tex_image_param = TexImageParam::default();
        self.active_polygon_attr = Gpu3DDrawAttr::default();
        for i in 0..self.translucent_polygons.len() {
            unsafe { self.add_vertices::<true>(*self.translucent_polygons.get_unchecked(i)) };
        }
        self.add_indices_batch::<true>();

        if self.vertices_buf_count == 0 {
            return;
        }

        // println!("render");

        gl::Enable(gl::DEPTH_TEST);
        gl::DepthFunc(gl::LEQUAL);

        gl::Enable(gl::STENCIL_TEST);

        gl::BindBuffer(gl::ARRAY_BUFFER, self.gl.vertices_buf);
        #[cfg(target_os = "linux")]
        {
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (size_of::<Gpu3DVertex>() * self.vertices_buf_count as usize) as _,
                self.vertices_buf.as_ptr() as _,
                gl::DYNAMIC_DRAW,
            );
        }
        #[cfg(target_os = "vita")]
        {
            crate::presenter::Presenter::gl_buffer_data(gl::ARRAY_BUFFER, self.vertices_buf.as_ptr() as _);
        }

        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 4, gl::FLOAT, gl::FALSE, size_of::<Gpu3DVertex>() as _, mem::offset_of!(Gpu3DVertex, coords) as _);

        gl::EnableVertexAttribArray(1);
        gl::VertexAttribPointer(1, 2, gl::FLOAT, gl::FALSE, size_of::<Gpu3DVertex>() as _, mem::offset_of!(Gpu3DVertex, tex_coords) as _);

        gl::EnableVertexAttribArray(2);
        gl::VertexAttribPointer(2, 4, gl::UNSIGNED_BYTE, gl::FALSE, size_of::<Gpu3DVertex>() as _, mem::offset_of!(Gpu3DVertex, viewport) as _);

        gl::EnableVertexAttribArray(3);
        gl::VertexAttribPointer(3, 4, gl::UNSIGNED_BYTE, gl::FALSE, size_of::<Gpu3DVertex>() as _, mem::offset_of!(Gpu3DVertex, color) as _);

        gl::EnableVertexAttribArray(4);
        gl::VertexAttribPointer(4, 2, gl::UNSIGNED_BYTE, gl::FALSE, size_of::<Gpu3DVertex>() as _, mem::offset_of!(Gpu3DVertex, tex_size) as _);

        let program = self.gl.program.get_program(self.buffer.swap_buffers.depth_buffering_w());

        if !self.indices_opaque.is_empty() {
            gl::UseProgram(program.opaque.program);
            gl::DepthMask(gl::TRUE);

            Self::draw_elements(false, &program.opaque, &self.indices_opaque, &self.indices_opaque_batches);
        }

        if !self.indices_translucent.is_empty() {
            gl::UseProgram(program.translucent.program);

            gl::Enable(gl::BLEND);
            gl::DepthMask(gl::FALSE);

            gl::BlendFuncSeparate(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA, gl::ONE, gl::ONE);
            gl::BlendEquationSeparate(gl::FUNC_ADD, gl::MAX);

            Self::draw_elements(true, &program.translucent, &self.indices_translucent, &self.indices_translucent_batches);
        }

        gl::DepthMask(gl::TRUE);
        gl::Disable(gl::DEPTH_TEST);
        gl::Disable(gl::BLEND);
        gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, 0);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        gl::UseProgram(0);
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        gl::CullFace(gl::BACK);
        gl::Disable(gl::CULL_FACE);
        gl::Disable(gl::STENCIL_TEST);
        gl::ColorMask(gl::TRUE, gl::TRUE, gl::TRUE, gl::TRUE);
    }
}
