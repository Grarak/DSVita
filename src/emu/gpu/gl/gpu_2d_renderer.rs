use crate::emu::gpu::gl::gl_glyph::GlGlyph;
use crate::emu::gpu::gl::gl_utils::{
    create_fb_color, create_fb_depth_tex, create_mem_texture1d, create_mem_texture2d, create_pal_texture1d, create_pal_texture2d, create_program, create_shader, shader_source, sub_mem_texture1d,
    sub_mem_texture2d, sub_pal_texture1d, sub_pal_texture2d,
};
use crate::emu::gpu::gpu::{PowCnt1, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::emu::gpu::gpu_2d::Gpu2DEngine::{A, B};
use crate::emu::gpu::gpu_2d::{DispCnt, Gpu2DEngine, Gpu2DInner};
use crate::emu::gpu::gpu_2d_mem_buf::Gpu2dMemBuf;
use crate::emu::memory::mem::Memory;
use crate::emu::memory::oam::{OamAttrib0, OamAttrib1, OamAttrib2, OamAttribs, OamGfxMode, OamObjMode};
use crate::emu::memory::regions;
use crate::presenter::{Presenter, PresenterScreen, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH, PRESENTER_SUB_BOTTOM_SCREEN, PRESENTER_SUB_TOP_SCREEN};
use crate::utils;
use crate::utils::StrErr;
use gl::types::{GLint, GLuint};
use static_assertions::const_assert;
use std::hint::unreachable_unchecked;
use std::intrinsics::unlikely;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;
use std::{mem, ptr, slice};

struct Gpu2dMem {
    bg_ptr: *const u8,
    obj_ptr: *const u8,
    pal_ptr: *const u8,
    oam_ptr: *const u8,
    bg_ext_pal_ptr: *const u8,
    obj_ext_pal_ptr: *const u8,
}

impl Gpu2dMem {
    fn new<const ENGINE: Gpu2DEngine>(buf: &Gpu2dMemBuf) -> Self {
        match ENGINE {
            A => Gpu2dMem {
                bg_ptr: buf.bg_a.as_ptr(),
                obj_ptr: buf.obj_a.as_ptr(),
                pal_ptr: buf.pal_a.as_ptr(),
                oam_ptr: buf.oam_a.as_ptr(),
                bg_ext_pal_ptr: buf.bg_a_ext_palette.as_ptr(),
                obj_ext_pal_ptr: buf.obj_a_ext_palette.as_ptr(),
            },
            B => Gpu2dMem {
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

const_assert!(mem::size_of::<WinBgUbo>() < 16 * 1024);

#[derive(Clone)]
#[repr(C)]
struct BgUbo {
    ofs: [u32; DISPLAY_HEIGHT * 4],
    x: [f32; DISPLAY_HEIGHT * 2],
    y: [f32; DISPLAY_HEIGHT * 2],
    pa: [f32; DISPLAY_HEIGHT * 2],
    pb: [f32; DISPLAY_HEIGHT * 2],
    pc: [f32; DISPLAY_HEIGHT * 2],
    pd: [f32; DISPLAY_HEIGHT * 2],
}

const_assert!(mem::size_of::<BgUbo>() < 16 * 1024);

#[derive(Clone)]
struct GpuRegs {
    disp_cnts: [u32; DISPLAY_HEIGHT],
    bg_cnts: [u32; DISPLAY_HEIGHT * 4],
    win_bg_ubo: WinBgUbo,
    bg_ubo: BgUbo,
    blend_ubo: BlendUbo,
    batch_counts: [u8; DISPLAY_HEIGHT],
    current_batch_count_index: usize,
}

impl Default for GpuRegs {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

impl GpuRegs {
    fn on_scanline(&mut self, inner: &mut Gpu2DInner, line: u8) {
        let updated = self.disp_cnts[self.current_batch_count_index] != u32::from(inner.disp_cnt);
        let updated = updated || {
            let mut updated = false;
            for i in 0..4 {
                if self.bg_cnts[self.current_batch_count_index * 4 + i] != u16::from(inner.bg_cnt[i]) as u32 {
                    updated = true;
                    break;
                }
            }
            updated
        };

        if updated {
            self.disp_cnts[line as usize] = u32::from(inner.disp_cnt);
            for i in 0..4 {
                self.bg_cnts[line as usize * 4 + i] = u16::from(inner.bg_cnt[i]) as u32;
            }
            self.current_batch_count_index = line as usize;
        } else {
            self.batch_counts[self.current_batch_count_index] += 1;
        }

        for i in 0..2 {
            self.win_bg_ubo.win_h[i * DISPLAY_HEIGHT + line as usize] = inner.win_h[i] as u32;
            self.win_bg_ubo.win_v[i * DISPLAY_HEIGHT + line as usize] = inner.win_v[i] as u32;
        }
        self.win_bg_ubo.win_in[line as usize] = inner.win_in as u32;
        self.win_bg_ubo.win_out[line as usize] = inner.win_out as u32;

        for i in 0..4 {
            self.bg_ubo.ofs[i * DISPLAY_HEIGHT + line as usize] = (inner.bg_h_ofs[i] as u32) | ((inner.bg_v_ofs[i] as u32) << 16);
        }
        for i in 0..2 {
            self.bg_ubo.x[i * DISPLAY_HEIGHT + line as usize] = inner.bg_x[i] as f32 / 256.0;
            self.bg_ubo.y[i * DISPLAY_HEIGHT + line as usize] = inner.bg_y[i] as f32 / 256.0;
            self.bg_ubo.pa[i * DISPLAY_HEIGHT + line as usize] = inner.bg_pa[i] as f32 / 256.0;
            self.bg_ubo.pc[i * DISPLAY_HEIGHT + line as usize] = inner.bg_pc[i] as f32 / 256.0;

            if unlikely(inner.bg_x_dirty || line == 0) {
                self.bg_ubo.pb[i * DISPLAY_HEIGHT + line as usize] = 0f32;
            } else {
                self.bg_ubo.pb[i * DISPLAY_HEIGHT + line as usize] = inner.bg_pb[i] as f32 / 256.0 + self.bg_ubo.pb[i * DISPLAY_HEIGHT + line as usize - 1];
            }

            if unlikely(inner.bg_y_dirty || line == 0) {
                self.bg_ubo.pd[i * DISPLAY_HEIGHT + line as usize] = 0f32;
            } else {
                self.bg_ubo.pd[i * DISPLAY_HEIGHT + line as usize] = inner.bg_pd[i] as f32 / 256.0 + self.bg_ubo.pd[i * DISPLAY_HEIGHT + line as usize - 1];
            }
        }
        inner.bg_x_dirty = false;
        inner.bg_y_dirty = false;

        self.blend_ubo.bld_cnts[line as usize] = inner.bld_cnt as u32;
        self.blend_ubo.bld_alphas[line as usize] = inner.bld_alpha as u32;
        self.blend_ubo.bld_ys[line as usize] = inner.bld_y as u32;
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

const_assert!(mem::size_of::<ObjUbo>() < 16 * 1024);

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

const_assert!(mem::size_of::<BlendUbo>() < 16 * 1024);

struct Gpu2dFbo {
    color: GLuint,
    depth: Option<GLuint>,
    fbo: GLuint,
}

impl Gpu2dFbo {
    fn new(width: u32, height: u32, depth: bool) -> Result<Self, StrErr> {
        unsafe {
            let color = create_fb_color(width, height);

            let mut fbo = 0;
            gl::GenFramebuffers(1, &mut fbo);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
            gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, color, 0);

            let depth = if depth { Some(create_fb_depth_tex(fbo, width, height)) } else { None };

            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            gl::BindRenderbuffer(gl::RENDERBUFFER, 0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            if status != gl::FRAMEBUFFER_COMPLETE {
                Err(StrErr::new(format!("Failed to create fbo: {status}")))
            } else {
                Ok(Gpu2dFbo { color, depth, fbo })
            }
        }
    }
}

struct Gpu2dTextures {
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

impl Gpu2dTextures {
    fn new(obj_width: u32, obj_height: u32, bg_width: u32, bg_height: u32) -> Self {
        unsafe {
            Gpu2dTextures {
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

struct Gpu2dCommon {
    win_bg_program: GLuint,
    win_bg_disp_cnt_loc: GLint,
    win_bg_ubo: GLuint,
    win_bg_fbo: Gpu2dFbo,
    obj_fbo: Gpu2dFbo,
    bg_fbos: [Gpu2dFbo; 4],
    blend_program: GLuint,
    blend_ubo: GLuint,
    blend_fbo: Gpu2dFbo,
}

impl Gpu2dCommon {
    fn new() -> Self {
        unsafe {
            let (win_bg_program, win_bg_disp_cnt_loc, win_bg_ubo, win_bg_fbo) = {
                println!("Compile win vert");
                let vert_shader = create_shader(shader_source!("win_bg_vert"), gl::VERTEX_SHADER).unwrap();
                println!("Compile win frag");
                let frag_shader = create_shader(shader_source!("win_bg_frag"), gl::FRAGMENT_SHADER).unwrap();
                let program = create_program(&[vert_shader, frag_shader]).unwrap();
                gl::DeleteShader(vert_shader);
                gl::DeleteShader(frag_shader);

                gl::UseProgram(program);

                gl::BindAttribLocation(program, 0, "position\0".as_ptr() as _);

                let disp_cnt_loc = gl::GetUniformLocation(program, "dispCnt\0".as_ptr() as _);

                let mut ubo = 0;
                gl::GenBuffers(1, &mut ubo);
                gl::BindBuffer(gl::UNIFORM_BUFFER, ubo);

                gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, "WinBgUbo\0".as_ptr() as _), 0);

                gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
                gl::UseProgram(0);

                let fbo = Gpu2dFbo::new(DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32, false).unwrap();

                (program, disp_cnt_loc, ubo, fbo)
            };

            let (blend_program, blend_ubo, blend_fbo) = {
                println!("Compile blend vert");
                let vert_shader = create_shader(shader_source!("blend_vert"), gl::VERTEX_SHADER).unwrap();
                println!("Compile blend frag");
                let frag_shader = create_shader(shader_source!("blend_frag"), gl::FRAGMENT_SHADER).unwrap();
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

                gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, "BlendUbo\0".as_ptr() as _), 0);

                gl::UseProgram(0);

                let fbo = Gpu2dFbo::new(DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32, false).unwrap();

                (program, ubo, fbo)
            };

            Gpu2dCommon {
                win_bg_program,
                win_bg_disp_cnt_loc,
                win_bg_ubo,
                win_bg_fbo,
                obj_fbo: Gpu2dFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, true).unwrap(),
                bg_fbos: [
                    Gpu2dFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false).unwrap(),
                    Gpu2dFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false).unwrap(),
                    Gpu2dFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false).unwrap(),
                    Gpu2dFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false).unwrap(),
                ],
                blend_program,
                blend_ubo,
                blend_fbo,
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
}

struct Gpu2dProgram {
    obj_program: GLuint,
    obj_vao: GLuint,
    obj_oam_indices: Vec<[u16; 6]>,
    obj_ubo_data: ObjUbo,
    obj_disp_cnt_loc: GLint,
    obj_ubo: GLuint,
    bg_program: GLuint,
    bg_disp_cnt_loc: GLint,
    bg_cnt_loc: GLint,
    bg_mode_loc: GLint,
    bg_ubo: GLuint,
}

impl Gpu2dProgram {
    fn new<const ENGINE: Gpu2DEngine>(obj_vert_shader: GLuint, bg_vert_shader: GLuint) -> Self {
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

                println!("Compile obj frag");
                let frag_shader = create_shader(&frag_shader_src, gl::FRAGMENT_SHADER).unwrap();
                let program = create_program(&[obj_vert_shader, frag_shader]).unwrap();
                gl::DeleteShader(frag_shader);

                gl::UseProgram(program);

                let mut vertices_buf = 0;
                gl::GenBuffers(1, &mut vertices_buf);
                gl::BindBuffer(gl::ARRAY_BUFFER, vertices_buf);
                gl::BufferData(gl::ARRAY_BUFFER, (mem::size_of::<f32>() * OBJ_VERTICES.len()) as _, OBJ_VERTICES.as_ptr() as _, gl::STATIC_DRAW);

                let mut indices_buf = 0;
                gl::GenBuffers(1, &mut indices_buf);
                gl::BindBuffer(gl::ARRAY_BUFFER, indices_buf);
                gl::BufferData(gl::ARRAY_BUFFER, OBJ_OAM_INDICES.len() as _, OBJ_OAM_INDICES.as_ptr() as _, gl::STATIC_DRAW);

                let mut vao = 0;
                gl::GenVertexArrays(1, &mut vao);
                gl::BindVertexArray(vao);

                gl::BindBuffer(gl::ARRAY_BUFFER, vertices_buf);
                gl::EnableVertexAttribArray(0);
                gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, 0 as _);

                gl::BindBuffer(gl::ARRAY_BUFFER, indices_buf);
                gl::EnableVertexAttribArray(1);
                gl::VertexAttribPointer(1, 1, gl::UNSIGNED_BYTE, gl::FALSE, 0, 0 as _);

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

                gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, "ObjUbo\0".as_ptr() as _), 0);

                gl::UseProgram(0);

                (program, vao, disp_cnt_loc, ubo)
            };

            let (bg_program, bg_disp_cnt_loc, bg_cnt_loc, bg_mode_loc, bg_ubo) = {
                let frag_shader_src = shader_source!("bg_frag").replace(
                    "BG_TEX_HEIGHT",
                    &format!(
                        "{}.0",
                        match ENGINE {
                            A => BG_A_TEX_HEIGHT / 2,
                            B => BG_B_TEX_HEIGHT / 2,
                        }
                    ),
                );

                println!("Compile bg frag");
                let frag_shader = create_shader(&frag_shader_src, gl::FRAGMENT_SHADER).unwrap();
                let program = create_program(&[bg_vert_shader, frag_shader]).unwrap();
                gl::DeleteShader(frag_shader);

                gl::UseProgram(program);

                gl::BindAttribLocation(program, 0, "position\0".as_ptr() as _);

                let disp_cnt_loc = gl::GetUniformLocation(program, "dispCnt\0".as_ptr() as _);
                let bg_cnt_loc = gl::GetUniformLocation(program, "bgCnt\0".as_ptr() as _);
                let bg_mode_loc = gl::GetUniformLocation(program, "bgMode\0".as_ptr() as _);

                gl::Uniform1i(gl::GetUniformLocation(program, "bgTex\0".as_ptr() as _), 0);
                gl::Uniform1i(gl::GetUniformLocation(program, "palTex\0".as_ptr() as _), 1);
                gl::Uniform1i(gl::GetUniformLocation(program, "extPalTex\0".as_ptr() as _), 2);
                gl::Uniform1i(gl::GetUniformLocation(program, "winTex\0".as_ptr() as _), 3);

                let mut ubo = 0;
                gl::GenBuffers(1, &mut ubo);
                gl::BindBuffer(gl::UNIFORM_BUFFER, ubo);

                gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, "BgUbo\0".as_ptr() as _), 0);

                gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
                gl::UseProgram(0);

                (program, disp_cnt_loc, bg_cnt_loc, bg_mode_loc, ubo)
            };

            Gpu2dProgram {
                obj_program,
                obj_vao,
                obj_oam_indices: Vec::new(),
                obj_ubo_data: ObjUbo::default(),
                obj_disp_cnt_loc,
                obj_ubo,
                bg_program,
                bg_disp_cnt_loc,
                bg_cnt_loc,
                bg_mode_loc,
                bg_ubo,
            }
        }
    }

    unsafe fn draw_windows(&mut self, common: &Gpu2dCommon, regs: &GpuRegs, from_line: u8, to_line: u8) {
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

    unsafe fn draw_objects(&mut self, regs: &GpuRegs, mem: &Gpu2dMem, from_line: u8, to_line: u8) {
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
        gl::BufferData(gl::UNIFORM_BUFFER, mem::size_of::<ObjUbo>() as _, ptr::addr_of!(self.obj_ubo_data) as _, gl::DYNAMIC_DRAW);
        gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, self.obj_ubo);

        gl::DrawElements(gl::TRIANGLES, (6 * self.obj_oam_indices.len()) as _, gl::UNSIGNED_SHORT, self.obj_oam_indices.as_ptr() as _);
    }

    unsafe fn draw_bg(&mut self, common: &Gpu2dCommon, regs: &GpuRegs, from_line: u8, to_line: u8) {
        let disp_cnt = regs.disp_cnts[from_line as usize];

        gl::Uniform1i(self.bg_disp_cnt_loc, disp_cnt as _);

        gl::BindBuffer(gl::UNIFORM_BUFFER, self.bg_ubo);
        gl::BufferData(gl::UNIFORM_BUFFER, mem::size_of::<BgUbo>() as _, ptr::addr_of!(regs.bg_ubo) as _, gl::DYNAMIC_DRAW);
        gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, self.bg_ubo);

        let draw_call = |bg_num: u8, bg_mode: BgMode| {
            if bg_mode == BgMode::Affine || bg_mode == BgMode::Large {
                todo!("{bg_mode:?}")
            }

            gl::BindFramebuffer(gl::FRAMEBUFFER, common.bg_fbos[bg_num as usize].fbo);

            gl::Uniform1i(self.bg_cnt_loc, regs.bg_cnts[from_line as usize * 4 + bg_num as usize] as _);
            gl::Uniform1i(self.bg_mode_loc, bg_mode as _);

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
                if disp_cnt.screen_display_bg0() && (!disp_cnt.bg0_3d()) {
                    draw_call(0, $bg0mode);
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

    unsafe fn blend_fbos(&self, common: &Gpu2dCommon, regs: &GpuRegs, mem: &Gpu2dMem) {
        gl::BindFramebuffer(gl::FRAMEBUFFER, common.blend_fbo.fbo);
        gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);

        let pal_slice = slice::from_raw_parts(mem.pal_ptr, regions::STANDARD_PALETTES_SIZE as usize / 2);
        let backdrop = utils::read_from_mem::<u16>(pal_slice, 0);
        let (r, g, b, _) = Self::rgb5_to_float8(backdrop);
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
        gl::BufferData(gl::UNIFORM_BUFFER, mem::size_of::<BlendUbo>() as _, ptr::addr_of!(regs.blend_ubo) as _, gl::DYNAMIC_DRAW);
        gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, common.blend_ubo);

        const VERTICES: [f32; 2 * 4] = [-1f32, 1f32, 1f32, 1f32, 1f32, -1f32, -1f32, -1f32];

        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, VERTICES.as_ptr() as _);
        gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);

        gl::BindTexture(gl::TEXTURE_2D, 0);
        gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
    }

    unsafe fn draw(&mut self, common: &Gpu2dCommon, regs: &GpuRegs, texs: &Gpu2dTextures, mem: Gpu2dMem) {
        macro_rules! draw_scanlines {
            ($draw_fn:expr) => {{
                let mut line = 0;
                while line < DISPLAY_HEIGHT {
                    let batch_count = regs.batch_counts[line];
                    let from_line = line as u8;
                    let to_line = line as u8 + batch_count as u8 + 1;
                    $draw_fn(from_line, to_line);
                    line = to_line as usize;
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
            gl::BufferData(gl::UNIFORM_BUFFER, mem::size_of::<WinBgUbo>() as _, ptr::addr_of!(regs.win_bg_ubo) as _, gl::DYNAMIC_DRAW);
            gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, common.win_bg_ubo);

            let mut draw_windows = |from_line, to_line| self.draw_windows(common, regs, from_line, to_line);
            draw_scanlines!(draw_windows);

            // gl::ReadPixels(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, gl::RGBA, gl::UNSIGNED_BYTE, self.read_buf.as_mut_ptr() as _);

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
            draw_scanlines!(draw_objects);

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
            gl::UseProgram(self.bg_program);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, texs.bg);

            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, texs.pal);

            gl::ActiveTexture(gl::TEXTURE2);
            gl::BindTexture(gl::TEXTURE_2D, texs.bg_ext_pal);

            gl::ActiveTexture(gl::TEXTURE3);
            gl::BindTexture(gl::TEXTURE_2D, common.win_bg_fbo.color);

            for i in 0..4 {
                gl::BindFramebuffer(gl::FRAMEBUFFER, common.bg_fbos[i].fbo);
                gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
                gl::ClearColor(0f32, 0f32, 0f32, 1f32);
                gl::Clear(gl::COLOR_BUFFER_BIT);
            }

            let mut draw_bg = |from_line, to_line| self.draw_bg(common, regs, from_line, to_line);
            draw_scanlines!(draw_bg);

            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        self.blend_fbos(common, regs, &mem);

        gl::UseProgram(0);
    }

    fn assemble_oam<const OBJ_WINDOW: bool>(&mut self, mem: &Gpu2dMem, from_line: u8, to_line: u8, disp_cnt: DispCnt) {
        const OAM_COUNT: usize = regions::OAM_SIZE as usize / 2 / mem::size_of::<OamAttribs>();
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
                todo!()
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

    fn rgb5_to_float8(color: u16) -> (f32, f32, f32, f32) {
        let r = (color & 0x1F) as f32;
        let g = ((color >> 5) & 0x1F) as f32;
        let b = ((color >> 10) & 0x1F) as f32;
        let a = ((color >> 15) & 1) as f32;
        (r / 31f32, g / 31f32, b / 31f32, a)
    }
}

pub struct Gpu2dRenderer {
    regs_a: [GpuRegs; 2],
    regs_b: [GpuRegs; 2],
    mem_buf: Gpu2dMemBuf,
    drawing: Mutex<bool>,
    drawing_condvar: Condvar,
    tex_a: Gpu2dTextures,
    tex_b: Gpu2dTextures,
    common: Gpu2dCommon,
    program_a: Gpu2dProgram,
    program_b: Gpu2dProgram,
    pow_cnt1: PowCnt1,
    gl_glyph: GlGlyph,
    render_time_measure_count: u8,
    render_time_sum: u32,
    average_render_time: u16,
}

impl Gpu2dRenderer {
    pub fn new() -> Self {
        let (obj_vert_shader, bg_vert_shader) = unsafe {
            println!("Compile obj vert");
            let obj_vert_shader = create_shader(shader_source!("obj_vert"), gl::VERTEX_SHADER).unwrap();
            println!("Compile bg vert");
            let bg_vert_shader = create_shader(shader_source!("bg_vert"), gl::VERTEX_SHADER).unwrap();
            (obj_vert_shader, bg_vert_shader)
        };

        let instance = Gpu2dRenderer {
            regs_a: [GpuRegs::default(), GpuRegs::default()],
            regs_b: [GpuRegs::default(), GpuRegs::default()],
            mem_buf: Gpu2dMemBuf::default(),
            drawing: Mutex::new(false),
            drawing_condvar: Condvar::new(),
            tex_a: Gpu2dTextures::new(1024, OBJ_A_TEX_HEIGHT, 1024, BG_A_TEX_HEIGHT),
            tex_b: Gpu2dTextures::new(1024, OBJ_B_TEX_HEIGHT, 1024, BG_B_TEX_HEIGHT),
            common: Gpu2dCommon::new(),
            program_a: Gpu2dProgram::new::<{ A }>(obj_vert_shader, bg_vert_shader),
            program_b: Gpu2dProgram::new::<{ B }>(obj_vert_shader, bg_vert_shader),
            pow_cnt1: PowCnt1::from(0),
            gl_glyph: GlGlyph::new(),
            render_time_measure_count: 0,
            render_time_sum: 0,
            average_render_time: 0,
        };

        unsafe {
            gl::DeleteShader(obj_vert_shader);
            gl::DeleteShader(bg_vert_shader);
        }

        instance
    }

    pub fn on_scanline(&mut self, inner_a: &mut Gpu2DInner, inner_b: &mut Gpu2DInner, line: u8) {
        self.regs_a[1].on_scanline(inner_a, line);
        self.regs_b[1].on_scanline(inner_b, line);
    }

    pub fn reload_registers(&mut self) {
        self.regs_a[1] = GpuRegs::default();
        self.regs_b[1] = GpuRegs::default();
    }

    pub fn start_drawing(&mut self, mem: &mut Memory, pow_cnt1: PowCnt1) {
        let mut drawing = self.drawing.lock().unwrap();

        if !*drawing {
            self.mem_buf.read(mem);

            self.regs_a[0] = self.regs_a[1].clone();
            self.regs_b[0] = self.regs_b[1].clone();

            self.pow_cnt1 = pow_cnt1;

            *drawing = true;
            self.drawing_condvar.notify_one();
        }
    }

    // pub fn wait_for_swap(&self) {
    //     let drawing = self.drawing.lock().unwrap();
    //     let _drawing = self.drawing_condvar.wait_while(drawing, |drawing| *drawing).unwrap();
    // }

    pub unsafe fn draw(&mut self, presenter: &mut Presenter, fps: &Arc<AtomicU16>) {
        {
            let drawing = self.drawing.lock().unwrap();
            let _drawing = self.drawing_condvar.wait_while(drawing, |drawing| !*drawing).unwrap();
        }

        let render_time_start = Instant::now();

        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
        gl::ClearColor(0f32, 0f32, 0f32, 1f32);
        gl::Clear(gl::COLOR_BUFFER_BIT);

        if self.pow_cnt1.enable() {
            let blit_fb = |screen: &PresenterScreen| {
                gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
                gl::BindFramebuffer(gl::READ_FRAMEBUFFER, self.common.blend_fbo.fbo);
                gl::BlitFramebuffer(
                    0,
                    0,
                    DISPLAY_WIDTH as _,
                    DISPLAY_HEIGHT as _,
                    screen.x as _,
                    screen.y as _,
                    (screen.x + screen.width) as _,
                    (screen.y + screen.height) as _,
                    gl::COLOR_BUFFER_BIT,
                    gl::NEAREST,
                );
            };

            self.program_a.draw(&self.common, &self.regs_a[0], &self.tex_a, Gpu2dMem::new::<{ A }>(&self.mem_buf));
            blit_fb(if self.pow_cnt1.display_swap() { &PRESENTER_SUB_TOP_SCREEN } else { &PRESENTER_SUB_BOTTOM_SCREEN });

            self.program_b.draw(&self.common, &self.regs_b[0], &self.tex_b, Gpu2dMem::new::<{ B }>(&self.mem_buf));
            blit_fb(if self.pow_cnt1.display_swap() { &PRESENTER_SUB_BOTTOM_SCREEN } else { &PRESENTER_SUB_TOP_SCREEN });
        }

        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);

        let fps = fps.load(Ordering::Relaxed);
        let per = fps * 100 / 60;
        self.gl_glyph.draw(format!("Render time: {}ms\nFPS: {fps} ({per}%)", self.average_render_time));

        presenter.gl_swap_window();

        let render_time_end = Instant::now();
        let render_time_diff = render_time_end - render_time_start;
        self.render_time_sum += render_time_diff.as_millis() as u32;
        self.render_time_measure_count += 1;
        if self.render_time_measure_count == 30 {
            self.render_time_measure_count = 0;
            self.average_render_time = (self.render_time_sum / 30) as u16;
            self.render_time_sum = 0;
        }

        {
            let mut drawing = self.drawing.lock().unwrap();
            *drawing = false;
            // self.drawing_condvar.notify_one();
        }
    }
}
