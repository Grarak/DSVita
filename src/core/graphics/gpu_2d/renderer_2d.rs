use crate::core::graphics::gl_utils::{
    create_mem_texture1d, create_mem_texture2d, create_pal_texture1d, create_pal_texture2d, create_program, create_shader, shader_source, sub_mem_texture1d, sub_mem_texture2d, sub_pal_texture1d,
    sub_pal_texture2d, GpuFbo,
};
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_2d::registers_2d::{BgCnt, DispCnt, Gpu2DRegisters};
use crate::core::graphics::gpu_2d::Gpu2DEngine;
use crate::core::graphics::gpu_2d::Gpu2DEngine::{A, B};
use crate::core::graphics::gpu_mem_buf::GpuMemBuf;
use crate::core::graphics::gpu_renderer::GpuRendererCommon;
use crate::core::memory::oam::{OamAttrib0, OamAttrib1, OamAttrib2, OamAttribs, OamGfxMode, OamObjMode};
use crate::core::memory::regions;
use crate::settings::{self, Settings};
use crate::utils;
use crate::utils::rgb5_to_float8;
use gl::types::{GLint, GLuint};
use static_assertions::const_assert;
use std::hint::{assert_unchecked, unreachable_unchecked};
use std::intrinsics::unlikely;
use std::{mem, ptr, slice};

pub struct Gpu2DMem {
    lcdc_ptr: *const u8,
    bg_ptr: *const u8,
    obj_ptr: *const u8,
    pal_ptr: *const u8,
    oam_ptr: *const u8,
    bg_ext_pal_ptr: *const u8,
    obj_ext_pal_ptr: *const u8,
}

impl Gpu2DMem {
    fn new<const ENGINE: Gpu2DEngine>(buf: &GpuMemBuf) -> Self {
        match ENGINE {
            A => Gpu2DMem {
                lcdc_ptr: buf.lcdc.as_ptr(),
                bg_ptr: buf.bg_a.as_ptr(),
                obj_ptr: buf.obj_a.as_ptr(),
                pal_ptr: buf.pal_a.as_ptr(),
                oam_ptr: buf.oam_a.as_ptr(),
                bg_ext_pal_ptr: buf.bg_a_ext_palette.as_ptr(),
                obj_ext_pal_ptr: buf.obj_a_ext_palette.as_ptr(),
            },
            B => Gpu2DMem {
                lcdc_ptr: ptr::null(),
                bg_ptr: buf.bg_b.as_ptr(),
                obj_ptr: buf.obj_b.as_ptr(),
                pal_ptr: buf.pal_b.as_ptr(),
                oam_ptr: buf.oam_b.as_ptr(),
                bg_ext_pal_ptr: buf.bg_b_ext_palette.as_ptr(),
                obj_ext_pal_ptr: buf.obj_b_ext_palette.as_ptr(),
            },
        }
    }
}

const BG_A_TEX_HEIGHT: u32 = 512;
const BG_B_TEX_HEIGHT: u32 = 128;
const OBJ_A_TEX_HEIGHT: u32 = 256;
const OBJ_B_TEX_HEIGHT: u32 = 128;

#[derive(Clone)]
#[repr(C)]
struct WinBgUbo {
    win_h: [u32; DISPLAY_HEIGHT * 2],
    win_v: [u32; DISPLAY_HEIGHT * 2],
    win_in: [u32; DISPLAY_HEIGHT],
    win_out: [u32; DISPLAY_HEIGHT],
}

const_assert!(size_of::<WinBgUbo>() <= 16 * 1024);

#[derive(Clone)]
#[repr(C)]
struct BgUbo {
    ofs: [u32; DISPLAY_HEIGHT * 4],
    x: [i32; DISPLAY_HEIGHT * 2],
    y: [i32; DISPLAY_HEIGHT * 2],
    pa: [i32; DISPLAY_HEIGHT * 2],
    pb: [i32; DISPLAY_HEIGHT * 2],
    pc: [i32; DISPLAY_HEIGHT * 2],
    pd: [i32; DISPLAY_HEIGHT * 2],
}

const_assert!(size_of::<BgUbo>() <= 16 * 1024);

#[derive(Clone)]
pub struct Gpu2DRenderRegs {
    disp_cnts: [u32; DISPLAY_HEIGHT],
    bg_cnts: [u16; DISPLAY_HEIGHT * 4],
    win_bg_ubo: WinBgUbo,
    bg_ubo: BgUbo,
    blend_ubo: BlendUbo,
    batch_counts: [u8; DISPLAY_HEIGHT],
    current_batch_count_index: usize,
}

impl Gpu2DRenderRegs {
    fn reset(&mut self) {
        self.disp_cnts = unsafe { mem::zeroed() };
        self.bg_cnts = unsafe { mem::zeroed() };
        self.batch_counts = unsafe { mem::zeroed() };
        self.current_batch_count_index = 0;
    }
}

