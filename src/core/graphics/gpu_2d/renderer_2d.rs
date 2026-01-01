use crate::core::graphics::gl_utils::{
    create_mem_texture1d, create_mem_texture2d, create_pal_texture1d, create_pal_texture2d, create_program, create_shader, shader_source, sub_mem_texture1d, sub_mem_texture2d, sub_pal_texture1d,
    sub_pal_texture2d, GpuFbo,
};
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_2d::registers_2d::{BgCnt, DispCnt};
use crate::core::graphics::gpu_2d::renderer_regs_2d::{BgUbo, BlendUbo, Gpu2DMem, Gpu2DRenderRegs, Gpu2DRenderRegsShared, WinBgUbo};
use crate::core::graphics::gpu_2d::Gpu2DEngine;
use crate::core::graphics::gpu_2d::Gpu2DEngine::{A, B};
use crate::core::graphics::gpu_mem_buf::GpuMemRefs;
use crate::core::memory::oam::{OamAttrib0, OamAttrib1, OamAttrib2, OamAttribs, OamGfxMode, OamObjMode};
use crate::core::memory::{regions, vram};
use crate::utils::{self, array_init, HeapMemU8};
use crate::utils::{rgb5_to_float8, PtrWrapper};
use gl::types::{GLint, GLuint};
use static_assertions::const_assert;
use std::hint::unreachable_unchecked;
use std::{mem, ptr, slice};

const BG_A_TEX_HEIGHT: u32 = 512;
const BG_B_TEX_HEIGHT: u32 = 128;
const OBJ_A_TEX_HEIGHT: u32 = 256;
const OBJ_B_TEX_HEIGHT: u32 = 128;

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
struct ObjAttr {
    map_width: u32,
    obj_bound: u32,
}

#[repr(C)]
struct ObjUbo {
    attrs: [ObjAttr; 128],
}

const_assert!(size_of::<ObjUbo>() <= 16 * 1024);

impl Default for ObjUbo {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

struct Gpu2DTexMem {
    bg: Vec<u8>,
    obj: Vec<u8>,
    oam: HeapMemU8<{ regions::OAM_SIZE as usize / 2 }>,
    pal: HeapMemU8<{ regions::STANDARD_PALETTES_SIZE as usize / 2 }>,
    bg_ext_pal: HeapMemU8<{ vram::BG_EXT_PAL_SIZE }>,
    obj_ext_pal: HeapMemU8<{ vram::OBJ_EXT_PAL_SIZE }>,
}

impl Gpu2DTexMem {
    fn new(bg_size: u32, obj_size: u32) -> Self {
        let mut bg = Vec::new();
        bg.resize(bg_size as usize, 0);
        let mut obj = Vec::new();
        obj.resize(obj_size as usize, 0);
        Gpu2DTexMem {
            bg,
            obj,
            oam: HeapMemU8::new(),
            pal: HeapMemU8::new(),
            bg_ext_pal: HeapMemU8::new(),
            obj_ext_pal: HeapMemU8::new(),
        }
    }
}

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
    #[cfg(target_os = "linux")]
    mem: Gpu2DTexMem,
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
                #[cfg(target_os = "linux")]
                mem: Gpu2DTexMem::new(bg_width * bg_height, obj_width * obj_height),
            }
        }
    }
}

pub struct Gpu2DCommon {
    win_bg_program: GLuint,
    win_bg_disp_cnt_loc: GLint,
    win_bg_ubo: GLuint,
    win_bg_fbo: GpuFbo,
    win_obj_fbo: GpuFbo,
    obj_fbo: GpuFbo,
    bg_fbos: [GpuFbo; 4],
    blend_program: GLuint,
    blend_ubo: GLuint,
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

                gl::BindAttribLocation(program, 0, c"position".as_ptr() as _);

                gl::Uniform1i(gl::GetUniformLocation(program, c"objWinTex".as_ptr() as _), 0);

                let disp_cnt_loc = gl::GetUniformLocation(program, c"dispCnt".as_ptr() as _);

                let mut ubo = 0;
                gl::GenBuffers(1, &mut ubo);
                gl::BindBuffer(gl::UNIFORM_BUFFER, ubo);

