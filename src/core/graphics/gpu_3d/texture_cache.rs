use crate::core::graphics::gl_utils::GpuFbo;
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_3d::registers_3d::Polygon;
use crate::core::graphics::gpu_3d::renderer_3d::Gpu3DGl;
use crate::core::graphics::gpu_shaders::GpuShadersPrograms;
use crate::utils;
use crate::utils::{array_init, rgb5_to_float8, NoHashMap};
use bilge::prelude::*;
use gl::types::{GLint, GLuint};
use std::collections::HashMap;
use std::fs::File;
use std::hint::assert_unchecked;
use std::os::unix::fs::FileExt;
use std::time::{Instant, SystemTime};
use std::{mem, ptr, time};

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
struct Texture3DMetadata {
    s_shift: u3,
    t_shift: u3,
    format: u3,
    color_0_transparent: bool,
    unused: u6,
}

pub struct Texture3D {
    vram_addr: u16,
    pal_addr: u16,
    metadata: Texture3DMetadata,
    last_used: Instant,
    color_tex: GLuint,
}

impl Texture3D {
    fn new(polygon: &Polygon) -> Self {
        let color_tex = unsafe {
            let mut tex = 0;
            gl::GenTextures(1, &mut tex);
            gl::BindTexture(gl::TEXTURE_2D, tex);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as _,
                (8 << u8::from(polygon.tex_image_param.size_s_shift())) as _,
                (8 << u8::from(polygon.tex_image_param.size_t_shift())) as _,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                ptr::null(),
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as _);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            tex
        };

        Texture3D {
            vram_addr: polygon.tex_image_param.vram_offset(),
            pal_addr: polygon.palette_addr,
            metadata: Texture3DMetadata::new(
                polygon.tex_image_param.size_s_shift(),
                polygon.tex_image_param.size_t_shift(),
                polygon.tex_image_param.format(),
                polygon.tex_image_param.color_0_transparent(),
                u6::new(0),
            ),
            last_used: Instant::now(),
            color_tex,
        }
    }

    fn width(&self) -> u32 {
        8 << u8::from(self.metadata.s_shift())
    }

    fn height(&self) -> u32 {
        8 << u8::from(self.metadata.t_shift())
    }

    fn size(&self) -> u32 {
        1 << (u8::from(self.metadata.s_shift()) + u8::from(self.metadata.t_shift()) + 6)
    }
}

impl Drop for Texture3D {
    fn drop(&mut self) {
        unsafe { gl::DeleteTextures(1, &self.color_tex) };
    }
}

const CACHE_SIZE_LIMIT: u32 = 8 * 1024 * 1024;

pub struct Texture3DCache {
    program: GLuint,
    tex_fmt_loc: GLint,
    color_transparent_loc: GLint,
    vram_addr_loc: GLint,
    pal_addr_loc: GLint,
    size_s_loc: GLint,
    cache: HashMap<u64, Texture3D, utils::BuildNoHasher64>,
    total_size: u32,
}

impl Texture3DCache {
    pub fn new(gpu_programs: &GpuShadersPrograms) -> Self {
        unsafe {
            gl::UseProgram(gpu_programs.texture_cache_3d);

            let tex_fmt_loc = gl::GetUniformLocation(gpu_programs.texture_cache_3d, c"texFmt".as_ptr() as _);
            let color_transparent_loc = gl::GetUniformLocation(gpu_programs.texture_cache_3d, c"colorTransparent".as_ptr() as _);
            let vram_addr_loc = gl::GetUniformLocation(gpu_programs.texture_cache_3d, c"vramAddr".as_ptr() as _);
            let pal_addr_loc = gl::GetUniformLocation(gpu_programs.texture_cache_3d, c"palAddr".as_ptr() as _);
            let size_s_loc = gl::GetUniformLocation(gpu_programs.texture_cache_3d, c"sizeS".as_ptr() as _);

            gl::BindAttribLocation(gpu_programs.texture_cache_3d, 0, c"position".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(gpu_programs.texture_cache_3d, c"tex".as_ptr() as _), 0);
            gl::Uniform1i(gl::GetUniformLocation(gpu_programs.texture_cache_3d, c"palTex".as_ptr() as _), 1);

            gl::UseProgram(0);

            Texture3DCache {
                program: gpu_programs.texture_cache_3d,
                tex_fmt_loc,
                color_transparent_loc,
                vram_addr_loc,
                pal_addr_loc,
                size_s_loc,
                cache: HashMap::default(),
                total_size: 0,
            }
        }
    }

    unsafe fn load_texture(&self, texture_3d: &Texture3D, gl_3d: &Gpu3DGl) {
        let mut fbo = 0;
        gl::GenFramebuffers(1, &mut fbo);
        gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
        gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, texture_3d.color_tex, 0);