impl Default for Gpu2DRenderRegs {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

impl Gpu2DRenderRegs {
    fn on_scanline(&mut self, inner: &mut Gpu2DRegisters, line: u8) {
        let line = line as usize;
        unsafe { assert_unchecked(self.current_batch_count_index < DISPLAY_HEIGHT && line < DISPLAY_HEIGHT) };
        let mut updated = self.disp_cnts[self.current_batch_count_index] != u32::from(inner.disp_cnt);
        for i in 0..4 {
            updated |= self.bg_cnts[self.current_batch_count_index * 4 + i] != u16::from(inner.bg_cnt[i]);
        }

        if updated {
            self.disp_cnts[line] = u32::from(inner.disp_cnt);
            for i in 0..4 {
                self.bg_cnts[line * 4 + i] = u16::from(inner.bg_cnt[i]);
            }
            self.current_batch_count_index = line;
        } else {
            self.batch_counts[self.current_batch_count_index] += 1;
        }

        for i in 0..2 {
            self.win_bg_ubo.win_h[i * DISPLAY_HEIGHT + line] = inner.win_h[i] as u32;
            self.win_bg_ubo.win_v[i * DISPLAY_HEIGHT + line] = inner.win_v[i] as u32;
        }
        self.win_bg_ubo.win_in[line] = inner.win_in as u32;
        self.win_bg_ubo.win_out[line] = inner.win_out as u32;

        for i in 0..4 {
            self.bg_ubo.ofs[i * DISPLAY_HEIGHT + line] = (inner.bg_h_ofs[i] as u32) | ((inner.bg_v_ofs[i] as u32) << 16);
        }
        for i in 0..2 {
            self.bg_ubo.x[i * DISPLAY_HEIGHT + line] = inner.bg_x[i];
            self.bg_ubo.y[i * DISPLAY_HEIGHT + line] = inner.bg_y[i];
            self.bg_ubo.pa[i * DISPLAY_HEIGHT + line] = inner.bg_pa[i] as i32;
            self.bg_ubo.pc[i * DISPLAY_HEIGHT + line] = inner.bg_pc[i] as i32;
        }

        if unlikely(line == 0) || inner.bg_x_dirty {
            self.bg_ubo.pb[line] = 0;
            self.bg_ubo.pb[DISPLAY_HEIGHT + line] = 0;
            inner.bg_x_dirty = false;
        } else {
            self.bg_ubo.pb[line] = inner.bg_pb[0] as i32 + self.bg_ubo.pb[line - 1];
            self.bg_ubo.pb[DISPLAY_HEIGHT + line] = inner.bg_pb[1] as i32 + self.bg_ubo.pb[DISPLAY_HEIGHT + line - 1];
        }

        if unlikely(line == 0) || inner.bg_y_dirty {
            self.bg_ubo.pd[line] = 0;
            self.bg_ubo.pd[DISPLAY_HEIGHT + line] = 0;
            inner.bg_y_dirty = false;
        } else {
            self.bg_ubo.pd[line] = inner.bg_pd[0] as i32 + self.bg_ubo.pd[line - 1];
            self.bg_ubo.pd[DISPLAY_HEIGHT + line] = inner.bg_pd[1] as i32 + self.bg_ubo.pd[DISPLAY_HEIGHT + line - 1];
        }

        self.blend_ubo.bld_cnts[line] = inner.bld_cnt as u32;
        self.blend_ubo.bld_alphas[line] = inner.bld_alpha as u32;
        self.blend_ubo.bld_ys[line] = inner.bld_y as u32;
    }
}

const fn generate_obj_vertices() -> [f32; 128 * 4 * 2] {
    let mut vertices: [f32; 128 * 4 * 2] = unsafe { mem::zeroed() };
    let mut i = 0;
    while i < vertices.len() {
        // top left
        vertices[i] = 0f32;
        vertices[i + 1] = 0f32;
        // top right
        vertices[i + 2] = 1f32;
        vertices[i + 3] = 0f32;
        // bottom right
        vertices[i + 4] = 1f32;
        vertices[i + 5] = 1f32;
        // bottom left
        vertices[i + 6] = 0f32;
        vertices[i + 7] = 1f32;
        i += 4 * 2;
    }
    vertices
}

const OBJ_VERTICES: [f32; 128 * 4 * 2] = generate_obj_vertices();

const fn generate_oam_indices() -> [u8; 128 * 4] {
    let mut indices: [u8; 128 * 4] = unsafe { mem::zeroed() };
    let mut i = 0;
    while i < indices.len() {
        indices[i] = (i / 4) as u8;
        indices[i + 1] = (i / 4) as u8;
        indices[i + 2] = (i / 4) as u8;
        indices[i + 3] = (i / 4) as u8;
        i += 4;
    }
    indices
}

const OBJ_OAM_INDICES: [u8; 128 * 4] = generate_oam_indices();

#[repr(C)]
struct ObjUbo {
    map_widths: [u32; 128],
    obj_bounds: [u32; 128],
}

const_assert!(size_of::<ObjUbo>() <= 16 * 1024);

impl Default for ObjUbo {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

#[derive(Clone)]
#[repr(C)]
struct BlendUbo {
    bld_cnts: [u32; DISPLAY_HEIGHT],
    bld_alphas: [u32; DISPLAY_HEIGHT],
    bld_ys: [u32; DISPLAY_HEIGHT],
}

const_assert!(size_of::<BlendUbo>() <= 16 * 1024);

struct Gpu2DTextures {
    oam: GLuint,
    obj: GLuint,
    obj_width: u32,
    obj_height: u32,
    bg: GLuint,
    bg_width: u32,
    bg_height: u32,
    pal: GLuint,
    bg_ext_pal: GLuint,
    obj_ext_pal: GLuint,
}

impl Gpu2DTextures {
    fn new(obj_width: u32, obj_height: u32, bg_width: u32, bg_height: u32) -> Self {
        unsafe {
            Gpu2DTextures {
                oam: create_mem_texture1d(regions::OAM_SIZE / 2),
                obj: create_mem_texture2d(obj_width, obj_height),
                obj_width,
                obj_height,
                bg: create_mem_texture2d(bg_width, bg_height),
                bg_width,
                bg_height,
                pal: create_pal_texture1d(regions::STANDARD_PALETTES_SIZE / 2),
                bg_ext_pal: create_pal_texture2d(1024, 32),
                obj_ext_pal: create_pal_texture2d(1024, 8),
            }
        }
    }
}

pub struct Gpu2DCommon {
    win_bg_program: GLuint,
    win_bg_disp_cnt_loc: GLint,
    win_bg_ubo: GLuint,
    win_bg_fbo: GpuFbo,
    obj_fbo: GpuFbo,
    bg_fbos: [GpuFbo; 4],
    blend_program: GLuint,
    blend_ubo: GLuint,
    pub blend_fbo: GpuFbo,
    rotate_program: GLuint,
    pub rotate_fbo: GpuFbo
}

impl Gpu2DCommon {
    fn new() -> Self {
        unsafe {
            let (win_bg_program, win_bg_disp_cnt_loc, win_bg_ubo, win_bg_fbo) = {
                let vert_shader = create_shader("win", shader_source!("win_bg_vert"), gl::VERTEX_SHADER).unwrap();
                let frag_shader = create_shader("win", shader_source!("win_bg_frag"), gl::FRAGMENT_SHADER).unwrap();
                let program = create_program(&[vert_shader, frag_shader]).unwrap();
                gl::DeleteShader(vert_shader);
                gl::DeleteShader(frag_shader);

                gl::UseProgram(program);

                gl::BindAttribLocation(program, 0, "position\0".as_ptr() as _);

                let disp_cnt_loc = gl::GetUniformLocation(program, "dispCnt\0".as_ptr() as _);

                let mut ubo = 0;
                gl::GenBuffers(1, &mut ubo);
                gl::BindBuffer(gl::UNIFORM_BUFFER, ubo);

                if cfg!(target_os = "linux") {
                    gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, "WinBgUbo\0".as_ptr() as _), 0);
                }

                gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
                gl::UseProgram(0);

                let fbo = GpuFbo::new(DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32, false).unwrap();

                (program, disp_cnt_loc, ubo, fbo)
            };