                // Don't set ubo binding on vita, shader cache in vitaGL doesn't seem to consider block name
                // Which results in an endless loop
                if cfg!(target_os = "linux") {
                    gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, c"WinBgUbo".as_ptr() as _), 0);
                }

                gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
                gl::UseProgram(0);

                let fbo = GpuFbo::new(DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32, false).unwrap();

                (program, disp_cnt_loc, ubo, fbo)
            };

            let (blend_program, blend_ubo) = {
                let vert_shader = create_shader("blend", shader_source!("blend_vert"), gl::VERTEX_SHADER).unwrap();
                let frag_shader = create_shader("blend", shader_source!("blend_frag"), gl::FRAGMENT_SHADER).unwrap();
                let program = create_program(&[vert_shader, frag_shader]).unwrap();
                gl::DeleteShader(vert_shader);
                gl::DeleteShader(frag_shader);

                gl::UseProgram(program);

                gl::BindAttribLocation(program, 0, c"position".as_ptr() as _);

                gl::Uniform1i(gl::GetUniformLocation(program, c"bg0Tex".as_ptr() as _), 0);
                gl::Uniform1i(gl::GetUniformLocation(program, c"bg1Tex".as_ptr() as _), 1);
                gl::Uniform1i(gl::GetUniformLocation(program, c"bg2Tex".as_ptr() as _), 2);
                gl::Uniform1i(gl::GetUniformLocation(program, c"bg3Tex".as_ptr() as _), 3);
                gl::Uniform1i(gl::GetUniformLocation(program, c"objTex".as_ptr() as _), 4);
                gl::Uniform1i(gl::GetUniformLocation(program, c"objDepthTex".as_ptr() as _), 5);
                gl::Uniform1i(gl::GetUniformLocation(program, c"winTex".as_ptr() as _), 6);

                let mut ubo = 0;
                gl::GenBuffers(1, &mut ubo);
                gl::BindBuffer(gl::UNIFORM_BUFFER, ubo);

                if cfg!(target_os = "linux") {
                    gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, c"BlendUbo".as_ptr() as _), 0);
                }

                gl::UseProgram(0);

                (program, ubo)
            };

            Gpu2DCommon {
                win_bg_program,
                win_bg_disp_cnt_loc,
                win_bg_ubo,
                win_bg_fbo,
                win_obj_fbo: GpuFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false).unwrap(),
                obj_fbo: GpuFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, true).unwrap(),
                bg_fbos: array_init!({ GpuFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false).unwrap() }; 4),
                blend_program,
                blend_ubo,
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

            gl::BindAttribLocation(program, 0, c"position".as_ptr() as _);

            let disp_cnt_loc = gl::GetUniformLocation(program, c"dispCnt".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(program, c"lcdcPalTex".as_ptr() as _), 0);

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

struct Gpu2DObjProgram {
    program: GLuint,
    vao: GLuint,
    disp_cnt_loc: GLint,
    ubo: GLuint,
}

impl Gpu2DObjProgram {
    unsafe fn new<const ENGINE: Gpu2DEngine>(obj_vert_shader: GLuint, window: bool) -> Self {
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

        let frag_shader_src = frag_shader_src.replace("OBJ_WINDOW", if window { "true" } else { "false" });

        let frag_shader = create_shader(if window { "obj window" } else { "obj" }, &frag_shader_src, gl::FRAGMENT_SHADER).unwrap();
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

        gl::BindAttribLocation(program, 0, c"position".as_ptr() as _);
        gl::BindAttribLocation(program, 1, c"oamIndex".as_ptr() as _);

        gl::Uniform1i(gl::GetUniformLocation(program, c"oamTex".as_ptr() as _), 0);
        gl::Uniform1i(gl::GetUniformLocation(program, c"objTex".as_ptr() as _), 1);
        gl::Uniform1i(gl::GetUniformLocation(program, c"palTex".as_ptr() as _), 2);
        gl::Uniform1i(gl::GetUniformLocation(program, c"extPalTex".as_ptr() as _), 3);
        gl::Uniform1i(gl::GetUniformLocation(program, c"winTex".as_ptr() as _), 4);

        let disp_cnt_loc = gl::GetUniformLocation(program, c"dispCnt".as_ptr() as _);

        let mut ubo = 0;
        gl::GenBuffers(1, &mut ubo);
        gl::BindBuffer(gl::UNIFORM_BUFFER, ubo);

        if cfg!(target_os = "linux") {
            gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, c"ObjUbo".as_ptr() as _), 0);
            gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, c"WinBgUbo".as_ptr() as _), 1);
        }

        gl::UseProgram(0);

        Gpu2DObjProgram { program, vao, disp_cnt_loc, ubo }
    }
}

