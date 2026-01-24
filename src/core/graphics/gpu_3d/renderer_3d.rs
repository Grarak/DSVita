use crate::core::graphics::gl_utils::{create_mem_texture2d, create_pal_texture2d, sub_mem_texture2d, sub_pal_texture2d, GpuFbo};
use crate::core::graphics::gpu::{PowCnt1, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_3d::registers_3d::POLYGON_LIMIT;
use crate::core::graphics::gpu_3d::registers_3d::{Gpu3DBuffer, Gpu3DRegisters, PolygonAttr, PrimitiveType, TexImageParam, TextureCoordTransMode, Vertex, Viewport};
use crate::core::graphics::gpu_mem_buf::GpuMemRefs;
use crate::core::graphics::gpu_renderer::GpuRendererCommon;
use crate::core::graphics::gpu_shaders::GpuShadersPrograms;
use crate::core::memory::vram;
use crate::math::{vmult_vec4_mat4_no_store, Vectori32};
use crate::utils::{rgb5_to_float8, HeapArray, HeapArrayU16, HeapArrayU8, HeapMem, PtrWrapper};
use bilge::prelude::*;
use gl::types::{GLint, GLuint};
use static_assertions::const_assert_eq;
use std::arch::arm::{vcvt_n_f32_s32, vcvtq_n_f32_s32, vget_low_s32, vsetq_lane_s32, vshr_n_s32, vst1_f32, vst1q_f32};
use std::hint::{assert_unchecked, unreachable_unchecked};
use std::intrinsics::unlikely;
use std::mem::{self, MaybeUninit};

#[bitsize(32)]
#[derive(FromBits)]
struct ClearColor {
    color: u15,
    fog: bool,
    alpha: u5,
    not_used: u3,
    clear_polygon_id: u6,
    not_used1: u2,
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

#[derive(Clone, Default)]
struct Gpu3DRendererInner {
    disp_cnt: Disp3DCnt,
    edge_colors: [u16; 8],
    clear_color: u32,
    clear_depth: u16,
    fog_color: u32,
    fog_offset: u16,
    fog_table: [u8; 32],
    toon_table: [u16; 32],
}

pub struct Gpu3DGl {
    translucent_only_loc: GLint,
    tex: GLuint,
    pal_tex: GLuint,
    attrs_ubo: GLuint,
    vertices_buf: GLuint,
    program: GLuint,
    top_fbo: GpuFbo,
    bottom_fbo: GpuFbo,
}

impl Gpu3DGl {
    fn new(gpu_programs: &GpuShadersPrograms) -> Self {
        unsafe {
            gl::UseProgram(gpu_programs.render_3d);

            let translucent_only_loc = gl::GetUniformLocation(gpu_programs.render_3d, c"translucentOnly".as_ptr() as _);

            gl::BindAttribLocation(gpu_programs.render_3d, 0, c"position".as_ptr() as _);
            gl::BindAttribLocation(gpu_programs.render_3d, 1, c"texCoords".as_ptr() as _);
            gl::BindAttribLocation(gpu_programs.render_3d, 2, c"viewport".as_ptr() as _);
            gl::BindAttribLocation(gpu_programs.render_3d, 3, c"color".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(gpu_programs.render_3d, c"tex".as_ptr() as _), 0);
            gl::Uniform1i(gl::GetUniformLocation(gpu_programs.render_3d, c"palTex".as_ptr() as _), 1);

            let mut vertices_buf = 0;
            gl::GenBuffers(1, &mut vertices_buf);
            gl::BindBuffer(gl::ARRAY_BUFFER, vertices_buf);

            let mut attrs_ubo = 0;
            gl::GenBuffers(1, &mut attrs_ubo);
            gl::BindBuffer(gl::UNIFORM_BUFFER, attrs_ubo);

            if cfg!(target_os = "linux") {
                gl::UniformBlockBinding(gpu_programs.render_3d, gl::GetUniformBlockIndex(gpu_programs.render_3d, c"PolygonAttrsUbo".as_ptr() as _), 0);
            }

            gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::UseProgram(0);

            Gpu3DGl {
                translucent_only_loc,
                tex: create_mem_texture2d(1024, 512),
                pal_tex: create_pal_texture2d(1024, 96),
                attrs_ubo,
                vertices_buf,
                program: gpu_programs.render_3d,
                top_fbo: GpuFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, true).unwrap(),
                bottom_fbo: GpuFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, true).unwrap(),
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
}

#[derive(Default, Clone)]
#[repr(C)]
struct Gpu3DVertex {
    coords: [f32; 4],
    tex_coords: [f32; 3],
    viewport: [u8; 4],
    color: [u8; 3],
}

#[derive(Default)]
struct Gpu3DPolygon {
    vertex_start_index: u16,
    attr: PolygonAttr,
    tex_image_param: TexImageParam,
    viewport: Viewport,
}

pub struct Gpu3DRenderer {
    pub dirty: bool,
    inners: [Gpu3DRendererInner; 2],
    buffer: HeapMem<Gpu3DBuffer>,
    gl: Gpu3DGl,

    assembled_polygons: HeapArray<Gpu3DPolygon, POLYGON_LIMIT>,
    assembled_polygons_count: u16,

    translucent_polygons: Vec<u16>,
    translucent_depth_polygons: Vec<u16>,

    vertices_buf: Vec<Gpu3DVertex>,

    indices_opaque_buf: Vec<u16>,
    indices_translucent_buf: Vec<u16>,

    polygon_vertices_mapping: HeapArrayU16<POLYGON_LIMIT>,
    polygon_attrs: HeapArray<Gpu3dPolygonAttr, POLYGON_LIMIT>,

    #[cfg(target_os = "linux")]
    mem: Gpu3DTexMem,
}

impl Gpu3DRenderer {
    pub fn new(gpu_programs: &GpuShadersPrograms) -> Self {
        Gpu3DRenderer {
            dirty: false,
            inners: [Gpu3DRendererInner::default(), Gpu3DRendererInner::default()],
            buffer: Default::default(),
            gl: Gpu3DGl::new(gpu_programs),

            assembled_polygons: HeapArray::default(),
            assembled_polygons_count: 0,

            translucent_polygons: Vec::new(),
            translucent_depth_polygons: Vec::new(),

            vertices_buf: Vec::new(),

            indices_opaque_buf: Vec::new(),
            indices_translucent_buf: Vec::new(),

            polygon_vertices_mapping: Default::default(),
            polygon_attrs: Default::default(),

            #[cfg(target_os = "linux")]
            mem: Default::default(),
        }
    }

    pub fn init(&mut self) {
        self.dirty = false;
        self.inners[0] = Gpu3DRendererInner::default();
        self.inners[1] = Gpu3DRendererInner::default();
        self.buffer.reset_all();
        self.buffer.pow_cnt1 = PowCnt1::from(0);

        unsafe {
            for fbo in [self.gl.top_fbo.fbo, self.gl.bottom_fbo.fbo] {
                gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
                gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);

                let clear_color = ClearColor::from(self.inners[0].clear_color);
                let [r, g, b] = rgb5_to_float8(u16::from(clear_color.color()));
                gl::ClearColor(r, g, b, u8::from(clear_color.alpha()) as f32 / 31f32);

                gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
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
        if value & mask == self.inners[1].clear_color & mask {
            return;
        }
        self.inners[1].clear_color = (self.inners[1].clear_color & !mask) | (value & mask);
        self.invalidate();
    }

    pub fn set_clear_depth(&mut self, mut mask: u16, value: u16) {
        mask &= 0x7FFF;
        if value & mask == self.inners[1].clear_depth & mask {
            return;
        }
        self.inners[1].clear_depth = (self.inners[1].clear_depth & !mask) | (value & mask);
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
            vertex.coords.fixed[3] = 1 << 12;
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

                if tex_coord_trans_mode != TextureCoordTransMode::TexCoord {
                    tex_matrix[3] = vsetq_lane_s32::<0>((vertex.s.indices.tex_coords[0] as i32) << 12, tex_matrix[3]);
                    tex_matrix[3] = vsetq_lane_s32::<1>((vertex.s.indices.tex_coords[1] as i32) << 12, tex_matrix[3]);
                }

                match tex_coord_trans_mode {
                    TextureCoordTransMode::TexCoord => {
                        let vector = Vectori32::<4>::new([(vertex.s.indices.tex_coords[0] as i32) << 8, (vertex.s.indices.tex_coords[1] as i32) << 8, 1 << 8, 1 << 8]);
                        let ret = vmult_vec4_mat4_no_store(vector.vld(), tex_matrix);
                        let ret = vshr_n_s32::<8>(vget_low_s32(ret));
                        let ret = vcvt_n_f32_s32::<4>(ret);
                        vst1_f32(vertex.s.trans_tex_coords.0.as_mut_ptr(), ret);
                    }
                    TextureCoordTransMode::Normal => {
                        let normal = Vectori32::<4>::new([vertex.normal[0] as i32, vertex.normal[1] as i32, vertex.normal[2] as i32, 1 << 12]);
                        let ret = vmult_vec4_mat4_no_store(normal.vld(), tex_matrix);
                        let ret = vshr_n_s32::<12>(vget_low_s32(ret));
                        let ret = vcvt_n_f32_s32::<4>(ret);
                        vst1_f32(vertex.s.trans_tex_coords.0.as_mut_ptr(), ret);
                    }
                    TextureCoordTransMode::Vertex => {
                        let ret = vmult_vec4_mat4_no_store(trans_coords, tex_matrix);
                        let ret = vshr_n_s32::<12>(vget_low_s32(ret));
                        let ret = vcvt_n_f32_s32::<4>(ret);
                        vst1_f32(vertex.s.trans_tex_coords.0.as_mut_ptr(), ret);
                    }
                    _ => unreachable_unchecked(),
                }
            } else {
                let tex_coords = vertex.s.indices.tex_coords;
                vertex.s.trans_tex_coords[0] = tex_coords[0] as f32 / 16.0;
                vertex.s.trans_tex_coords[1] = tex_coords[1] as f32 / 16.0;
            }
        }
    }

    unsafe fn assemble_polygons(&mut self) {
        self.assembled_polygons_count = 0;

        let mut viewport = MaybeUninit::uninit().assume_init();
        let mut polygon_attr = MaybeUninit::uninit().assume_init();
        let mut polygon_attr_value_ubo = MaybeUninit::uninit().assume_init();
        let mut polygon_vertex_count: u16 = MaybeUninit::uninit().assume_init();

        for i in 0..self.buffer.vertices_count {
            let vertex = self.buffer.vertices.get_unchecked(i as usize);
            let polygon = self.buffer.polygons.get_unchecked(u16::from(vertex.data.polygon_index()) as usize);
            assert_unchecked(i != 0 || vertex.data.begin_vtxs());
            if vertex.data.begin_vtxs() {
                viewport = polygon.viewport;
                polygon_attr = polygon.attr;
                polygon_attr_value_ubo = (u32::from(polygon_attr.alpha()) | (u32::from(polygon_attr.mode()) << 5) | ((polygon_attr.trans_new_depth() as u32) << 7)) << 16;
                polygon_vertex_count = 0;
            }

            polygon_vertex_count += 1;
            let polygon_complete = match polygon_attr.primitive_type() {
                PrimitiveType::SeparateTriangles => {
                    let ret = polygon_vertex_count == 3;
                    if ret {
                        polygon_vertex_count = 0;
                    }
                    ret
                }
                PrimitiveType::SeparateQuadliterals => polygon_vertex_count % 4 == 0,
                PrimitiveType::TriangleStrips => polygon_vertex_count >= 3,
                PrimitiveType::QuadliteralStrips => polygon_vertex_count >= 4 && polygon_vertex_count % 2 == 0,
            };

            if polygon_complete {
                debug_assert_eq!(polygon_attr.primitive_type(), polygon.attr.primitive_type());
                self.assembled_polygons[self.assembled_polygons_count as usize] = Gpu3DPolygon {
                    vertex_start_index: i + 1 - polygon_attr.primitive_type().vertex_count() as u16,
                    attr: polygon_attr,
                    tex_image_param: polygon.tex_image_param,
                    viewport,
                };

                let polygon_attr_ubo = &mut self.polygon_attrs[self.assembled_polygons_count as usize];
                polygon_attr_ubo.tex_image_param = u32::from(polygon.tex_image_param);
                polygon_attr_ubo.pal_addr_poly_attr = polygon.palette_addr as u32 | polygon_attr_value_ubo;

                self.assembled_polygons_count += 1;
                if self.assembled_polygons_count == POLYGON_LIMIT as u16 {
                    break;
                }
            }
        }
    }

    unsafe fn add_vertices<const TRANSLUCENT_ONLY: bool>(&mut self, polygon_index: u16) {
        assert_unchecked((polygon_index as usize) < POLYGON_LIMIT);
        let polygon = &self.assembled_polygons[polygon_index as usize];

        // println!(
        //     "renderer: translucent only {TRANSLUCENT_ONLY} polygon {polygon_index} type {:?} pal addr {:x} tex image param {:?} attr {:?}",
        //     polygon.polygon_type,
        //     (polygon.palette_addr as u32) << 3,
        //     polygon.tex_image_param,
        //     polygon.attr
        // );

        let polygon_type = polygon.attr.primitive_type();
        let vertex_start_index = polygon.vertex_start_index;
        for i in 0..polygon_type.vertex_count() {
            let vertex = self.buffer.vertices.get_unchecked_mut(vertex_start_index as usize + i as usize);
            if vertex.coords.float[3] == 0.0 {
                return;
            }
        }

        let push_indices = |indices_buf: &mut Vec<u16>, vertex_index: u16| {
            indices_buf.push(vertex_index);
            indices_buf.push(vertex_index + 1);
            if polygon_type == PrimitiveType::QuadliteralStrips {
                indices_buf.push(vertex_index + 3);
            } else {
                indices_buf.push(vertex_index + 2);
            }

            for i in 3..polygon_type.vertex_count() as u16 {
                indices_buf.push(vertex_index);
                indices_buf.push(vertex_index + i - 1);
                indices_buf.push(vertex_index + i);
            }
        };

        if TRANSLUCENT_ONLY {
            if (polygon.attr.is_translucent() && polygon.attr.trans_new_depth()) || (!polygon.attr.is_translucent() && polygon.tex_image_param.is_translucent()) {
                push_indices(&mut self.indices_translucent_buf, self.polygon_vertices_mapping[polygon_index as usize]);
                return;
            } else {
                push_indices(&mut self.indices_translucent_buf, self.vertices_buf.len() as u16);
            }
        } else {
            push_indices(&mut self.indices_opaque_buf, self.vertices_buf.len() as u16);
            if (polygon.attr.is_translucent() && polygon.attr.trans_new_depth()) || polygon.tex_image_param.is_translucent() {
                self.polygon_vertices_mapping[polygon_index as usize] = self.vertices_buf.len() as u16;
            }
        }

        for i in 0..polygon_type.vertex_count() {
            let vertex_index = vertex_start_index + i as u16;
            let vertex = self.buffer.vertices.get_unchecked(vertex_index as usize);

            let color = u16::from(vertex.data.color());

            let gpu_vertex = Gpu3DVertex {
                coords: vertex.coords.float.0,
                tex_coords: [vertex.s.trans_tex_coords[0], vertex.s.trans_tex_coords[1], polygon_index as f32],
                viewport: [polygon.viewport.x1(), polygon.viewport.y1(), polygon.viewport.x2(), polygon.viewport.y2()],
                color: [(color & 0x1F) as u8, ((color >> 5) & 0x1F) as u8, ((color >> 10) & 0x1F) as u8],
            };

            self.vertices_buf.push(gpu_vertex);
        }
    }

    pub unsafe fn process_polygons(&mut self, common: &GpuRendererCommon) {
        if self.buffer.pow_cnt1 != common.pow_cnt1[0] {
            return;
        }

        self.process_vertices();
        self.assemble_polygons();

        self.vertices_buf.clear();
        self.indices_opaque_buf.clear();
        self.indices_translucent_buf.clear();
        self.translucent_polygons.clear();
        self.translucent_depth_polygons.clear();

        for i in 0..self.assembled_polygons_count {
            let polygon = self.assembled_polygons.get_unchecked(i as usize);
            if unlikely(polygon.attr.is_translucent()) {
                if polygon.attr.trans_new_depth() {
                    self.translucent_depth_polygons.push(i);
                }
                self.translucent_polygons.push(i);
            } else {
                if polygon.tex_image_param.is_translucent() {
                    self.translucent_polygons.push(i);
                }
                self.add_vertices::<false>(i);
            }
        }

        for i in 0..self.translucent_depth_polygons.len() {
            unsafe { self.add_vertices::<false>(*self.translucent_depth_polygons.get_unchecked(i)) };
        }

        for i in 0..self.translucent_polygons.len() {
            unsafe { self.add_vertices::<true>(*self.translucent_polygons.get_unchecked(i)) };
        }
    }

    pub fn set_tex_ptrs(&mut self, refs: &mut GpuMemRefs) {
        #[cfg(target_os = "linux")]
        unsafe {
            refs.tex_rear_plane_image = PtrWrapper::new(mem::transmute(self.mem.tex.as_mut_ptr()));
            refs.tex_pal = PtrWrapper::new(mem::transmute(self.mem.pal.as_mut_ptr()));
        }
        #[cfg(target_os = "vita")]
        unsafe {
            use crate::presenter::Presenter;

            gl::BindTexture(gl::TEXTURE_2D, self.gl.tex);
            refs.tex_rear_plane_image = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
            gl::BindTexture(gl::TEXTURE_2D, self.gl.pal_tex);
            refs.tex_pal = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
        }
    }

    pub fn get_fbo(&self, swap: bool) -> &GpuFbo {
        if swap {
            &self.gl.top_fbo
        } else {
            &self.gl.bottom_fbo
        }
    }

    pub unsafe fn render(&mut self, common: &GpuRendererCommon, mem_refs: &GpuMemRefs) {
        if self.buffer.pow_cnt1 != common.pow_cnt1[0] {
            return;
        }

        let fbo = self.get_fbo(self.buffer.pow_cnt1.display_swap());
        gl::BindFramebuffer(gl::FRAMEBUFFER, fbo.fbo);
        gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);

        let clear_color = ClearColor::from(self.inners[0].clear_color);
        let [r, g, b] = rgb5_to_float8(u16::from(clear_color.color()));
        gl::ClearColor(r, g, b, u8::from(clear_color.alpha()) as f32 / 31f32);

        gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

        if self.vertices_buf.is_empty() {
            return;
        }

        if cfg!(target_os = "linux") {
            gl::BindTexture(gl::TEXTURE_2D, self.gl.tex);
            sub_mem_texture2d(1024, 512, mem_refs.tex_rear_plane_image.as_ptr());

            gl::BindTexture(gl::TEXTURE_2D, self.gl.pal_tex);
            sub_pal_texture2d(1024, 96, mem_refs.tex_pal.as_ptr());
        }

        gl::BindBuffer(gl::ARRAY_BUFFER, self.gl.vertices_buf);
        gl::BufferData(
            gl::ARRAY_BUFFER,
            (size_of::<Gpu3DVertex>() * self.vertices_buf.len()) as _,
            self.vertices_buf.as_ptr() as _,
            gl::DYNAMIC_DRAW,
        );

        gl::UseProgram(self.gl.program);

        gl::Enable(gl::DEPTH_TEST);
        gl::DepthFunc(gl::LEQUAL);
        gl::DepthRange(0.0, 1.0);

        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindTexture(gl::TEXTURE_2D, self.gl.tex);

        gl::ActiveTexture(gl::TEXTURE1);
        gl::BindTexture(gl::TEXTURE_2D, self.gl.pal_tex);

        gl::BindBuffer(gl::UNIFORM_BUFFER, self.gl.attrs_ubo);
        gl::BufferData(
            gl::UNIFORM_BUFFER,
            (self.assembled_polygons_count as usize * size_of::<Gpu3dPolygonAttr>()) as _,
            self.polygon_attrs.as_ptr() as _,
            gl::DYNAMIC_DRAW,
        );
        gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, self.gl.attrs_ubo);

        gl::BindBuffer(gl::ARRAY_BUFFER, self.gl.vertices_buf);

        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 4, gl::FLOAT, gl::FALSE, size_of::<Gpu3DVertex>() as _, mem::offset_of!(Gpu3DVertex, coords) as _);

        gl::EnableVertexAttribArray(1);
        gl::VertexAttribPointer(1, 3, gl::FLOAT, gl::FALSE, size_of::<Gpu3DVertex>() as _, mem::offset_of!(Gpu3DVertex, tex_coords) as _);

        gl::EnableVertexAttribArray(2);
        gl::VertexAttribPointer(2, 4, gl::UNSIGNED_BYTE, gl::TRUE, size_of::<Gpu3DVertex>() as _, mem::offset_of!(Gpu3DVertex, viewport) as _);

        gl::EnableVertexAttribArray(3);
        gl::VertexAttribPointer(3, 3, gl::UNSIGNED_BYTE, gl::FALSE, size_of::<Gpu3DVertex>() as _, mem::offset_of!(Gpu3DVertex, color) as _);

        if !self.indices_opaque_buf.is_empty() {
            gl::DepthMask(gl::TRUE);

            gl::Uniform1i(self.gl.translucent_only_loc, 0);

            gl::DrawElements(gl::TRIANGLES, self.indices_opaque_buf.len() as _, gl::UNSIGNED_SHORT, self.indices_opaque_buf.as_ptr() as _);
        }

        if !self.indices_translucent_buf.is_empty() {
            gl::DepthMask(gl::FALSE);

            gl::Enable(gl::BLEND);
            gl::BlendFuncSeparate(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA, gl::ONE, gl::ONE);
            gl::BlendEquationSeparate(gl::FUNC_ADD, gl::MAX);

            gl::Uniform1i(self.gl.translucent_only_loc, 1);

            gl::DrawElements(gl::TRIANGLES, self.indices_translucent_buf.len() as _, gl::UNSIGNED_SHORT, self.indices_translucent_buf.as_ptr() as _);
        }

        gl::DepthMask(gl::TRUE);
        gl::Disable(gl::DEPTH_TEST);
        gl::DepthRange(0.0, 1.0);
        gl::Disable(gl::BLEND);
        gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
        gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        gl::UseProgram(0);
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
    }
}