            let (blend_program, blend_ubo, blend_fbo) = {
                let vert_shader = create_shader("blend", shader_source!("blend_vert"), gl::VERTEX_SHADER).unwrap();
                let frag_shader = create_shader("blend", shader_source!("blend_frag"), gl::FRAGMENT_SHADER).unwrap();
                let program = create_program(&[vert_shader, frag_shader]).unwrap();
                gl::DeleteShader(vert_shader);
                gl::DeleteShader(frag_shader);

                gl::UseProgram(program);

                gl::BindAttribLocation(program, 0, "position\0".as_ptr() as _);

                gl::Uniform1i(gl::GetUniformLocation(program, "bg0Tex\0".as_ptr() as _), 0);
                gl::Uniform1i(gl::GetUniformLocation(program, "bg1Tex\0".as_ptr() as _), 1);
                gl::Uniform1i(gl::GetUniformLocation(program, "bg2Tex\0".as_ptr() as _), 2);
                gl::Uniform1i(gl::GetUniformLocation(program, "bg3Tex\0".as_ptr() as _), 3);
                gl::Uniform1i(gl::GetUniformLocation(program, "objTex\0".as_ptr() as _), 4);
                gl::Uniform1i(gl::GetUniformLocation(program, "objDepthTex\0".as_ptr() as _), 5);
                gl::Uniform1i(gl::GetUniformLocation(program, "winTex\0".as_ptr() as _), 6);

                let mut ubo = 0;
                gl::GenBuffers(1, &mut ubo);
                gl::BindBuffer(gl::UNIFORM_BUFFER, ubo);

                if cfg!(target_os = "linux") {
                    gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, "BlendUbo\0".as_ptr() as _), 0);
                }

                gl::UseProgram(0);

                let fbo = GpuFbo::new(DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32, false).unwrap();

                (program, ubo, fbo)
            };

            let (rotate_program, rotate_fbo) = {
                let vert_shader = create_shader("rotate", shader_source!("rotate_vert"), gl::VERTEX_SHADER).unwrap();
                let frag_shader = create_shader("rotate", shader_source!("rotate_frag"), gl::FRAGMENT_SHADER).unwrap();
                let program = create_program(&[vert_shader, frag_shader]).unwrap();
                gl::DeleteShader(vert_shader);
                gl::DeleteShader(frag_shader);

                gl::UseProgram(program);

                gl::BindAttribLocation(program, 0, "position\0".as_ptr() as _);
                gl::Uniform1i(gl::GetUniformLocation(program, "tex\0".as_ptr() as _), 0);

                gl::UseProgram(0);
                
                let fbo = GpuFbo::new(DISPLAY_HEIGHT as u32, DISPLAY_WIDTH as u32, false).unwrap();

                (program, fbo)
            };

            Gpu2DCommon {
                win_bg_program,
                win_bg_disp_cnt_loc,
                win_bg_ubo,
                win_bg_fbo,
                obj_fbo: GpuFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, true).unwrap(),
                bg_fbos: [
                    GpuFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false).unwrap(),
                    GpuFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false).unwrap(),
                    GpuFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false).unwrap(),
                    GpuFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false).unwrap(),
                ],
                blend_program,
                blend_ubo,
                blend_fbo,
                rotate_program,
                rotate_fbo
            }
        }
    }
}

#[repr(u8)]
#[derive(Debug, Eq, PartialEq)]
enum BgMode {
    Text = 0,
    Affine = 1,
    Extended = 2,
    Large = 3,
    Display3d = 4,
}

struct Gpu2DVramDisplayProgram {
    disp_cnt_loc: GLint,
    program: GLuint,
}

impl Gpu2DVramDisplayProgram {
    fn new() -> Self {
        unsafe {
            let vert_shader = create_shader("vram display", shader_source!("vram_display_vert"), gl::VERTEX_SHADER).unwrap();
            let frag_shader = create_shader("vram display", shader_source!("vram_display_frag"), gl::FRAGMENT_SHADER).unwrap();

            let program = create_program(&[vert_shader, frag_shader]).unwrap();
            gl::DeleteShader(vert_shader);
            gl::DeleteShader(frag_shader);

            gl::UseProgram(program);

            gl::BindAttribLocation(program, 0, "position\0".as_ptr() as _);

            let disp_cnt_loc = gl::GetUniformLocation(program, "dispCnt\0".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(program, "lcdcPalTex\0".as_ptr() as _), 0);

            gl::UseProgram(0);

            Gpu2DVramDisplayProgram { disp_cnt_loc, program }
        }
    }

    unsafe fn draw(&self, regs: &Gpu2DRenderRegs, from_line: u8, to_line: u8) {
        let disp_cnt = regs.disp_cnts[from_line as usize];

        #[rustfmt::skip]
        let vertices = [
            -1f32, from_line as f32,
            1f32, from_line as f32,
            1f32, to_line as f32,
            -1f32, to_line as f32,
        ];

        gl::Uniform1i(self.disp_cnt_loc, disp_cnt as _);

        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, vertices.as_ptr() as _);
        gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
    }
}

struct Gpu2DBgProgram {
    program: GLuint,
    disp_cnt_loc: GLint,
    cnt_loc: GLint,
    has_ubo: bool,
}

struct Gpu2DProgram {
    obj_program: GLuint,
    obj_vao: GLuint,
    obj_oam_indices: Vec<[u16; 6]>,
    obj_ubo_data: ObjUbo,
    obj_disp_cnt_loc: GLint,
    obj_ubo: GLuint,

    bg_affine_extended_program: Gpu2DBgProgram,
    bg_bitmap_program: Gpu2DBgProgram,
    bg_display_3d_program: Gpu2DBgProgram,
    bg_text_4bpp_program: Gpu2DBgProgram,
    bg_text_8bpp_program: Gpu2DBgProgram,
    bg_ubo: GLuint,

    rotate_vao: GLuint,
}