struct Gpu2DProgram {
    obj_oam_indices: Vec<[u16; 6]>,
    obj_ubo_data: ObjUbo,
    obj_window_program: Gpu2DObjProgram,
    obj_program: Gpu2DObjProgram,

    bg_affine_program: Gpu2DBgProgram,
    bg_affine_extended_program: Gpu2DBgProgram,
    bg_bitmap_program: Gpu2DBgProgram,
    bg_display_3d_program: Gpu2DBgProgram,
    bg_text_4bpp_program: Gpu2DBgProgram,
    bg_text_8bpp_program: Gpu2DBgProgram,
    bg_ubo: GLuint,

    pub blend_fbo: GpuFbo,
}

macro_rules! draw_scanlines {
    ($regs:expr, $draw_fn:expr, $draw_vram_display:expr, $lcdc_pal:expr, $is_bg:expr) => {{
        let mut line = 0;
        while line < DISPLAY_HEIGHT {
            let from_line = line;
            let from_disp_cnt = $regs.disp_cnts[from_line];
            let from_bg_cnts = &$regs.bg_cnts[from_line * 4..from_line * 4 + 4];
            line += 1;
            while line < DISPLAY_HEIGHT {
                if from_disp_cnt != $regs.disp_cnts[line] || ($is_bg && from_bg_cnts != &$regs.bg_cnts[line * 4..line * 4 + 4]) {
                    break;
                }
                line += 1;
            }

            if $lcdc_pal != 0 {
                let disp_cnt = DispCnt::from($regs.disp_cnts[from_line]);
                if u8::from(disp_cnt.display_mode()) == 2 {
                    if $draw_vram_display {
                        $draw_fn(from_line as u8, line as u8);
                    }
                    continue;
                }
            }
            $draw_fn(from_line as u8, line as u8);
        }
    }};
}

impl Gpu2DProgram {
    fn new<const ENGINE: Gpu2DEngine>(obj_window_vert_shader: GLuint, obj_vert_shader: GLuint, bg_vert_shader: GLuint, bg_vert_affine_shader: GLuint, bg_vert_bitmap_shader: GLuint) -> Self {
        unsafe {
            let (bg_affine_program, bg_affine_extended_program, bg_bitmap_program, bg_display_3d_program, bg_text_4bpp_program, bg_text_8bpp_program, bg_ubo) = {
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

                let frag_affine_shader = create_shader("bg affine", &(frag_common_shader_src.clone() + shader_source!("bg_frag_affine")), gl::FRAGMENT_SHADER).unwrap();
                let frag_affine_extended_shader = create_shader("bg affine extended", &(frag_common_shader_src.clone() + shader_source!("bg_frag_affine_extended")), gl::FRAGMENT_SHADER).unwrap();
                let frag_bitmap_shader = create_shader("bg bitmap", &(frag_common_shader_src.clone() + shader_source!("bg_frag_bitmap")), gl::FRAGMENT_SHADER).unwrap();
                let frag_display_3d_shader = create_shader("bg display 3d", &(frag_common_shader_src.clone() + shader_source!("bg_frag_display_3d")), gl::FRAGMENT_SHADER).unwrap();
                let frag_text_4bpp_shader = create_shader("bg text 4bpp", &(frag_common_shader_src.clone() + shader_source!("bg_frag_text_4bpp")), gl::FRAGMENT_SHADER).unwrap();
                let frag_text_8bpp_shader = create_shader("bg text 8bpp", &(frag_common_shader_src + shader_source!("bg_frag_text_8bpp")), gl::FRAGMENT_SHADER).unwrap();

                let affine_program = create_program(&[bg_vert_affine_shader, frag_affine_shader]).unwrap();
                let affine_extended_program = create_program(&[bg_vert_affine_shader, frag_affine_extended_shader]).unwrap();
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

                    gl::BindAttribLocation(program, 0, c"position".as_ptr() as _);

                    let disp_cnt_loc = gl::GetUniformLocation(program, c"dispCnt".as_ptr() as _);
                    let cnt_loc = gl::GetUniformLocation(program, c"bgCnt".as_ptr() as _);

                    gl::Uniform1i(gl::GetUniformLocation(program, c"bgTex".as_ptr() as _), 0);
                    gl::Uniform1i(gl::GetUniformLocation(program, c"palTex".as_ptr() as _), 1);
                    gl::Uniform1i(gl::GetUniformLocation(program, c"extPalTex".as_ptr() as _), 2);
                    gl::Uniform1i(gl::GetUniformLocation(program, c"winTex".as_ptr() as _), 3);
                    gl::Uniform1i(gl::GetUniformLocation(program, c"display3dTex".as_ptr() as _), 4);

                    if cfg!(target_os = "linux") && has_ubo {
                        gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, c"BgUbo".as_ptr() as _), 0);
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
                    init_program(affine_program, true),
                    init_program(affine_extended_program, true),
                    init_program(bitmap_program, true),
                    init_program(display_3d_program, false),
                    init_program(text_4bpp_program, true),
                    init_program(text_8bpp_program, true),
                    ubo,
                )
            };

