use crate::core::graphics::gl_utils::{create_mem_texture2d, create_pal_texture2d, create_program, create_shader, shader_source, sub_mem_texture2d, sub_pal_texture2d, GpuFbo};
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_3d::registers_3d::{Gpu3DRegisters, Polygon, PrimitiveType, SwapBuffers, TextureCoordTransMode, Vertex};
use crate::core::graphics::gpu_3d::registers_3d::{POLYGON_LIMIT, VERTEX_LIMIT};
use crate::core::graphics::gpu_renderer::GpuRendererCommon;
use crate::math::{Matrix, Vectori32};
use crate::utils::{rgb5_to_float8, rgb6_to_float8, HeapMem};
use bilge::prelude::*;
use gl::types::GLuint;
use static_assertions::const_assert_eq;
use std::mem::MaybeUninit;
use std::ptr;

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
    tex: GLuint,
    pal_tex: GLuint,
    attr_tex: GLuint,
    vertices_buf: GLuint,
    program: GLuint,
    pub fbo: GpuFbo,
}

impl Default for Gpu3DGl {
    fn default() -> Self {
        unsafe {
            let vert_shader = create_shader("render 3d", shader_source!("render_vert"), gl::VERTEX_SHADER).unwrap();
            let frag_shader = create_shader("render 3d", shader_source!("render_frag"), gl::FRAGMENT_SHADER).unwrap();
            let program = create_program(&[vert_shader, frag_shader]).unwrap();
            gl::DeleteShader(vert_shader);
            gl::DeleteShader(frag_shader);

            gl::UseProgram(program);

            gl::BindAttribLocation(program, 0, c"position".as_ptr() as _);
            gl::BindAttribLocation(program, 1, c"color".as_ptr() as _);
            gl::BindAttribLocation(program, 2, c"texCoords".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(program, c"tex".as_ptr() as _), 0);
            gl::Uniform1i(gl::GetUniformLocation(program, c"palTex".as_ptr() as _), 1);
            gl::Uniform1i(gl::GetUniformLocation(program, c"attrTex".as_ptr() as _), 2);

            let mut vertices_buf = 0;
            gl::GenBuffers(1, &mut vertices_buf);
            gl::BindBuffer(gl::ARRAY_BUFFER, vertices_buf);

            gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::UseProgram(0);

            Gpu3DGl {
                tex: create_mem_texture2d(1024, 512),
                pal_tex: create_pal_texture2d(1024, 96),
                attr_tex: create_mem_texture2d(256, 256),
                vertices_buf,
                program,
                fbo: GpuFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, true).unwrap(),
            }
        }
    }
}

#[derive(Clone)]
#[repr(C)]
struct Gpu3DVertex {
    coords: [f32; 4],
    color: [f32; 4],
    tex_coords: [f32; 2],
}

#[derive(Default)]
#[repr(C)]
struct Gpu3dPolygonAttr {
    tex_image_param: u32,
    pal_addr: u16,
    poly_attr: u16,
}

const_assert_eq!(size_of::<Gpu3dPolygonAttr>(), 8);

#[derive(Default)]
pub struct Gpu3DRendererContent {
    pub vertices: HeapMem<Vertex, VERTEX_LIMIT>,
    pub vertices_size: u16,
    pub polygons: HeapMem<Polygon, POLYGON_LIMIT>,
    pub polygons_size: u16,
    pub clip_matrices: Vec<Matrix>,
    pub tex_matrices: Vec<Matrix>,
    pub swap_buffers: SwapBuffers,
}

#[derive(Default)]
pub struct Gpu3DRenderer {
    pub dirty: bool,
    inners: [Gpu3DRendererInner; 2],
    content: Gpu3DRendererContent,
    pub gl: Gpu3DGl,
    vertices_buf: Vec<Gpu3DVertex>,
    indices_buf: Vec<u16>,
    polygon_attrs: HeapMem<Gpu3dPolygonAttr, POLYGON_LIMIT>,
}

impl Gpu3DRenderer {
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