impl Gpu2DProgram {
    fn new<const ENGINE: Gpu2DEngine>(obj_vert_shader: GLuint, bg_vert_shader: GLuint, bg_vert_affine_extended_shader: GLuint, bg_vert_bitmap_shader: GLuint) -> Self {
        unsafe {
            let (obj_program, obj_vao, obj_disp_cnt_loc, obj_ubo) = {
                let frag_shader_src = shader_source!("obj_frag").replace(
                    "OBJ_TEX_HEIGHT",
                    &format!(
                        "{}.0",
                        match ENGINE {
                            A => OBJ_A_TEX_HEIGHT / 2,
                            B => OBJ_B_TEX_HEIGHT / 2,
                        }
                    ),
                );

                let frag_shader = create_shader("obj", &frag_shader_src, gl::FRAGMENT_SHADER).unwrap();
                let program = create_program(&[obj_vert_shader, frag_shader]).unwrap();
                gl::DeleteShader(frag_shader);

                gl::UseProgram(program);

                let mut vertices_buf = 0;
                gl::GenBuffers(1, &mut vertices_buf);
                gl::BindBuffer(gl::ARRAY_BUFFER, vertices_buf);
                gl::BufferData(gl::ARRAY_BUFFER, (size_of::<f32>() * OBJ_VERTICES.len()) as _, OBJ_VERTICES.as_ptr() as _, gl::STATIC_DRAW);

                let mut indices_buf = 0;
                gl::GenBuffers(1, &mut indices_buf);
                gl::BindBuffer(gl::ARRAY_BUFFER, indices_buf);
                gl::BufferData(gl::ARRAY_BUFFER, OBJ_OAM_INDICES.len() as _, OBJ_OAM_INDICES.as_ptr() as _, gl::STATIC_DRAW);

                let mut vao = 0;
                gl::GenVertexArrays(1, &mut vao);
                gl::BindVertexArray(vao);

                gl::BindBuffer(gl::ARRAY_BUFFER, vertices_buf);
                gl::EnableVertexAttribArray(0);
                gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, ptr::null());

                gl::BindBuffer(gl::ARRAY_BUFFER, indices_buf);
                gl::EnableVertexAttribArray(1);
                gl::VertexAttribPointer(1, 1, gl::UNSIGNED_BYTE, gl::FALSE, 0, ptr::null());

                gl::BindVertexArray(0);
                gl::BindBuffer(gl::ARRAY_BUFFER, 0);

                gl::BindAttribLocation(program, 0, "position\0".as_ptr() as _);
                gl::BindAttribLocation(program, 1, "oamIndex\0".as_ptr() as _);

                gl::Uniform1i(gl::GetUniformLocation(program, "oamTex\0".as_ptr() as _), 0);
                gl::Uniform1i(gl::GetUniformLocation(program, "objTex\0".as_ptr() as _), 1);
                gl::Uniform1i(gl::GetUniformLocation(program, "palTex\0".as_ptr() as _), 2);
                gl::Uniform1i(gl::GetUniformLocation(program, "extPalTex\0".as_ptr() as _), 3);
                gl::Uniform1i(gl::GetUniformLocation(program, "winTex\0".as_ptr() as _), 4);

                let disp_cnt_loc = gl::GetUniformLocation(program, "dispCnt\0".as_ptr() as _);

                let mut ubo = 0;
                gl::GenBuffers(1, &mut ubo);
                gl::BindBuffer(gl::UNIFORM_BUFFER, ubo);

                if cfg!(target_os = "linux") {
                    gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, "ObjUbo\0".as_ptr() as _), 0);
                }

                gl::UseProgram(0);