            let blend_fbo = GpuFbo::new(DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32, false).unwrap();

            Gpu2DProgram {
                obj_window_program: Gpu2DObjProgram::new::<ENGINE>(obj_window_vert_shader, true),
                obj_program: Gpu2DObjProgram::new::<ENGINE>(obj_vert_shader, false),
                obj_oam_indices: Vec::new(),
                obj_ubo_data: ObjUbo::default(),
                bg_affine_program,
                bg_affine_extended_program,
                bg_bitmap_program,
                bg_display_3d_program,
                bg_text_4bpp_program,
                bg_text_8bpp_program,
                bg_ubo,
                blend_fbo,
            }
        }
    }

    unsafe fn draw_windows(&self, common: &Gpu2DCommon, regs: &Gpu2DRenderRegs, from_line: u8, to_line: u8) {
        let disp_cnt = DispCnt::from(regs.disp_cnts[from_line as usize]);
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

    unsafe fn draw_objects<const OBJ_WINDOW: bool>(&mut self, regs: &Gpu2DRenderRegs, mem: &Gpu2DMem, from_line: u8, to_line: u8) {
        let disp_cnt = DispCnt::from(regs.disp_cnts[from_line as usize]);
        if !disp_cnt.screen_display_obj() {
            return;
        }

        let highest_index = if OBJ_WINDOW {
            if disp_cnt.obj_window_display_flag() {
                self.assemble_oam(mem, from_line, to_line, disp_cnt, true)
            } else {
                return;
            }
        } else {
            self.assemble_oam(mem, from_line, to_line, disp_cnt, false)
        };

        if self.obj_oam_indices.is_empty() {
            return;
        }

        let program = if OBJ_WINDOW { &self.obj_window_program } else { &self.obj_program };
        gl::Uniform1i(program.disp_cnt_loc, u32::from(disp_cnt) as _);

        gl::BindBuffer(gl::UNIFORM_BUFFER, program.ubo);
        gl::BufferData(
            gl::UNIFORM_BUFFER,
            ((highest_index + 1) * size_of::<[u32; 2]>()) as _,
            ptr::addr_of!(self.obj_ubo_data) as _,
            gl::DYNAMIC_DRAW,
        );
        gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, program.ubo);

        gl::DrawElements(gl::TRIANGLES, (6 * self.obj_oam_indices.len()) as _, gl::UNSIGNED_SHORT, self.obj_oam_indices.as_ptr() as _);
    }

    unsafe fn draw_bg_program(&self, common: &Gpu2DCommon, regs: &Gpu2DRenderRegs, texs: &Gpu2DTextures, from_line: u8, to_line: u8, fb_tex_3d: GLuint, bg_num: u8, bg_mode: BgMode) {
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
            BgMode::Affine => &self.bg_affine_program,
            BgMode::Extended => {
                if bg_cnt.color_256_palettes() {
                    &self.bg_bitmap_program
                } else {
                    &self.bg_affine_extended_program
                }
            }
            BgMode::Large => {
                todo!()
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

        if fb_tex_3d != 0 {
            gl::ActiveTexture(gl::TEXTURE4);
            gl::BindTexture(gl::TEXTURE_2D, fb_tex_3d);
        }

        if program.has_ubo {
            gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, self.bg_ubo);
        }

        let disp_cnt = regs.disp_cnts[from_line as usize];
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
    }

    unsafe fn draw_bg(&self, common: &Gpu2DCommon, regs: &Gpu2DRenderRegs, texs: &Gpu2DTextures, from_line: u8, to_line: u8) {
        let disp_cnt = regs.disp_cnts[from_line as usize];

        let disp_cnt = DispCnt::from(disp_cnt);
        macro_rules! draw {
            ($bg3mode:expr, $bg2mode:expr, $bg1mode:expr, $bg0mode:expr) => {{
                if disp_cnt.screen_display_bg3() {
                    self.draw_bg_program(common, regs, texs, from_line, to_line, 0, 3, $bg3mode);
                }
                if disp_cnt.screen_display_bg2() {
                    self.draw_bg_program(common, regs, texs, from_line, to_line, 0, 2, $bg2mode);
                }
                if disp_cnt.screen_display_bg1() {
                    self.draw_bg_program(common, regs, texs, from_line, to_line, 0, 1, $bg1mode);
                }
                if disp_cnt.screen_display_bg0() && !disp_cnt.bg0_3d() {
                    self.draw_bg_program(common, regs, texs, from_line, to_line, 0, 0, $bg0mode);
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
                    self.draw_bg_program(common, regs, texs, from_line, to_line, 0, 2, BgMode::Large);
                }
            }
            7 => {}
            _ => unreachable_unchecked(),
        }

        gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
    }

    unsafe fn blend_fbos(&self, common: &Gpu2DCommon, regs: &Gpu2DRenderRegs, texs: &Gpu2DTextures, mem: &Gpu2DMem, fb_tex_3d: GLuint) -> GLuint {
        if fb_tex_3d != 0 {
            let draw_bg = |from_line, to_line| {
                let disp_cnt = DispCnt::from(regs.disp_cnts[from_line as usize]);
                if let 0..=5 = u8::from(disp_cnt.bg_mode()) {
                    if disp_cnt.screen_display_bg0() && disp_cnt.bg0_3d() {
                        self.draw_bg_program(common, regs, texs, from_line, to_line, fb_tex_3d, 0, BgMode::Display3d);
                    }
                }
            };
            draw_scanlines!(regs, draw_bg, false, 0, false);
        }

        gl::BindFramebuffer(gl::FRAMEBUFFER, self.blend_fbo.fbo);
        gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);

        let backdrop = utils::read_from_mem::<u16>(mem.pal, 0);
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

        self.blend_fbo.color
    }

    unsafe fn draw(&mut self, common: &Gpu2DCommon, regs: &Gpu2DRenderRegs, texs: &Gpu2DTextures, mem: Gpu2DMem, lcdc_pal: GLuint, vram_display_program: &Gpu2DVramDisplayProgram) {
        if cfg!(target_os = "linux") {
            gl::BindTexture(gl::TEXTURE_2D, texs.oam);
            sub_mem_texture1d(regions::OAM_SIZE / 2, mem.oam.as_ptr());

            gl::BindTexture(gl::TEXTURE_2D, texs.obj);
            sub_mem_texture2d(texs.obj_width, texs.obj_height, mem.obj.as_ptr());
        }

        {
            gl::UseProgram(self.obj_window_program.program);

            gl::BindVertexArray(self.obj_window_program.vao);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, texs.oam);

            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, texs.obj);

            gl::BindFramebuffer(gl::FRAMEBUFFER, common.win_obj_fbo.fbo);
            gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
            gl::ClearColor(0f32, 0f32, 0f32, 0f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            gl::BindBuffer(gl::UNIFORM_BUFFER, common.win_bg_ubo);
            gl::BufferData(gl::UNIFORM_BUFFER, size_of::<WinBgUbo>() as _, ptr::addr_of!(regs.win_bg_ubo) as _, gl::DYNAMIC_DRAW);
            gl::BindBufferBase(gl::UNIFORM_BUFFER, 1, common.win_bg_ubo);

            // This sets the bind index in vita gl by force
            // Shader cache doesn't seem to consider uniform block name and we can't use glUniformBlockBinding
            #[cfg(target_os = "vita")]
            crate::presenter::Presenter::gl_bind_frag_ubo(1);

            let mut draw_objects = |from_line, to_line| self.draw_objects::<true>(regs, &mem, from_line, to_line);
            draw_scanlines!(regs, draw_objects, false, lcdc_pal, false);

            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::BindVertexArray(0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }

        {
            gl::BindFramebuffer(gl::FRAMEBUFFER, common.win_bg_fbo.fbo);
            gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
            gl::ClearColor(1f32, 0f32, 0f32, 0f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            gl::UseProgram(common.win_bg_program);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, common.win_obj_fbo.color);

            gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, common.win_bg_ubo);

            let draw_windows = |from_line, to_line| self.draw_windows(common, regs, from_line, to_line);
            draw_scanlines!(regs, draw_windows, false, lcdc_pal, false);

            gl::BindBuffer(gl::UNIFORM_BUFFER, 0);
            gl::UseProgram(0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }

        if cfg!(target_os = "linux") {
            gl::BindTexture(gl::TEXTURE_2D, texs.pal);
            sub_pal_texture1d(regions::STANDARD_PALETTES_SIZE / 2, mem.pal.as_ptr());

            gl::BindTexture(gl::TEXTURE_2D, texs.obj_ext_pal);
            sub_pal_texture2d(1024, 8, mem.obj_ext_pal.as_ptr());
        }

        gl::BindTexture(gl::TEXTURE_2D, 0);

        {
            gl::UseProgram(self.obj_program.program);

            gl::BindVertexArray(self.obj_program.vao);

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
            gl::ClearColor(0f32, 0f32, 0f32, 0f32);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::ONE_MINUS_DST_ALPHA, gl::DST_ALPHA);

            gl::Enable(gl::DEPTH_TEST);
            gl::DepthFunc(gl::LEQUAL);

            let mut draw_objects = |from_line, to_line| self.draw_objects::<false>(regs, &mem, from_line, to_line);
            draw_scanlines!(regs, draw_objects, false, lcdc_pal, false);

            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::BLEND);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::BindVertexArray(0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }

        if cfg!(target_os = "linux") {
            gl::BindTexture(gl::TEXTURE_2D, texs.bg);
            sub_mem_texture2d(texs.bg_width, texs.bg_height, mem.bg.as_ptr());

            gl::BindTexture(gl::TEXTURE_2D, texs.bg_ext_pal);
            sub_pal_texture2d(1024, 32, mem.bg_ext_pal.as_ptr());
        }

        {
            for i in 0..4 {
                gl::BindFramebuffer(gl::FRAMEBUFFER, common.bg_fbos[i].fbo);
                gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
                gl::ClearColor(0f32, 0f32, 0f32, 1f32);
                gl::Clear(gl::COLOR_BUFFER_BIT);
            }

            gl::BindBuffer(gl::UNIFORM_BUFFER, self.bg_ubo);
            gl::BufferData(gl::UNIFORM_BUFFER, size_of::<BgUbo>() as _, ptr::addr_of!(regs.bg_ubo) as _, gl::DYNAMIC_DRAW);

            let draw_bg = |from_line, to_line| self.draw_bg(common, regs, texs, from_line, to_line);
            draw_scanlines!(regs, draw_bg, false, lcdc_pal, true);

            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        if lcdc_pal != 0 {
            if cfg!(target_os = "linux") {
                gl::BindTexture(gl::TEXTURE_2D, lcdc_pal);
                sub_pal_texture2d(1024, 656, mem.lcdc.as_ptr());
            }

            gl::UseProgram(vram_display_program.program);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, lcdc_pal);

            // Use any of the bg fbos to draw the vram into
            // At this point all other fbo won't contain any pixels for blending
            gl::BindFramebuffer(gl::FRAMEBUFFER, common.bg_fbos[0].fbo);
            gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);

            let draw_vram_display = |from_line, to_line| vram_display_program.draw(regs, from_line, to_line);
            draw_scanlines!(regs, draw_vram_display, true, lcdc_pal, false);

            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::UseProgram(0);
        }

        gl::UseProgram(0);
    }

    fn assemble_oam(&mut self, mem: &Gpu2DMem, from_line: u8, to_line: u8, disp_cnt: DispCnt, window: bool) -> usize {
        const OAM_COUNT: usize = regions::OAM_SIZE as usize / 2 / size_of::<OamAttribs>();
        let oams = unsafe { slice::from_raw_parts(mem.oam.as_ptr() as *const OamAttribs, OAM_COUNT) };

        let mut highest_index = 0;
        self.obj_oam_indices.clear();
        for (i, oam) in oams.iter().enumerate() {
            let attrib0 = OamAttrib0::from(oam.attr0);
            let obj_mode = attrib0.get_obj_mode();
            let gfx_mode = attrib0.get_gfx_mode();
            if obj_mode == OamObjMode::Disabled || (gfx_mode == OamGfxMode::Window) != window {
                continue;
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
                    self.obj_ubo_data.attrs[i].map_width = width as u32;
                    self.obj_ubo_data.attrs[i].obj_bound = u16::from(OamAttrib2::from(oam.attr2).tile_index()) as u32 * if disp_cnt.bitmap_obj_1d_boundary() { 256 } else { 128 };
                } else {
                    self.obj_ubo_data.attrs[i].map_width = if disp_cnt.bitmap_obj_2d() { 256 } else { 128 };
                    let x_mask = if disp_cnt.bitmap_obj_2d() { 0x1F } else { 0x0F };
                    self.obj_ubo_data.attrs[i].obj_bound = (oam.attr2 & x_mask) as u32 * 0x10 + (oam.attr2 & 0x3FF & !x_mask) as u32 * 0x80;
                }
            } else if disp_cnt.tile_1d_obj_mapping() {
                self.obj_ubo_data.attrs[i].map_width = width as u32;
                self.obj_ubo_data.attrs[i].obj_bound = 32 << u8::from(disp_cnt.tile_obj_1d_boundary());
            } else {
                self.obj_ubo_data.attrs[i].map_width = if attrib0.is_8bit() { 128 } else { 256 };
                self.obj_ubo_data.attrs[i].obj_bound = 32;
            }

            let index_base = (i * 4) as u16;
            self.obj_oam_indices.push([index_base, index_base + 1, index_base + 2, index_base, index_base + 2, index_base + 3]);
            if i > highest_index {
                highest_index = i;
            }
        }
        highest_index
    }
}

