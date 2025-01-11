use crate::core::graphics::gl_utils::{create_mem_texture2d, create_pal_texture2d, create_program, create_shader, shader_source, sub_mem_texture2d, sub_pal_texture2d, GpuFbo};
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_3d::registers_3d::{Gpu3DRegisters, Polygon, Vertex};
use crate::core::graphics::gpu_3d::registers_3d::{POLYGON_LIMIT, VERTEX_LIMIT};
use crate::core::graphics::gpu_renderer::GpuRendererCommon;
use crate::utils::{rgb6_to_float8, HeapMem};
use bilge::prelude::*;
use gl::types::GLuint;
use static_assertions::const_assert_eq;
use std::{mem, ptr};

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

            gl::BindAttribLocation(program, 0, "position\0".as_ptr() as _);
            gl::BindAttribLocation(program, 1, "color\0".as_ptr() as _);
            gl::BindAttribLocation(program, 2, "texCoords\0".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(program, "tex\0".as_ptr() as _), 0);
            gl::Uniform1i(gl::GetUniformLocation(program, "palTex\0".as_ptr() as _), 1);
            gl::Uniform1i(gl::GetUniformLocation(program, "attrTex\0".as_ptr() as _), 2);

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
    color: [f32; 3],
    tex_coords: [f32; 2],
}

impl From<(&Vertex, u16)> for Gpu3DVertex {
    fn from(value: (&Vertex, u16)) -> Self {
        let (vertex, polygon_index) = value;
        if vertex.coords[3] != 0 {
            let c = rgb6_to_float8(vertex.color);

            let [x1, y1, x2, y2] = *vertex.viewport.as_ref();
            let x = x1;
            let y = 191 - y2;
            let w = (x2 - x1) as u16 + 1;
            let h = (191 - y1 - y) as u16 + 1;
            let vertex_x = ((vertex.coords[0] as i64 + vertex.coords[3] as i64) * w as i64 / (vertex.coords[3] as i64 * 2) + x as i64) as i32;
            let vertex_y = ((-vertex.coords[1] as i64 + vertex.coords[3] as i64) * h as i64 / (vertex.coords[3] as i64 * 2) + y as i64) as i32;
            let vertex_z = (((vertex.coords[2] as i64) << 12) / vertex.coords[3] as i64) as i32;

            Gpu3DVertex {
                coords: [
                    vertex_x as f32 / 255f32 * 2f32 - 1f32,
                    1f32 - vertex_y as f32 / 191f32 * 2f32,
                    (vertex_z as f32 / 4096f32) * 0.5 - 0.5,
                    polygon_index as f32,
                ],
                color: [c.0, c.1, c.2],
                tex_coords: [vertex.tex_coords[0] as f32 / 16f32, vertex.tex_coords[1] as f32 / 16f32],
            }
        } else {
            unsafe { mem::zeroed() }
        }
    }
}

#[derive(Default)]
#[repr(C)]
struct Gpu3dPolygonAttr {
    tex_image_param: u32,
    pal_addr: u32,
}

const_assert_eq!(size_of::<Gpu3dPolygonAttr>(), 8);

#[derive(Default)]
pub struct Gpu3DRenderer {
    pub dirty: bool,
    inners: [Gpu3DRendererInner; 2],
    vertices: HeapMem<Vertex, VERTEX_LIMIT>,
    vertex_count: u16,
    polygons: HeapMem<Polygon, POLYGON_LIMIT>,
    polygon_count: u16,
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

        self.vertex_count = registers.vertices.count_out;
        mem::swap(&mut self.vertices, &mut registers.vertices.outs);

        self.polygon_count = registers.polygons.count_out;
        mem::swap(&mut self.polygons, &mut registers.polygons.outs);
    }

    pub unsafe fn render(&mut self, common: &GpuRendererCommon) {
        self.vertices_buf.clear();
        self.indices_buf.clear();
        for i in 0..self.polygon_count {
            let polygon = &self.polygons[i as usize];

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
            //     "polygon {i} format {} addr {:x} pal addr {:x} size s {} size t {} repeat s {} flip s {} repeat t {} flip t {}",
            //     u8::from(polygon.tex_image_param.format()),
            //     u32::from(polygon.tex_image_param.vram_offset()) << 3,
            //     (polygon.palette_addr as u32) << 3,
            //     8 << u8::from(polygon.tex_image_param.size_s_shift()),
            //     8 << u8::from(polygon.tex_image_param.size_t_shift()),
            //     polygon.tex_image_param.repeat_s(),
            //     polygon.tex_image_param.flip_s(),
            //     polygon.tex_image_param.repeat_t(),
            //     polygon.tex_image_param.flip_t(),
            // );

            let vertex_index = self.vertices_buf.len() as u16;
            self.indices_buf.push(vertex_index);
            self.indices_buf.push(vertex_index + 1);
            if polygon.crossed {
                self.indices_buf.push(vertex_index + 3);
            } else {
                self.indices_buf.push(vertex_index + 2);
            }

            for j in 3..polygon.size as u16 {
                self.indices_buf.push(vertex_index);
                self.indices_buf.push(vertex_index + j - 1);
                self.indices_buf.push(vertex_index + j);
            }

            for j in 0..polygon.size {
                self.vertices_buf.push(Gpu3DVertex::from((&self.vertices[polygon.vertices_index as usize + j as usize], i)));

                // println!(
                //     "vertex {j} s {} t {} s_norm {} t_norm {}",
                //     self.vertices[polygon.vertices_index + j].tex_coords[0],
                //     self.vertices[polygon.vertices_index + j].tex_coords[1],
                //     self.vertices_buf[self.vertices_buf.len() - 1].tex_coords[0],
                //     self.vertices_buf[self.vertices_buf.len() - 1].tex_coords[1],
                // )
            }

            self.polygon_attrs[i as usize].tex_image_param = u32::from(polygon.tex_image_param);
            self.polygon_attrs[i as usize].pal_addr = polygon.palette_addr as u32;
        }

        gl::BindFramebuffer(gl::FRAMEBUFFER, self.gl.fbo.fbo);
        gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
        gl::ClearColor(0f32, 0f32, 0f32, 0f32);
        gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

        if self.vertices_buf.is_empty() {
            return;
        }

        gl::UseProgram(self.gl.program);

        gl::Enable(gl::BLEND);
        gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        gl::Enable(gl::DEPTH_TEST);
        gl::DepthFunc(gl::LESS);

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
        gl::VertexAttribPointer(1, 3, gl::FLOAT, gl::FALSE, size_of::<Gpu3DVertex>() as _, (size_of::<f32>() * 4) as _);

        gl::EnableVertexAttribArray(2);
        gl::VertexAttribPointer(2, 2, gl::FLOAT, gl::FALSE, size_of::<Gpu3DVertex>() as _, (size_of::<f32>() * 7) as _);

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