                (program, vao, disp_cnt_loc, ubo)
            };

            let (bg_affine_extended_program, bg_bitmap_program, bg_display_3d_program, bg_text_4bpp_program, bg_text_8bpp_program, bg_ubo) = {
                let frag_common_shader_src = shader_source!("bg_frag_common").replace(
                    "BG_TEX_HEIGHT",
                    &format!(
                        "{}.0",
                        match ENGINE {
                            A => BG_A_TEX_HEIGHT / 2,
                            B => BG_B_TEX_HEIGHT / 2,
                        }
                    ),
                );

                let frag_affine_extended_shader = create_shader("bg affine extended", &(frag_common_shader_src.clone() + shader_source!("bg_frag_affine_extended")), gl::FRAGMENT_SHADER).unwrap();
                let frag_bitmap_shader = create_shader("bg bitmap", &(frag_common_shader_src.clone() + shader_source!("bg_frag_bitmap")), gl::FRAGMENT_SHADER).unwrap();
                let frag_display_3d_shader = create_shader("bg display 3d", &(frag_common_shader_src.clone() + shader_source!("bg_frag_display_3d")), gl::FRAGMENT_SHADER).unwrap();
                let frag_text_4bpp_shader = create_shader("bg text 4bpp", &(frag_common_shader_src.clone() + shader_source!("bg_frag_text_4bpp")), gl::FRAGMENT_SHADER).unwrap();
                let frag_text_8bpp_shader = create_shader("bg text 8bpp", &(frag_common_shader_src + shader_source!("bg_frag_text_8bpp")), gl::FRAGMENT_SHADER).unwrap();

                let affine_extended_program = create_program(&[bg_vert_affine_extended_shader, frag_affine_extended_shader]).unwrap();
                let bitmap_program = create_program(&[bg_vert_bitmap_shader, frag_bitmap_shader]).unwrap();
                let display_3d_program = create_program(&[bg_vert_shader, frag_display_3d_shader]).unwrap();
                let text_4bpp_program = create_program(&[bg_vert_shader, frag_text_4bpp_shader]).unwrap();
                let text_8bpp_program = create_program(&[bg_vert_shader, frag_text_8bpp_shader]).unwrap();

                gl::DeleteShader(frag_affine_extended_shader);
                gl::DeleteShader(frag_bitmap_shader);
                gl::DeleteShader(frag_display_3d_shader);
                gl::DeleteShader(frag_text_4bpp_shader);
                gl::DeleteShader(frag_text_8bpp_shader);

                let mut ubo = 0;
                gl::GenBuffers(1, &mut ubo);

                let init_program = |program: GLuint, has_ubo: bool| {
                    gl::UseProgram(program);

                    gl::BindAttribLocation(program, 0, "position\0".as_ptr() as _);

                    let disp_cnt_loc = gl::GetUniformLocation(program, "dispCnt\0".as_ptr() as _);
                    let cnt_loc = gl::GetUniformLocation(program, "bgCnt\0".as_ptr() as _);

                    gl::Uniform1i(gl::GetUniformLocation(program, "bgTex\0".as_ptr() as _), 0);
                    gl::Uniform1i(gl::GetUniformLocation(program, "palTex\0".as_ptr() as _), 1);
                    gl::Uniform1i(gl::GetUniformLocation(program, "extPalTex\0".as_ptr() as _), 2);
                    gl::Uniform1i(gl::GetUniformLocation(program, "winTex\0".as_ptr() as _), 3);
                    gl::Uniform1i(gl::GetUniformLocation(program, "display3dTex\0".as_ptr() as _), 4);

                    if cfg!(target_os = "linux") && has_ubo {
                        gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, "BgUbo\0".as_ptr() as _), 0);
                    }

                    gl::UseProgram(0);

                    Gpu2DBgProgram {
                        program,
                        disp_cnt_loc,
                        cnt_loc,
                        has_ubo,
                    }
                };

                (
                    init_program(affine_extended_program, true),
                    init_program(bitmap_program, true),
                    init_program(display_3d_program, false),
                    init_program(text_4bpp_program, true),
                    init_program(text_8bpp_program, true),
                    ubo,
                )
            };

            let rotate_vao = {
                const VERTICES: [f32; 2 * 2 * 4] = [-1f32, -1f32, 0f32, 1f32,
                                                     1f32, -1f32, 0f32, 0f32,
                                                     1f32,  1f32, 1f32, 0f32,
                                                    -1f32,  1f32, 1f32, 1f32];

                let mut vao = 0;
                let mut vbo = 0;
                gl::GenVertexArrays(1, &mut vao);
                gl::GenBuffers(1, &mut vbo);

                gl::BindVertexArray(vao);

                gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
                gl::BufferData(gl::ARRAY_BUFFER, (size_of::<f32>() * VERTICES.len()) as _, VERTICES.as_ptr() as _, gl::STATIC_DRAW);

                gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, (4 * size_of::<f32>()) as _, ptr::null());
                gl::EnableVertexAttribArray(0);

                gl::VertexAttribPointer(1, 2, gl::FLOAT, gl::FALSE, (4 * size_of::<f32>()) as _, (2 * size_of::<f32>()) as _);
                gl::EnableVertexAttribArray(1);

                gl::BindBuffer(gl::ARRAY_BUFFER, 0);
                gl::BindVertexArray(0);

                vao
            };

            Gpu2DProgram {
                obj_program,
                obj_vao,
                obj_oam_indices: Vec::new(),
                obj_ubo_data: ObjUbo::default(),
                obj_disp_cnt_loc,
                obj_ubo,
                bg_affine_extended_program,
                bg_bitmap_program,
                bg_display_3d_program,
                bg_text_4bpp_program,
                bg_text_8bpp_program,
                bg_ubo,
                rotate_vao
            }
        }
    }

    unsafe fn draw_windows(&self, common: &Gpu2DCommon, regs: &Gpu2DRenderRegs, from_line: u8, to_line: u8) {
        let disp_cnt = DispCnt::from(regs.disp_cnts[from_line as usize]);
        if disp_cnt.obj_window_display_flag() {
            // todo!()
        }
        if !disp_cnt.is_any_window_enabled() {
            return;
        }

        #[rustfmt::skip]
        let vertices = [
            -1f32, from_line as f32,
            1f32, from_line as f32,
            1f32, to_line as f32,
            -1f32, to_line as f32,
        ];

        gl::Uniform1i(common.win_bg_disp_cnt_loc, u32::from(disp_cnt) as _);

        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, vertices.as_ptr() as _);
        gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
    }

    unsafe fn draw_objects(&mut self, regs: &Gpu2DRenderRegs, mem: &Gpu2DMem, from_line: u8, to_line: u8) {
        let disp_cnt = DispCnt::from(regs.disp_cnts[from_line as usize]);
        if !disp_cnt.screen_display_obj() {
            return;
        }

        if disp_cnt.obj_window_display_flag() {
            self.assemble_oam::<true>(mem, from_line, to_line, disp_cnt);
        } else {
            self.assemble_oam::<false>(mem, from_line, to_line, disp_cnt);
        }

        if self.obj_oam_indices.is_empty() {
            return;
        }

        gl::Uniform1i(self.obj_disp_cnt_loc, u32::from(disp_cnt) as _);

        gl::BindBuffer(gl::UNIFORM_BUFFER, self.obj_ubo);
        gl::BufferData(gl::UNIFORM_BUFFER, size_of::<ObjUbo>() as _, ptr::addr_of!(self.obj_ubo_data) as _, gl::DYNAMIC_DRAW);
        gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, self.obj_ubo);

        gl::DrawElements(gl::TRIANGLES, (6 * self.obj_oam_indices.len()) as _, gl::UNSIGNED_SHORT, self.obj_oam_indices.as_ptr() as _);
    }

    unsafe fn draw_bg(&self, common: &Gpu2DCommon, regs: &Gpu2DRenderRegs, texs: &Gpu2DTextures, fb_tex_3d: GLuint, from_line: u8, to_line: u8) {
        let disp_cnt = regs.disp_cnts[from_line as usize];

        gl::BindBuffer(gl::UNIFORM_BUFFER, self.bg_ubo);
        gl::BufferData(gl::UNIFORM_BUFFER, size_of::<BgUbo>() as _, ptr::addr_of!(regs.bg_ubo) as _, gl::DYNAMIC_DRAW);

        let draw_call = |bg_num: u8, bg_mode: BgMode| {
            if bg_mode == BgMode::Affine || bg_mode == BgMode::Large {
                // todo!("{bg_mode:?}")
            }

            let bg_cnt = regs.bg_cnts[from_line as usize * 4 + bg_num as usize];
            let bg_cnt = BgCnt::from(bg_cnt);
            let program = match bg_mode {
                BgMode::Text => {
                    if bg_cnt.color_256_palettes() {
                        &self.bg_text_8bpp_program
                    } else {
                        &self.bg_text_4bpp_program
                    }
                }
                BgMode::Affine => {
                    // TODO
                    &self.bg_affine_extended_program
                }
                BgMode::Extended => {
                    if bg_cnt.color_256_palettes() {
                        &self.bg_bitmap_program
                    } else {
                        &self.bg_affine_extended_program
                    }
                }
                BgMode::Large => {
                    // TODO
                    &self.bg_affine_extended_program
                }
                BgMode::Display3d => &self.bg_display_3d_program,
            };

            gl::UseProgram(program.program);

            gl::BindFramebuffer(gl::FRAMEBUFFER, common.bg_fbos[bg_num as usize].fbo);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, texs.bg);

            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, texs.pal);

            gl::ActiveTexture(gl::TEXTURE2);
            gl::BindTexture(gl::TEXTURE_2D, texs.bg_ext_pal);

            gl::ActiveTexture(gl::TEXTURE3);
            gl::BindTexture(gl::TEXTURE_2D, common.win_bg_fbo.color);

            gl::ActiveTexture(gl::TEXTURE4);
            gl::BindTexture(gl::TEXTURE_2D, fb_tex_3d);

            if program.has_ubo {
                gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, self.bg_ubo);
            }

            gl::Uniform1i(program.disp_cnt_loc, disp_cnt as _);
            gl::Uniform1i(program.cnt_loc, u16::from(bg_cnt) as _);

            #[rustfmt::skip]
            let vertices = [
                -1f32, from_line as f32, bg_num as f32,
                1f32, from_line as f32, bg_num as f32,
                1f32, to_line as f32, bg_num as f32,
                -1f32, to_line as f32, bg_num as f32,
            ];

            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE, 0, vertices.as_ptr() as _);
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
        };

        let disp_cnt = DispCnt::from(disp_cnt);
        macro_rules! draw {
            ($bg3mode:expr, $bg2mode:expr, $bg1mode:expr, $bg0mode:expr) => {{
                if disp_cnt.screen_display_bg3() {
                    draw_call(3, $bg3mode);
                }
                if disp_cnt.screen_display_bg2() {
                    draw_call(2, $bg2mode);
                }
                if disp_cnt.screen_display_bg1() {
                    draw_call(1, $bg1mode);
                }
                if disp_cnt.screen_display_bg0() {
                    draw_call(0, if disp_cnt.bg0_3d() { BgMode::Display3d } else { $bg0mode });
                }
            }};
        }

        match u8::from(disp_cnt.bg_mode()) {
            0 => draw!(BgMode::Text, BgMode::Text, BgMode::Text, BgMode::Text),
            1 => draw!(BgMode::Affine, BgMode::Text, BgMode::Text, BgMode::Text),
            2 => draw!(BgMode::Affine, BgMode::Affine, BgMode::Text, BgMode::Text),
            3 => draw!(BgMode::Extended, BgMode::Text, BgMode::Text, BgMode::Text),
            4 => draw!(BgMode::Extended, BgMode::Affine, BgMode::Text, BgMode::Text),
            5 => draw!(BgMode::Extended, BgMode::Extended, BgMode::Text, BgMode::Text),
            6 => {
                if disp_cnt.screen_display_bg2() {
                    draw_call(2, BgMode::Large);
                }
            }
            7 => {}
            _ => unreachable_unchecked(),
        }

        gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
    }

    unsafe fn blend_fbos(&self, common: &Gpu2DCommon, regs: &Gpu2DRenderRegs, mem: &Gpu2DMem) {
        gl::BindFramebuffer(gl::FRAMEBUFFER, common.blend_fbo.fbo);
        gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);

        let pal_slice = slice::from_raw_parts(mem.pal_ptr, regions::STANDARD_PALETTES_SIZE as usize / 2);
        let backdrop = utils::read_from_mem::<u16>(pal_slice, 0);
        let (r, g, b) = rgb5_to_float8(backdrop);
        gl::ClearColor(r, g, b, 1f32);
        gl::Clear(gl::COLOR_BUFFER_BIT);

        gl::UseProgram(common.blend_program);

        for i in 0..4 {
            gl::ActiveTexture(gl::TEXTURE0 + i);
            gl::BindTexture(gl::TEXTURE_2D, common.bg_fbos[i as usize].color);
        }

        gl::ActiveTexture(gl::TEXTURE4);
        gl::BindTexture(gl::TEXTURE_2D, common.obj_fbo.color);

        gl::ActiveTexture(gl::TEXTURE5);
        gl::BindTexture(gl::TEXTURE_2D, common.obj_fbo.depth.unwrap());

        gl::ActiveTexture(gl::TEXTURE6);
        gl::BindTexture(gl::TEXTURE_2D, common.win_bg_fbo.color);

        gl::BindBuffer(gl::UNIFORM_BUFFER, common.blend_ubo);
        gl::BufferData(gl::UNIFORM_BUFFER, size_of::<BlendUbo>() as _, ptr::addr_of!(regs.blend_ubo) as _, gl::DYNAMIC_DRAW);
        gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, common.blend_ubo);

        const VERTICES: [f32; 2 * 4] = [-1f32, 1f32, 1f32, 1f32, 1f32, -1f32, -1f32, -1f32];

        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, VERTICES.as_ptr() as _);
        gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);

        gl::BindTexture(gl::TEXTURE_2D, 0);
        gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
    }

    unsafe fn rotate(&self, common: &Gpu2DCommon) {

        let depth_test_enabled = gl::IsEnabled(gl::DEPTH_TEST);

        // --- Configure Render Target ---
        gl::BindFramebuffer(gl::FRAMEBUFFER, common.rotate_fbo.fbo);
        gl::Viewport(0, 0, DISPLAY_HEIGHT as _, DISPLAY_WIDTH as _);
        gl::UseProgram(common.rotate_program);
        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindTexture(gl::TEXTURE_2D, common.blend_fbo.color);

        if depth_test_enabled > 0 {
            gl::Disable(gl::DEPTH_TEST);
        }

        gl::BindVertexArray(self.rotate_vao);
        gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);      
        if depth_test_enabled > 0 {
            gl::Enable(gl::DEPTH_TEST);
        }

        gl::BindVertexArray(0);
        gl::UseProgram(0);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
    }

    unsafe fn draw(&mut self, common: &Gpu2DCommon, regs: &Gpu2DRenderRegs, texs: &Gpu2DTextures, mem: Gpu2DMem, fb_tex_3d: GLuint, lcdc_pal: GLuint, vram_display_program: &Gpu2DVramDisplayProgram, rotate_screens: bool) {
        macro_rules! draw_scanlines {
            ($draw_fn:expr, $draw_vram_display:expr) => {{
                let mut line = 0;
                while line < DISPLAY_HEIGHT {
                    let batch_count = regs.batch_counts[line];
                    let from_line = line as u8;
                    let to_line = line as u8 + batch_count as u8 + 1;
                    line = to_line as usize;
                    if lcdc_pal != 0 {
                        let disp_cnt = DispCnt::from(regs.disp_cnts[from_line as usize]);
                        if u8::from(disp_cnt.display_mode()) == 2 {
                            if $draw_vram_display {
                                $draw_fn(from_line, to_line);
                            }
                            continue;
                        }
                    }
                    $draw_fn(from_line, to_line);
                }
            }};
        }

        {
            gl::BindFramebuffer(gl::FRAMEBUFFER, common.win_bg_fbo.fbo);
            gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
            gl::ClearColor(1f32, 0f32, 0f32, 1f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            gl::UseProgram(common.win_bg_program);

            gl::BindBuffer(gl::UNIFORM_BUFFER, common.win_bg_ubo);
            gl::BufferData(gl::UNIFORM_BUFFER, size_of::<WinBgUbo>() as _, ptr::addr_of!(regs.win_bg_ubo) as _, gl::DYNAMIC_DRAW);
            gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, common.win_bg_ubo);

            let draw_windows = |from_line, to_line| self.draw_windows(common, regs, from_line, to_line);
            draw_scanlines!(draw_windows, false);

            gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
            gl::UseProgram(0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }

        gl::BindTexture(gl::TEXTURE_2D, texs.oam);
        sub_mem_texture1d(regions::OAM_SIZE / 2, mem.oam_ptr);

        gl::BindTexture(gl::TEXTURE_2D, texs.obj);
        sub_mem_texture2d(texs.obj_width, texs.obj_height, mem.obj_ptr);

        gl::BindTexture(gl::TEXTURE_2D, texs.pal);
        sub_pal_texture1d(regions::STANDARD_PALETTES_SIZE / 2, mem.pal_ptr);

        gl::BindTexture(gl::TEXTURE_2D, texs.obj_ext_pal);
        sub_pal_texture2d(1024, 8, mem.obj_ext_pal_ptr);

        gl::BindTexture(gl::TEXTURE_2D, 0);

        {
            gl::UseProgram(self.obj_program);

            gl::BindVertexArray(self.obj_vao);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, texs.oam);

            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, texs.obj);

            gl::ActiveTexture(gl::TEXTURE2);
            gl::BindTexture(gl::TEXTURE_2D, texs.pal);

            gl::ActiveTexture(gl::TEXTURE3);
            gl::BindTexture(gl::TEXTURE_2D, texs.obj_ext_pal);

            gl::ActiveTexture(gl::TEXTURE4);
            gl::BindTexture(gl::TEXTURE_2D, common.win_bg_fbo.color);

            gl::BindFramebuffer(gl::FRAMEBUFFER, common.obj_fbo.fbo);
            gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
            gl::ClearColor(0f32, 0f32, 0f32, 1f32);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

            gl::Enable(gl::DEPTH_TEST);
            gl::DepthFunc(gl::LESS);

            let mut draw_objects = |from_line, to_line| self.draw_objects(regs, &mem, from_line, to_line);
            draw_scanlines!(draw_objects, false);

            gl::Disable(gl::DEPTH_TEST);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::BindVertexArray(0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }

        gl::BindTexture(gl::TEXTURE_2D, texs.bg);
        sub_mem_texture2d(texs.bg_width, texs.bg_height, mem.bg_ptr);

        gl::BindTexture(gl::TEXTURE_2D, texs.bg_ext_pal);
        sub_pal_texture2d(1024, 32, mem.bg_ext_pal_ptr);

        {
            for i in 0..4 {
                gl::BindFramebuffer(gl::FRAMEBUFFER, common.bg_fbos[i].fbo);
                gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
                gl::ClearColor(0f32, 0f32, 0f32, 1f32);
                gl::Clear(gl::COLOR_BUFFER_BIT);
            }

            let draw_bg = |from_line, to_line| self.draw_bg(common, regs, texs, fb_tex_3d, from_line, to_line);
            draw_scanlines!(draw_bg, false);

            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        if lcdc_pal != 0 {
            gl::BindTexture(gl::TEXTURE_2D, lcdc_pal);
            sub_pal_texture2d(1024, 656, mem.lcdc_ptr);

            gl::UseProgram(vram_display_program.program);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, lcdc_pal);

            // Use any of the bg fbos to draw the vram into
            // At this point all other fbo won't contain any pixels for blending
            gl::BindFramebuffer(gl::FRAMEBUFFER, common.bg_fbos[0].fbo);
            gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);

            let draw_vram_display = |from_line, to_line| vram_display_program.draw(regs, from_line, to_line);
            draw_scanlines!(draw_vram_display, true);

            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::UseProgram(0);
        }

        self.blend_fbos(common, regs, &mem);

        if rotate_screens == true {
            self.rotate(common);
        }

        gl::UseProgram(0);
    }

    fn assemble_oam<const OBJ_WINDOW: bool>(&mut self, mem: &Gpu2DMem, from_line: u8, to_line: u8, disp_cnt: DispCnt) {
        const OAM_COUNT: usize = regions::OAM_SIZE as usize / 2 / size_of::<OamAttribs>();
        let oams = unsafe { slice::from_raw_parts(mem.oam_ptr as *const OamAttribs, OAM_COUNT) };

        self.obj_oam_indices.clear();
        for (i, oam) in oams.iter().enumerate() {
            let attrib0 = OamAttrib0::from(oam.attr0);
            let obj_mode = attrib0.get_obj_mode();
            if obj_mode == OamObjMode::Disabled {
                continue;
            }
            let gfx_mode = attrib0.get_gfx_mode();
            if OBJ_WINDOW && gfx_mode == OamGfxMode::Window {
                // todo!()
            }

            let attrib1 = OamAttrib1::from(oam.attr1);
            let mut x = u16::from(attrib1.x()) as i32;
            if x >= DISPLAY_WIDTH as i32 {
                x -= 512;
            }
            let mut y = attrib0.y() as i32;
            if y >= DISPLAY_HEIGHT as i32 {
                y -= 256;
            }

            let (width, height) = match (u8::from(attrib0.shape()) << 2) | u8::from(attrib1.size()) {
                0x0 => (8, 8),
                0x1 => (16, 16),
                0x2 => (32, 32),
                0x3 => (64, 64),
                0x4 => (16, 8),
                0x5 => (32, 8),
                0x6 => (32, 16),
                0x7 => (64, 32),
                0x8 => (8, 16),
                0x9 => (8, 32),
                0xA => (16, 32),
                0xB => (32, 64),
                _ => continue,
            };

            if obj_mode == OamObjMode::AffineDouble {
                if x + width * 2 < 0 || y + height * 2 < from_line as i32 || x >= DISPLAY_WIDTH as i32 || y >= to_line as i32 {
                    continue;
                }
            } else if x + width < 0 || y + height < from_line as i32 || x >= DISPLAY_WIDTH as i32 || y >= to_line as i32 {
                continue;
            }

            if gfx_mode == OamGfxMode::Bitmap {
                if disp_cnt.bitmap_obj_mapping() {
                    self.obj_ubo_data.map_widths[i] = width as u32;
                    self.obj_ubo_data.obj_bounds[i] = u16::from(OamAttrib2::from(oam.attr2).tile_index()) as u32 * if disp_cnt.bitmap_obj_1d_boundary() { 256 } else { 128 };
                } else {
                    self.obj_ubo_data.map_widths[i] = if disp_cnt.bitmap_obj_2d() { 256 } else { 128 };
                    let x_mask = if disp_cnt.bitmap_obj_2d() { 0x1F } else { 0x0F };
                    self.obj_ubo_data.obj_bounds[i] = (oam.attr2 & x_mask) as u32 * 0x10 + (oam.attr2 & 0x3FF & !x_mask) as u32 * 0x80;
                }
            } else if disp_cnt.tile_1d_obj_mapping() {
                self.obj_ubo_data.map_widths[i] = width as u32;
                self.obj_ubo_data.obj_bounds[i] = 32 << u8::from(disp_cnt.tile_obj_1d_boundary());
            } else {
                self.obj_ubo_data.map_widths[i] = if attrib0.is_8bit() { 128 } else { 256 };
                self.obj_ubo_data.obj_bounds[i] = 32;
            }

            let index_base = (i * 4) as u16;
            self.obj_oam_indices.push([index_base, index_base + 1, index_base + 2, index_base, index_base + 2, index_base + 3]);
        }
    }
}