        gl::Viewport(0, 0, texture_3d.width() as _, texture_3d.height() as _);
        gl::ClearColor(0.0, 0.0, 0.0, 0.0);
        gl::Clear(gl::COLOR_BUFFER_BIT);

        gl::UseProgram(self.program);

        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindTexture(gl::TEXTURE_2D, gl_3d.tex);

        gl::ActiveTexture(gl::TEXTURE1);
        gl::BindTexture(gl::TEXTURE_2D, gl_3d.pal_tex);

        gl::Uniform1i(self.tex_fmt_loc, u8::from(texture_3d.metadata.format()) as _);
        gl::Uniform1i(self.color_transparent_loc, texture_3d.metadata.color_0_transparent() as _);
        gl::Uniform1i(self.vram_addr_loc, ((texture_3d.vram_addr as u32) << 3) as _);
        gl::Uniform1i(self.pal_addr_loc, texture_3d.pal_addr as _);
        gl::Uniform1i(self.size_s_loc, texture_3d.width() as _);

        const COORDS: [f32; 2 * 4] = [-1f32, 1f32, 1f32, 1f32, 1f32, -1f32, -1f32, -1f32];

        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, COORDS.as_ptr() as _);
        gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);

        // static mut PIXEL_BUF: Vec<u8> = Vec::new();
        //
        // PIXEL_BUF.resize((texture_3d.size() << 2) as _, 0);
        // // gl::BindFramebuffer(gl::READ_FRAMEBUFFER, texture_3d.fbo.fbo);
        // gl::BindFramebuffer(gl::READ_FRAMEBUFFER, fbo);
        // gl::ReadPixels(0, 0, texture_3d.width() as _, texture_3d.height() as _, gl::RGBA, gl::UNSIGNED_BYTE, PIXEL_BUF.as_mut_ptr() as _);
        // let file = File::create(format!(
        //     "texture_{}_{}_{:?}.bmp",
        //     texture_3d.width(),
        //     texture_3d.height(),
        //     SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_micros()
        // ))
        // .unwrap();
        //
        // let mut header = [
        //     0x42, 0x4D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x36, 0x00, 0x00, 0x00, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x20, 0x00, 0x00,
        //     0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x13, 0x0B, 0x00, 0x00, 0x13, 0x0B, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // ];
        // utils::write_to_mem::<u32>(&mut header, 0x2, 54 + PIXEL_BUF.len() as u32);
        // utils::write_to_mem::<u32>(&mut header, 0x12, texture_3d.width() as _);
        // utils::write_to_mem::<u32>(&mut header, 0x16, texture_3d.height() as _);
        // utils::write_to_mem::<u32>(&mut header, 0x22, PIXEL_BUF.len() as u32);
        // file.write_all_at(&header, 0).unwrap();
        // file.write_all_at(&PIXEL_BUF, header.len() as u64).unwrap();

        gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, 0, 0);
        gl::UseProgram(0);
        gl::DeleteFramebuffers(1, &fbo);
    }

    pub fn clear(&mut self) {
        println!("texture cache clear");
        self.cache.clear();
    }

    pub fn get(&mut self, polygon: &Polygon, gl_3d: &Gpu3DGl) -> &Texture3D {
        let cache_key = polygon.tex_image_param.key() as u64 | ((polygon.palette_addr as u64) << 32);
        if let Some(texture_3d) = self.cache.get_mut(&cache_key) {
            texture_3d.last_used = Instant::now();
            return unsafe { mem::transmute(texture_3d) };
        }

        let texture_3d = Texture3D::new(polygon);
        while self.total_size + texture_3d.size() >= CACHE_SIZE_LIMIT {
            let mut oldest_key = 0;
            let mut oldest_timestamp = Instant::now();
            let mut oldest_size = 0;
            unsafe { assert_unchecked(!self.cache.is_empty()) };
            for (&key, texture_3d) in &self.cache {
                if texture_3d.last_used < oldest_timestamp {
                    oldest_key = key;
                    oldest_timestamp = texture_3d.last_used;
                    oldest_size = texture_3d.size();
                }
            }
            self.total_size -= oldest_size;
            unsafe { self.cache.remove(&oldest_key).unwrap_unchecked() };
        }
        self.total_size += texture_3d.size();
        println!("texture cache 3d insert {}", self.cache.len());
        self.cache.insert(cache_key, texture_3d);
        unsafe {
            let texture_3d = self.cache.get(&cache_key).unwrap_unchecked();
            self.load_texture(&texture_3d, gl_3d);
            texture_3d
        }
    }
}