pub struct Gpu2DRenderer {
    lcdc_pal: GLuint,
    vram_display_program: Gpu2DVramDisplayProgram,
    texs: [Gpu2DTextures; 2],
    pub common: Gpu2DCommon,
    program_a: Gpu2DProgram,
    program_b: Gpu2DProgram,
    #[cfg(target_os = "linux")]
    lcdc_mem_buf: HeapMemU8<{ vram::TOTAL_SIZE }>,
}

impl Gpu2DRenderer {
    pub fn new() -> Self {
        unsafe {
            let obj_window_vert_shader = create_shader("obj window", &shader_source!("obj_vert").replace("OBJ_WINDOW", "true"), gl::VERTEX_SHADER).unwrap();
            let obj_vert_shader = create_shader("obj", &shader_source!("obj_vert").replace("OBJ_WINDOW", "false"), gl::VERTEX_SHADER).unwrap();
            let bg_vert_shader = create_shader("bg", shader_source!("bg_vert"), gl::VERTEX_SHADER).unwrap();
            let bg_vert_affine_shader = create_shader("bg affine", shader_source!("bg_vert_affine"), gl::VERTEX_SHADER).unwrap();
            let bg_vert_bitmap_shader = create_shader("bg bitmap", shader_source!("bg_vert_bitmap"), gl::VERTEX_SHADER).unwrap();

            let instance = Gpu2DRenderer {
                lcdc_pal: create_pal_texture2d(1024, 656),
                vram_display_program: Gpu2DVramDisplayProgram::new(),
                texs: [
                    Gpu2DTextures::new(1024, OBJ_A_TEX_HEIGHT, 1024, BG_A_TEX_HEIGHT),
                    Gpu2DTextures::new(1024, OBJ_B_TEX_HEIGHT, 1024, BG_B_TEX_HEIGHT),
                ],
                common: Gpu2DCommon::new(),
                program_a: Gpu2DProgram::new::<{ A }>(obj_window_vert_shader, obj_vert_shader, bg_vert_shader, bg_vert_affine_shader, bg_vert_bitmap_shader),
                program_b: Gpu2DProgram::new::<{ B }>(obj_window_vert_shader, obj_vert_shader, bg_vert_shader, bg_vert_affine_shader, bg_vert_bitmap_shader),
                #[cfg(target_os = "linux")]
                lcdc_mem_buf: HeapMemU8::new(),
            };

            gl::DeleteShader(obj_window_vert_shader);
            gl::DeleteShader(obj_vert_shader);
            gl::DeleteShader(bg_vert_shader);
            gl::DeleteShader(bg_vert_affine_shader);
            gl::DeleteShader(bg_vert_bitmap_shader);

            instance
        }
    }