pub struct Gpu2DRenderer {
    regs_a: [Box<Gpu2DRenderRegs>; 2],
    regs_b: [Box<Gpu2DRenderRegs>; 2],
    pub has_vram_display: [bool; 2],
    lcdc_pal: GLuint,
    vram_display_program: Gpu2DVramDisplayProgram,
    tex_a: Gpu2DTextures,
    tex_b: Gpu2DTextures,
    pub common: Gpu2DCommon,
    program_a: Gpu2DProgram,
    program_b: Gpu2DProgram,
}

impl Gpu2DRenderer {
    pub fn new() -> Self {
        unsafe {
            let obj_vert_shader = create_shader("obj", shader_source!("obj_vert"), gl::VERTEX_SHADER).unwrap();
            let bg_vert_shader = create_shader("bg", shader_source!("bg_vert"), gl::VERTEX_SHADER).unwrap();
            let bg_vert_affine_extended_shader = create_shader("bg affine extended", shader_source!("bg_vert_affine_extended"), gl::VERTEX_SHADER).unwrap();
            let bg_vert_bitmap_shader = create_shader("bg bitmap", shader_source!("bg_vert_bitmap"), gl::VERTEX_SHADER).unwrap();

            let instance = Gpu2DRenderer {
                regs_a: [Box::new(Gpu2DRenderRegs::default()), Box::new(Gpu2DRenderRegs::default())],
                regs_b: [Box::new(Gpu2DRenderRegs::default()), Box::new(Gpu2DRenderRegs::default())],
                has_vram_display: [false; 2],
                lcdc_pal: create_pal_texture2d(1024, 656),
                vram_display_program: Gpu2DVramDisplayProgram::new(),
                tex_a: Gpu2DTextures::new(1024, OBJ_A_TEX_HEIGHT, 1024, BG_A_TEX_HEIGHT),
                tex_b: Gpu2DTextures::new(1024, OBJ_B_TEX_HEIGHT, 1024, BG_B_TEX_HEIGHT),
                common: Gpu2DCommon::new(),
                program_a: Gpu2DProgram::new::<{ A }>(obj_vert_shader, bg_vert_shader, bg_vert_affine_extended_shader, bg_vert_bitmap_shader),
                program_b: Gpu2DProgram::new::<{ B }>(obj_vert_shader, bg_vert_shader, bg_vert_affine_extended_shader, bg_vert_bitmap_shader),
            };

            gl::DeleteShader(obj_vert_shader);
            gl::DeleteShader(bg_vert_shader);
            gl::DeleteShader(bg_vert_affine_extended_shader);
            gl::DeleteShader(bg_vert_bitmap_shader);

            instance
        }
    }