        if registers.consume {
            registers.consume = false;

            registers.swap_to_renderer(&mut self.content);
        }
    }

    pub unsafe fn render(&mut self, common: &GpuRendererCommon) {
        self.vertices_buf.clear();
        self.indices_buf.clear();

        'outer: for i in 0..self.content.polygons_size {
            let polygon = &self.content.polygons[i as usize];

            if u8::from(polygon.tex_image_param.format()) != 0
                && u8::from(polygon.tex_image_param.format()) != 1
                && u8::from(polygon.tex_image_param.format()) != 2
                && u8::from(polygon.tex_image_param.format()) != 3
                && u8::from(polygon.tex_image_param.format()) != 6
                && u8::from(polygon.tex_image_param.format()) != 7
            {
                // todo!("{}", u8::from(polygon.tex_image_param.format()))
            }

            // println!(
            //     "renderer: polygon {i} type {:?} pal addr {:x} tex image param {:?} attr {:?}",
            //     polygon.polygon_type,
            //     (polygon.palette_addr as u32) << 3,
            //     polygon.tex_image_param,
            //     polygon.attr
            // );

            let mut transformed_coords: [Vectori32<4>; 4] = unsafe { MaybeUninit::uninit().assume_init() };
            for j in 0..polygon.polygon_type.vertex_count() {
                let vertex = &mut self.content.vertices[polygon.vertices_index as usize + j as usize];
                vertex.coords[3] = 1 << 12;
                transformed_coords[j as usize] = vertex.coords * &self.content.clip_matrices[vertex.clip_matrix_index as usize];
                if transformed_coords[j as usize][3] == 0 {
                    continue 'outer;
                }
            }

            let vertex_index = self.vertices_buf.len() as u16;
            self.indices_buf.push(vertex_index);
            self.indices_buf.push(vertex_index + 1);
            if polygon.polygon_type == PrimitiveType::QuadliteralStrips {
                self.indices_buf.push(vertex_index + 3);
            } else {
                self.indices_buf.push(vertex_index + 2);
            }

            for j in 3..polygon.polygon_type.vertex_count() as u16 {
                self.indices_buf.push(vertex_index);
                self.indices_buf.push(vertex_index + j - 1);
                self.indices_buf.push(vertex_index + j);
            }

            let x = polygon.viewport.x1();
            let y = 191 - polygon.viewport.y2();
            let w = (polygon.viewport.x2() - polygon.viewport.x1()) as u16 + 1;
            let h = (191 - polygon.viewport.y1() - y) as u16 + 1;

            for j in 0..polygon.polygon_type.vertex_count() {
                let vertex = &mut self.content.vertices[polygon.vertices_index as usize + j as usize];

                let coords = &transformed_coords[j as usize];
                let c = rgb6_to_float8(vertex.color);

                let vertex_x = ((coords[0] as i64 + coords[3] as i64) * w as i64 / (coords[3] as i64 * 2) + x as i64) as i32;
                let vertex_y = ((-coords[1] as i64 + coords[3] as i64) * h as i64 / (coords[3] as i64 * 2) + y as i64) as i32;

                let mut tex_coords = vertex.tex_coords;
                let tex_coord_trans_mode = TextureCoordTransMode::from(u8::from(polygon.tex_image_param.coord_trans_mode()));
                if tex_coord_trans_mode == TextureCoordTransMode::TexCoord && (vertex.tex_matrix_index as usize) < self.content.tex_matrices.len() {
                    let mut vector = Vectori32::<4>::new([(tex_coords[0] as i32) << 8, (tex_coords[1] as i32) << 8, 1 << 12, 1 << 12]);
                    vector *= &self.content.tex_matrices[vertex.tex_matrix_index as usize];
                    tex_coords[0] = (vector[0] >> 8) as i16;
                    tex_coords[1] = (vector[1] >> 8) as i16;
                }

                let w = coords[3] as f32 / 4096f32;
                self.vertices_buf.push(Gpu3DVertex {
                    coords: [
                        (vertex_x as f32 * 2f32 / 255f32 - 1f32) * w,
                        (1f32 - vertex_y as f32 * 2f32 / 191f32) * w,
                        coords[2] as f32 / 4096f32,
                        w,
                    ],
                    color: [c.0, c.1, c.2, i as f32],
                    tex_coords: [tex_coords[0] as f32 / 16f32, tex_coords[1] as f32 / 16f32],
                });

                // println!(
                //     "vertex {j} s {} t {} s_norm {} t_norm {}",
                //     self.content.vertices[polygon.vertices_index as usize + j as usize].tex_coords[0],
                //     self.content.vertices[polygon.vertices_index as usize + j as usize].tex_coords[1],
                //     self.vertices_buf[self.vertices_buf.len() - 1].tex_coords[0],
                //     self.vertices_buf[self.vertices_buf.len() - 1].tex_coords[1],
                // )
            }

            self.polygon_attrs[i as usize].tex_image_param = u32::from(polygon.tex_image_param);
            self.polygon_attrs[i as usize].pal_addr = polygon.palette_addr;
            self.polygon_attrs[i as usize].poly_attr = u16::from(polygon.attr.alpha());
        }

        gl::BindFramebuffer(gl::FRAMEBUFFER, self.gl.fbo.fbo);
        gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);

        let clear_color = ClearColor::from(self.inners[0].clear_color);
        let (r, g, b) = rgb5_to_float8(u16::from(clear_color.color()));
        gl::ClearColor(r, g, b, u8::from(clear_color.alpha()) as f32 / 31f32);

        gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

        if self.vertices_buf.is_empty() {
            return;
        }

        gl::UseProgram(self.gl.program);

        gl::Enable(gl::BLEND);
        gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        gl::Enable(gl::DEPTH_TEST);
        gl::DepthFunc(gl::LEQUAL);
        gl::DepthRange(0.0, 1.0);

        gl::BindTexture(gl::TEXTURE_2D, self.gl.tex);
        sub_mem_texture2d(1024, 512, common.mem_buf.tex_rear_plane_image.as_ptr());

        gl::BindTexture(gl::TEXTURE_2D, self.gl.pal_tex);
        sub_pal_texture2d(1024, 96, common.mem_buf.tex_pal.as_ptr());

        gl::BindTexture(gl::TEXTURE_2D, self.gl.attr_tex);
        sub_mem_texture2d(256, 256, self.polygon_attrs.as_ptr() as _);

        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindTexture(gl::TEXTURE_2D, self.gl.tex);

        gl::ActiveTexture(gl::TEXTURE1);
        gl::BindTexture(gl::TEXTURE_2D, self.gl.pal_tex);

        gl::ActiveTexture(gl::TEXTURE2);
        gl::BindTexture(gl::TEXTURE_2D, self.gl.attr_tex);

        gl::BindBuffer(gl::ARRAY_BUFFER, self.gl.vertices_buf);
        gl::BufferData(
            gl::ARRAY_BUFFER,
            (size_of::<Gpu3DVertex>() * self.vertices_buf.len()) as _,
            self.vertices_buf.as_ptr() as _,
            gl::DYNAMIC_DRAW,
        );

        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 4, gl::FLOAT, gl::FALSE, size_of::<Gpu3DVertex>() as _, ptr::null());

        gl::EnableVertexAttribArray(1);
        gl::VertexAttribPointer(1, 4, gl::FLOAT, gl::FALSE, size_of::<Gpu3DVertex>() as _, (size_of::<f32>() * 4) as _);

        gl::EnableVertexAttribArray(2);
        gl::VertexAttribPointer(2, 2, gl::FLOAT, gl::FALSE, size_of::<Gpu3DVertex>() as _, (size_of::<f32>() * 8) as _);

        gl::DrawElements(gl::TRIANGLES, self.indices_buf.len() as _, gl::UNSIGNED_SHORT, self.indices_buf.as_ptr() as _);

        gl::Disable(gl::BLEND);
        gl::Disable(gl::DEPTH_TEST);
        gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
        gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        gl::UseProgram(0);
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
    }
}