    pub fn set_tex_ptrs(&mut self, refs: &mut GpuMemRefs) {
        #[cfg(target_os = "linux")]
        unsafe {
            refs.lcdc = PtrWrapper::new(mem::transmute(self.lcdc_mem_buf.as_mut_ptr()));

            refs.bg_a = PtrWrapper::new(mem::transmute(self.texs[0].mem.bg.as_mut_ptr()));
            refs.obj_a = PtrWrapper::new(mem::transmute(self.texs[0].mem.obj.as_mut_ptr()));
            refs.bg_a_ext_pal = PtrWrapper::new(mem::transmute(self.texs[0].mem.bg_ext_pal.as_mut_ptr()));
            refs.obj_a_ext_pal = PtrWrapper::new(mem::transmute(self.texs[0].mem.obj_ext_pal.as_mut_ptr()));
            refs.oam_a = PtrWrapper::new(mem::transmute(self.texs[0].mem.oam.as_mut_ptr()));
            refs.pal_a = PtrWrapper::new(mem::transmute(self.texs[0].mem.pal.as_mut_ptr()));

            refs.bg_b = PtrWrapper::new(mem::transmute(self.texs[1].mem.bg.as_mut_ptr()));
            refs.obj_b = PtrWrapper::new(mem::transmute(self.texs[1].mem.obj.as_mut_ptr()));
            refs.bg_b_ext_pal = PtrWrapper::new(mem::transmute(self.texs[1].mem.bg_ext_pal.as_mut_ptr()));
            refs.obj_b_ext_pal = PtrWrapper::new(mem::transmute(self.texs[1].mem.obj_ext_pal.as_mut_ptr()));
            refs.oam_b = PtrWrapper::new(mem::transmute(self.texs[1].mem.oam.as_mut_ptr()));
            refs.pal_b = PtrWrapper::new(mem::transmute(self.texs[1].mem.pal.as_mut_ptr()));
        }

        #[cfg(target_os = "vita")]
        unsafe {
            use crate::presenter::Presenter;
            gl::BindTexture(gl::TEXTURE_2D, self.lcdc_pal);
            refs.lcdc = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));