    pub fn on_scanline(&mut self, inner_a: &mut Gpu2DRegisters, inner_b: &mut Gpu2DRegisters, line: u8) {
        self.regs_a[1].on_scanline(inner_a, line);
        self.regs_b[1].on_scanline(inner_b, line);
        if u8::from(DispCnt::from(self.regs_a[1].disp_cnts[line as usize]).display_mode()) == 2 {
            self.has_vram_display[1] = true;
        }
    }

    pub fn on_scanline_finish(&mut self) {
        self.regs_a.swap(0, 1);
        self.regs_b.swap(0, 1);
        self.has_vram_display[0] = self.has_vram_display[1];
    }

    pub fn reload_registers(&mut self) {
        self.regs_a[1].reset();
        self.regs_b[1].reset();
        self.has_vram_display[1] = false;
    }

    pub unsafe fn render<const ENGINE: Gpu2DEngine>(&mut self, common: &GpuRendererCommon, fb_tex_3d: GLuint, rotate_screens: bool) {
        match ENGINE {
            A => {
                self.program_a.draw(
                    &self.common,
                    &self.regs_a[0],
                    &self.tex_a,
                    Gpu2DMem::new::<{ A }>(&common.mem_buf),
                    fb_tex_3d,
                    if self.has_vram_display[0] { self.lcdc_pal } else { 0 },
                    &self.vram_display_program,
                    rotate_screens,
                );
            }
            B => self.program_b.draw(&self.common,
                    &self.regs_b[0],
                    &self.tex_b, 
                    Gpu2DMem::new::<{ B }>(&common.mem_buf), 
                    0, 
                    0, 
                    &self.vram_display_program,
                    rotate_screens,
                ),
        }
    }
}