            gl::BindTexture(gl::TEXTURE_2D, self.texs[0].bg);
            refs.bg_a = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
            gl::BindTexture(gl::TEXTURE_2D, self.texs[0].obj);
            refs.obj_a = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
            gl::BindTexture(gl::TEXTURE_2D, self.texs[0].bg_ext_pal);
            refs.bg_a_ext_pal = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
            gl::BindTexture(gl::TEXTURE_2D, self.texs[0].obj_ext_pal);
            refs.obj_a_ext_pal = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
            gl::BindTexture(gl::TEXTURE_2D, self.texs[0].oam);
            refs.oam_a = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
            gl::BindTexture(gl::TEXTURE_2D, self.texs[0].pal);
            refs.pal_a = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));

            gl::BindTexture(gl::TEXTURE_2D, self.texs[1].bg);
            refs.bg_b = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
            gl::BindTexture(gl::TEXTURE_2D, self.texs[1].obj);
            refs.obj_b = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
            gl::BindTexture(gl::TEXTURE_2D, self.texs[1].bg_ext_pal);
            refs.bg_b_ext_pal = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
            gl::BindTexture(gl::TEXTURE_2D, self.texs[1].obj_ext_pal);
            refs.obj_b_ext_pal = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
            gl::BindTexture(gl::TEXTURE_2D, self.texs[1].oam);
            refs.oam_b = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
            gl::BindTexture(gl::TEXTURE_2D, self.texs[1].pal);
            refs.pal_b = PtrWrapper::new(mem::transmute(Presenter::gl_remap_tex()));
        }
    }

    pub unsafe fn draw<const ENGINE: Gpu2DEngine>(&mut self, mem_refs: &GpuMemRefs, regs: &Gpu2DRenderRegsShared) {
        match ENGINE {
            A => self.program_a.draw(
                &self.common,
                &regs.regs_a[0],
                &self.texs[0],
                Gpu2DMem::new::<{ A }>(mem_refs),
                if regs.has_vram_display[0] { self.lcdc_pal } else { 0 },
                &self.vram_display_program,
            ),
            B => self
                .program_b
                .draw(&self.common, &regs.regs_b[0], &self.texs[1], Gpu2DMem::new::<{ B }>(mem_refs), 0, &self.vram_display_program),
        }
    }

    pub unsafe fn blend<const ENGINE: Gpu2DEngine>(&mut self, mem_refs: &GpuMemRefs, regs: &Gpu2DRenderRegsShared, fb_tex_3d: GLuint) -> GLuint {
        match ENGINE {
            A => self.program_a.blend_fbos(&self.common, &regs.regs_a[0], &self.texs[0], &Gpu2DMem::new::<{ A }>(mem_refs), fb_tex_3d),
            B => self.program_b.blend_fbos(&self.common, &regs.regs_b[0], &self.texs[1], &Gpu2DMem::new::<{ B }>(mem_refs), 0),
        }
    }
}
