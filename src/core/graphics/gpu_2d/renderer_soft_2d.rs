use crate::core::graphics::gl_utils::{create_program, create_shader, shader_source, GpuFbo};
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_2d::registers_2d::{BgCnt, DispCnt};
use crate::core::graphics::gpu_2d::renderer_regs_2d::{BlendUbo, Gpu2DMem, Gpu2DRenderRegs, Gpu2DRenderRegsShared};
use crate::core::graphics::gpu_2d::{
    Gpu2DEngine,
    Gpu2DEngine::{A, B},
};
use crate::core::graphics::gpu_mem_buf::GpuMemRefs;
use crate::core::graphics::gpu_renderer::GpuRendererCommon;
use crate::core::memory::oam::{OamAttrib0, OamAttrib1, OamAttrib2, OamAttribs, OamGfxMode, OamObjMode};
use crate::core::memory::{regions, vram};
use crate::utils::{self, array_init, HeapArray};
use bilge::prelude::*;
use core::slice;
use gl::types::GLuint;
#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::{vandq_u8, vdupq_n_u8, vget_high_u8, vget_low_u8, vld1q_u8_x2, vld1q_u8_x4, vrev64q_u8, vshrq_n_u8, vst1_u8, vst1q_u8, vzipq_u8};
#[cfg(target_arch = "arm")]
use std::arch::arm::{vandq_u8, vdupq_n_u8, vget_high_u8, vget_low_u8, vld1q_u8_x2, vld1q_u8_x4, vrev64q_u8, vshrq_n_u8, vst1_u8, vst1q_u8, vzipq_u8};
use std::hint::{assert_unchecked, unreachable_unchecked};
use std::marker::ConstParamTy;
use std::mem::MaybeUninit;
use std::ptr;

#[derive(ConstParamTy, Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
enum BgMode {
    Text = 0,
    Affine = 1,
    Extended = 2,
    Large = 3,
    Display3d = 4,
}

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
pub struct BgScreenEntry {
    tile_index: u10,
    h_flip: bool,
    v_flip: bool,
    pal_bank: u4,
}

#[bitsize(32)]
#[derive(Copy, Clone, FromBits)]
pub struct FrameLayer {
    r: u8,
    g: u8,
    b: u8,
    a: bool,
    inv_prio: u2,
    inv_layer: u3,
    inv_prio_3d: u2,
}

impl Default for FrameLayer {
    fn default() -> Self {
        FrameLayer::from(0)
    }
}

#[bitsize(8)]
#[derive(Copy, Clone, FromBits)]
pub struct FrameWindow {
    bg0: bool,
    bg1: bool,
    bg2: bool,
    bg3: bool,
    obj: bool,
    bld: bool,
    is_in: bool,
    unused: u1,
}

impl Default for FrameWindow {
    fn default() -> Self {
        FrameWindow::new(true, true, true, true, true, true, false, u1::new(0))
    }
}

struct Gpu2DFrame {
    layers: [HeapArray<FrameLayer, { DISPLAY_WIDTH * DISPLAY_HEIGHT }>; 2],
    window: HeapArray<FrameWindow, { DISPLAY_WIDTH * DISPLAY_HEIGHT }>,
    obj_window: HeapArray<bool, { DISPLAY_WIDTH * DISPLAY_HEIGHT }>,
    layer_texs: [GLuint; 2],
    fbo: GpuFbo,
}

impl Gpu2DFrame {
    fn new() -> Self {
        Gpu2DFrame {
            layers: [HeapArray::default(), HeapArray::default()],
            window: HeapArray::default(),
            obj_window: HeapArray::default(),
            layer_texs: array_init!({ unsafe {
                let mut tex = 0;
                gl::GenTextures(1, &mut tex);
                gl::BindTexture(gl::TEXTURE_2D, tex);
                gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, 0, gl::RGBA, gl::UNSIGNED_BYTE, ptr::null());
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
                gl::BindTexture(gl::TEXTURE_2D, 0);
                tex
            }}; 2),
            fbo: GpuFbo::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false, false).unwrap(),
        }
    }

    fn set_window(&mut self, x: usize, y: usize, bits: u8, win_in: bool) {
        unsafe {
            assert_unchecked(y < DISPLAY_HEIGHT);
            assert_unchecked(x < DISPLAY_WIDTH);
        }
        let mut value = FrameWindow::from(bits);
        value.set_is_in(win_in);
        self.window[y * DISPLAY_WIDTH + x] = value;
    }

    fn can_display_pixel(&self, x: usize, y: usize, bit: u8) -> bool {
        unsafe {
            assert_unchecked(y < DISPLAY_HEIGHT);
            assert_unchecked(x < DISPLAY_WIDTH);
        }
        u8::from(self.window[y * DISPLAY_WIDTH + x]) & (1 << bit) != 0
    }

    fn reset_obj_window(&mut self, from_line: usize, to_line: usize) {
        self.obj_window[from_line * DISPLAY_WIDTH..to_line * DISPLAY_WIDTH].fill(false);
    }

    fn mark_obj_window(&mut self, x: usize, y: usize) {
        unsafe {
            assert_unchecked(y < DISPLAY_HEIGHT);
            assert_unchecked(x < DISPLAY_WIDTH);
        }
        self.obj_window[y * DISPLAY_WIDTH + x] = true;
    }

    fn is_obj_window(&self, x: usize, y: usize) -> bool {
        unsafe {
            assert_unchecked(y < DISPLAY_HEIGHT);
            assert_unchecked(x < DISPLAY_WIDTH);
        }
        self.obj_window[y * DISPLAY_WIDTH + x]
    }

    fn set_obj_pixel(&mut self, x: usize, y: usize, color: u16, prio: u8) {
        unsafe {
            assert_unchecked(y < DISPLAY_HEIGHT);
            assert_unchecked(x < DISPLAY_WIDTH);
        }
        let color = utils::rgba5_to_rgba8_non_norm(color);
        let layer = &mut self.layers[0][y * DISPLAY_WIDTH + x];
        let inv_prio = 3 - prio;
        if u8::from(layer.inv_layer()) == 0 || inv_prio > u8::from(layer.inv_prio()) {
            layer.value = color;
            layer.set_inv_prio(u2::new(inv_prio));
            layer.set_inv_layer(u3::new(1));
        }
    }

    fn set_bg_pixel(&mut self, x: usize, y: usize, color: u16, bg: u8, prio: u8) {
        unsafe {
            assert_unchecked(y < DISPLAY_HEIGHT);
            assert_unchecked(x < DISPLAY_WIDTH);
        }

        let color = utils::rgba5_to_rgba8_non_norm(color);
        let top_layer = self.layers[0][y * DISPLAY_WIDTH + x];
        let bottom_layer = self.layers[1][y * DISPLAY_WIDTH + x];
        let inv_prio = 3 - prio;
        if {
            let adjusted_prio = inv_prio as i8 - if u8::from(top_layer.inv_layer()) == 1 { 1 } else { 0 };
            adjusted_prio >= u8::from(top_layer.inv_prio()) as i8
        } {
            self.layers[1][y * DISPLAY_WIDTH + x] = top_layer;

            let top_layer = &mut self.layers[0][y * DISPLAY_WIDTH + x];
            top_layer.value = color;
            top_layer.set_inv_prio(u2::new(inv_prio));
            top_layer.set_inv_layer(u3::new(5 - bg));
        } else if {
            let adjusted_prio = inv_prio as i8 - if u8::from(bottom_layer.inv_layer()) == 1 { 1 } else { 0 };
            adjusted_prio >= u8::from(bottom_layer.inv_prio()) as i8
        } {
            let bottom_layer = &mut self.layers[1][y * DISPLAY_WIDTH + x];
            bottom_layer.value = color;
            bottom_layer.set_inv_prio(u2::new(inv_prio));
            bottom_layer.set_inv_layer(u3::new(5 - bg));
        }
    }

    fn set_3d_pixel(&mut self, x: usize, y: usize, prio: u8) {
        unsafe {
            assert_unchecked(y < DISPLAY_HEIGHT);
            assert_unchecked(x < DISPLAY_WIDTH);
        }

        let bottom_layer = self.layers[1][y * DISPLAY_WIDTH + x];
        let inv_prio = 3 - prio;
        if {
            let adjusted_prio = inv_prio as i8 - if u8::from(bottom_layer.inv_layer()) == 1 { 1 } else { 0 };
            adjusted_prio >= u8::from(bottom_layer.inv_prio()) as i8
        } {
            self.layers[0][y * DISPLAY_WIDTH + x].set_inv_prio_3d(u2::new(inv_prio));
            self.layers[1][y * DISPLAY_WIDTH + x].set_inv_prio_3d(u2::new(1)); // Indicate that this pixel could be 3d
        }
    }

    fn reset_window(&mut self, from_line: usize, to_line: usize) {
        let window = &mut self.window[from_line * DISPLAY_WIDTH..to_line * DISPLAY_WIDTH];
        window.fill(FrameWindow::default());
    }

    fn fill_pixels(&mut self, color: u16) {
        let layer = FrameLayer::from(utils::rgba5_to_rgba8_non_norm(color));
        self.layers[0].fill(layer);
        self.layers[1].fill(layer);
    }

    fn copy_textures(&self) {
        unsafe {
            for i in 0..2 {
                gl::BindTexture(gl::TEXTURE_2D, self.layer_texs[i]);
                gl::TexSubImage2D(
                    gl::TEXTURE_2D,
                    0,
                    0,
                    0,
                    DISPLAY_WIDTH as _,
                    DISPLAY_HEIGHT as _,
                    gl::RGBA,
                    gl::UNSIGNED_BYTE,
                    self.layers[i].as_ptr() as _,
                );
            }

            gl::BindTexture(gl::TEXTURE_2D, 0);
        }
    }
}

struct Gpu2dBlendProgram {
    program: GLuint,
    ubo: GLuint,
}

impl Gpu2dBlendProgram {
    fn new() -> Self {
        let (program, ubo) = unsafe {
            let vert_shader = create_shader("blend_new", shader_source!("blend_vert_new"), gl::VERTEX_SHADER).unwrap();
            let frag_shader = create_shader("blend_new", shader_source!("blend_frag_new"), gl::FRAGMENT_SHADER).unwrap();
            let program = create_program(&[vert_shader, frag_shader]).unwrap();
            gl::DeleteShader(vert_shader);
            gl::DeleteShader(frag_shader);

            gl::UseProgram(program);

            gl::BindAttribLocation(program, 0, c"coords".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(program, c"topLayer".as_ptr() as _), 0);
            gl::Uniform1i(gl::GetUniformLocation(program, c"bottomLayer".as_ptr() as _), 1);
            gl::Uniform1i(gl::GetUniformLocation(program, c"tex3d".as_ptr() as _), 2);

            let mut ubo = 0;
            gl::GenBuffers(1, &mut ubo);
            gl::BindBuffer(gl::UNIFORM_BUFFER, ubo);

            if cfg!(not(target_os = "vita")) {
                gl::UniformBlockBinding(program, gl::GetUniformBlockIndex(program, c"BlendUbo".as_ptr() as _), 0);
            }

            gl::UseProgram(0);

            (program, ubo)
        };
        Gpu2dBlendProgram { program, ubo }
    }

    fn draw(&self, frame: &Gpu2DFrame, regs: &Gpu2DRenderRegs, fb_tex_3d: GLuint) {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, frame.fbo.fbo);
            gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
            gl::ClearColor(0f32, 0f32, 0f32, 1f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            gl::UseProgram(self.program);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, frame.layer_texs[0]);

            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D, frame.layer_texs[1]);

            gl::ActiveTexture(gl::TEXTURE2);
            gl::BindTexture(gl::TEXTURE_2D, fb_tex_3d);

            gl::BindBuffer(gl::UNIFORM_BUFFER, self.ubo);
            gl::BufferData(gl::UNIFORM_BUFFER, size_of::<BlendUbo>() as _, ptr::addr_of!(regs.blend_ubo) as _, gl::DYNAMIC_DRAW);
            gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, self.ubo);

            const COORDS: [f32; 4 * 4] = [-1f32, 1f32, 0f32, 0f32, 1f32, 1f32, 1f32, 0f32, 1f32, -1f32, 1f32, 1f32, -1f32, -1f32, 0f32, 1f32];

            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(0, 4, gl::FLOAT, gl::FALSE, 0, COORDS.as_ptr() as _);
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);

            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }
    }
}

struct Gpu2DProgram {
    blend_program: Gpu2dBlendProgram,
}

impl Gpu2DProgram {
    fn new() -> Self {
        Gpu2DProgram {
            blend_program: Gpu2dBlendProgram::new(),
        }
    }

    unsafe fn draw_windows(frame: &mut Gpu2DFrame, regs: &Gpu2DRenderRegs, from_line: usize, to_line: usize) {
        assert_unchecked(to_line <= DISPLAY_HEIGHT);
        assert_unchecked(from_line < to_line);

        let disp_cnt = DispCnt::from(regs.disp_cnt(from_line));
        if disp_cnt.window0_display_flag() || disp_cnt.window1_display_flag() || disp_cnt.obj_window_display_flag() {
            let parse_win_h = |win_h_v: u32| {
                let win_h = win_h_v & 0xFFFF;
                let win_left = (win_h >> 8) as u8;
                let win_right = win_h as u8;
                let win_width = win_right.wrapping_sub(win_left);
                (win_left, win_width)
            };

            let is_line_in_win = |line, win_h_v: u32| {
                let win_v = win_h_v >> 16;
                let win_top = (win_v >> 8) as u8;
                let win_bottom = win_v as u8;
                let win_height = win_bottom.wrapping_sub(win_top);
                let line_height = (line as u8).wrapping_sub(win_top);
                line_height < win_height
            };

            for line in from_line..to_line {
                let win_h_v_0 = regs.win_bg_ubo.win_h_v[line * 2];
                let win_h_v_1 = regs.win_bg_ubo.win_h_v[line * 2 + 1];

                let check_win_0 = disp_cnt.window0_display_flag() && is_line_in_win(line, win_h_v_0);
                let check_win_1 = disp_cnt.window1_display_flag() && is_line_in_win(line, win_h_v_1);

                let (win_left_0, win_width_0) = parse_win_h(win_h_v_0);
                let (win_left_1, win_width_1) = parse_win_h(win_h_v_1);

                let win_in_out = regs.win_bg_ubo.win_in_out[line];
                let win_in = win_in_out & 0xFFFF;
                let win_out = win_in_out >> 16;

                for x in 0..DISPLAY_WIDTH {
                    let is_win_in_0 = check_win_0 && (x as u8).wrapping_sub(win_left_0) < win_width_0;
                    if is_win_in_0 {
                        frame.set_window(x, line, win_in as u8, true);
                        continue;
                    }

                    let is_win_in_1 = check_win_1 && (x as u8).wrapping_sub(win_left_1) < win_width_1;
                    if is_win_in_1 {
                        frame.set_window(x, line, (win_in >> 8) as u8, true);
                        continue;
                    }

                    if disp_cnt.obj_window_display_flag() && frame.is_obj_window(x, line) {
                        frame.set_window(x, line, (win_out >> 8) as u8, false);
                        continue;
                    }

                    frame.set_window(x, line, win_out as u8, false);
                }
            }
        } else {
            frame.reset_window(from_line, to_line);
        }
    }

    unsafe fn draw_bg_text(frame: &mut Gpu2DFrame, mem: &Gpu2DMem, regs: &Gpu2DRenderRegs, from_line: usize, to_line: usize, bg: usize) {
        assert_unchecked(to_line <= DISPLAY_HEIGHT);
        assert_unchecked(from_line < to_line);
        assert_unchecked(bg < 4);

        let line_count = to_line - from_line;
        let disp_cnt = DispCnt::from(regs.disp_cnt(from_line));
        let bg_cnt = BgCnt::from(regs.bg_cnt(from_line, bg));
        let (h_ofs, v_ofs) = regs.ofs(from_line, bg);

        if bg_cnt.mosaic() {
            // todo!()
        }

        let screen_base = u32::from(disp_cnt.screen_base()) * 64 * 1024 + u32::from(bg_cnt.screen_base_block()) * 2 * 1024;
        let char_base = u32::from(disp_cnt.char_base()) * 64 * 1024 + u32::from(bg_cnt.char_base_block()) * 16 * 1024;

        let screen_size = u8::from(bg_cnt.screen_size());
        let screen_width = 256 << (screen_size & 1);
        let screen_height = 256 << (screen_size >> 1);
        let entries_width = screen_width >> 3;
        let entries_height = screen_height >> 3;

        let h_overflow = ((screen_size & 1) as u32) << 11;
        let v_overflow = ((screen_size >> 1) as u32) << 11;

        let start_x = (h_ofs as usize) & (screen_width - 1);
        let start_y = (from_line + v_ofs as usize) & (screen_height - 1);
        let end_x = start_x + DISPLAY_WIDTH;
        let end_y = start_y + line_count;

        let x_entry_start = start_x >> 3;
        let x_entry_end = utils::align_up(end_x, 8) >> 3;

        let y_entry_start = start_y >> 3;
        let y_entry_end = utils::align_up(end_y, 8) >> 3;

        let mut y_pixel_offset = start_y;

        let pal_shift = if bg_cnt.color_256_palettes() { 9 } else { 5 };
        let pal = if bg_cnt.color_256_palettes() {
            if disp_cnt.bg_extended_palettes() {
                let offset = (if bg < 2 && bg_cnt.ext_palette_slot_display_area_overflow() { bg + 2 } else { bg }) * (vram::BG_EXT_PAL_SIZE / 4);
                slice::from_raw_parts(mem.bg_ext_pal.as_ptr().add(offset), vram::BG_EXT_PAL_SIZE / 4)
            } else {
                mem.pal
            }
        } else {
            mem.pal
        };

        for y_entry_index in y_entry_start..y_entry_end {
            let y_entry_index = y_entry_index & (entries_height - 1);
            let mut screen_base = screen_base + ((y_entry_index & 31) << 6) as u32;
            if y_entry_index >= 32 {
                screen_base += h_overflow + v_overflow;
            }

            let y_pixel_start = from_line + y_pixel_offset - start_y;
            let char_y = y_pixel_offset & 7;
            let y_pixel_count = 8 - char_y;
            y_pixel_offset += y_pixel_count;

            let mut x_pixel_offset = start_x;

            for x_entry_index in x_entry_start..x_entry_end {
                let x_entry_index = x_entry_index & (entries_width - 1);
                let mut screen_addr = screen_base + ((x_entry_index & 31) << 1) as u32;
                if x_entry_index >= 32 {
                    screen_addr += h_overflow;
                }

                let screen_entry = utils::read_from_mem::<BgScreenEntry>(mem.bg, screen_addr);

                let mut palette_indices: [u8; 64] = MaybeUninit::uninit().assume_init();

                let mut char_tile = if bg_cnt.color_256_palettes() {
                    let char_addr = char_base + u32::from(screen_entry.tile_index()) * 64;
                    let chars = vld1q_u8_x4(mem.bg.as_ptr().add(char_addr as usize) as _);
                    [chars.0, chars.1, chars.2, chars.3]
                } else {
                    let char_addr = char_base + u32::from(screen_entry.tile_index()) * 32;
                    let chars = vld1q_u8_x2(mem.bg.as_ptr().add(char_addr as usize) as _);
                    let mask = vdupq_n_u8(0xF);
                    let chars = [vandq_u8(chars.0, mask), vshrq_n_u8::<4>(chars.0), vandq_u8(chars.1, mask), vshrq_n_u8::<4>(chars.1)];

                    let char_tile = [vzipq_u8(chars[0], chars[1]), vzipq_u8(chars[2], chars[3])];
                    [char_tile[0].0, char_tile[0].1, char_tile[1].0, char_tile[1].1]
                };

                if screen_entry.h_flip() {
                    for i in 0..char_tile.len() {
                        char_tile[i] = vrev64q_u8(char_tile[i]);
                    }
                }

                if screen_entry.v_flip() {
                    for i in 0..char_tile.len() {
                        let char = char_tile[char_tile.len() - 1 - i];
                        vst1_u8(palette_indices.as_mut_ptr().add(i * 16), vget_high_u8(char));
                        vst1_u8(palette_indices.as_mut_ptr().add(i * 16 + 8), vget_low_u8(char));
                    }
                } else {
                    for i in 0..char_tile.len() {
                        vst1q_u8(palette_indices.as_mut_ptr().add(i * 16), char_tile[i]);
                    }
                }

                let x_pixel_start = x_pixel_offset - start_x;
                let char_x = x_pixel_offset & 7;
                let x_pixel_count = 8 - char_x;
                x_pixel_offset += x_pixel_count;

                let pal_bank_addr = if bg_cnt.color_256_palettes() && !disp_cnt.bg_extended_palettes() {
                    0
                } else {
                    u32::from(screen_entry.pal_bank()) << pal_shift
                };

                for y in 0..y_pixel_count {
                    let y_pixel = y_pixel_start + y;
                    if y_pixel >= to_line {
                        break;
                    }
                    let char_y = (char_y + y) * 8;
                    for x in 0..x_pixel_count {
                        let x_pixel = x_pixel_start + x;
                        if x_pixel >= DISPLAY_WIDTH {
                            break;
                        }

                        if !frame.can_display_pixel(x_pixel, y_pixel, bg as u8) {
                            continue;
                        }

                        let char_x = char_x + x;
                        let pal_index = *palette_indices.get_unchecked(char_y + char_x);
                        if pal_index != 0 {
                            let color = utils::read_from_mem::<u16>(pal, pal_bank_addr + ((pal_index as u32) << 1)) | (1 << 15);
                            frame.set_bg_pixel(x_pixel, y_pixel, color, bg as u8, u8::from(bg_cnt.priority()));
                        }
                    }
                }
            }
        }
    }

    // Affine coordinates are pre-accumulated per scanline in bg_ubo (async/batched rendering, not
    // NooDS' per-scanline internalX/Y): the effective internal register is x + accumulated pb.
    fn affine_start(regs: &Gpu2DRenderRegs, bg: usize, line: usize) -> (i32, i32, i32, i32) {
        let idx = (bg - 2) * DISPLAY_HEIGHT + line;
        let internal_x = regs.bg_ubo.x[idx].wrapping_add(regs.bg_ubo.pb[idx]);
        let internal_y = regs.bg_ubo.y[idx].wrapping_add(regs.bg_ubo.pd[idx]);
        (internal_x, internal_y, regs.bg_ubo.pa[idx], regs.bg_ubo.pc[idx])
    }

    unsafe fn draw_bg_affine(frame: &mut Gpu2DFrame, mem: &Gpu2DMem, regs: &Gpu2DRenderRegs, from_line: usize, to_line: usize, bg: usize) {
        assert_unchecked(to_line <= DISPLAY_HEIGHT);
        assert_unchecked(from_line < to_line);
        assert_unchecked(bg == 2 || bg == 3);

        let disp_cnt = DispCnt::from(regs.disp_cnt(from_line));
        let bg_cnt = BgCnt::from(regs.bg_cnt(from_line, bg));

        let screen_base = u32::from(disp_cnt.screen_base()) * 64 * 1024 + u32::from(bg_cnt.screen_base_block()) * 2 * 1024;
        let char_base = u32::from(disp_cnt.char_base()) * 64 * 1024 + u32::from(bg_cnt.char_base_block()) * 16 * 1024;

        let size = 128i32 << u8::from(bg_cnt.screen_size());
        let entries_width = (size / 8) as u32;
        let wrap = bg_cnt.ext_palette_slot_display_area_overflow();
        let priority = u8::from(bg_cnt.priority());

        for line in from_line..to_line {
            let (internal_x, internal_y, pa, pc) = Self::affine_start(regs, bg, line);
            for x in 0..DISPLAY_WIDTH {
                if !frame.can_display_pixel(x, line, bg as u8) {
                    continue;
                }
                let mut coord_x = internal_x.wrapping_add(x as i32 * pa) >> 8;
                let mut coord_y = internal_y.wrapping_add(x as i32 * pc) >> 8;
                if wrap {
                    coord_x &= size - 1;
                    coord_y &= size - 1;
                } else if coord_x < 0 || coord_x >= size || coord_y < 0 || coord_y >= size {
                    continue;
                }
                let coord_x = coord_x as u32;
                let coord_y = coord_y as u32;

                let tile = utils::read_from_mem::<u8>(mem.bg, screen_base + (coord_y >> 3) * entries_width + (coord_x >> 3));
                let pal_index = utils::read_from_mem::<u8>(mem.bg, char_base + tile as u32 * 64 + (coord_y & 7) * 8 + (coord_x & 7));
                if pal_index != 0 {
                    let color = utils::read_from_mem::<u16>(mem.pal, (pal_index as u32) << 1) | (1 << 15);
                    frame.set_bg_pixel(x, line, color, bg as u8, priority);
                }
            }
        }
    }

    unsafe fn draw_bg_extended(frame: &mut Gpu2DFrame, mem: &Gpu2DMem, regs: &Gpu2DRenderRegs, from_line: usize, to_line: usize, bg: usize) {
        assert_unchecked(to_line <= DISPLAY_HEIGHT);
        assert_unchecked(from_line < to_line);
        assert_unchecked(bg == 2 || bg == 3);

        let disp_cnt = DispCnt::from(regs.disp_cnt(from_line));
        let bg_cnt = BgCnt::from(regs.bg_cnt(from_line, bg));
        let bg_cnt_raw = u16::from(bg_cnt);
        let priority = u8::from(bg_cnt.priority());
        let wrap = bg_cnt.ext_palette_slot_display_area_overflow();

        if bg_cnt.color_256_palettes() {
            // Bitmap (direct color or 256-color)
            let data_base = ((bg_cnt_raw as u32) << 6) & 0x7C000;
            let (size_x, size_y) = match u8::from(bg_cnt.screen_size()) {
                0 => (128i32, 128i32),
                1 => (256, 256),
                2 => (512, 256),
                _ => (512, 512),
            };
            let direct_color = bg_cnt_raw & (1 << 2) != 0;
            for line in from_line..to_line {
                let (internal_x, internal_y, pa, pc) = Self::affine_start(regs, bg, line);
                for x in 0..DISPLAY_WIDTH {
                    if !frame.can_display_pixel(x, line, bg as u8) {
                        continue;
                    }
                    let mut coord_x = internal_x.wrapping_add(x as i32 * pa) >> 8;
                    let mut coord_y = internal_y.wrapping_add(x as i32 * pc) >> 8;
                    if wrap {
                        coord_x &= size_x - 1;
                        coord_y &= size_y - 1;
                    } else if coord_x < 0 || coord_x >= size_x || coord_y < 0 || coord_y >= size_y {
                        continue;
                    }
                    let pixel_offset = coord_y as u32 * size_x as u32 + coord_x as u32;
                    if direct_color {
                        let color = utils::read_from_mem::<u16>(mem.bg, data_base + pixel_offset * 2);
                        if color & (1 << 15) != 0 {
                            frame.set_bg_pixel(x, line, color, bg as u8, priority);
                        }
                    } else {
                        let pal_index = utils::read_from_mem::<u8>(mem.bg, data_base + pixel_offset);
                        if pal_index != 0 {
                            let color = utils::read_from_mem::<u16>(mem.pal, (pal_index as u32) << 1) | (1 << 15);
                            frame.set_bg_pixel(x, line, color, bg as u8, priority);
                        }
                    }
                }
            }
        } else {
            // Extended affine (16-bit tile entries with flip and extended palettes)
            let screen_base = u32::from(disp_cnt.screen_base()) * 64 * 1024 + u32::from(bg_cnt.screen_base_block()) * 2 * 1024;
            let char_base = u32::from(disp_cnt.char_base()) * 64 * 1024 + u32::from(bg_cnt.char_base_block()) * 16 * 1024;
            let size = 128i32 << u8::from(bg_cnt.screen_size());
            let entries_width = (size / 8) as u32;
            let use_ext_pal = disp_cnt.bg_extended_palettes();
            let ext_pal_base = (bg * (vram::BG_EXT_PAL_SIZE / 4)) as u32;
            for line in from_line..to_line {
                let (internal_x, internal_y, pa, pc) = Self::affine_start(regs, bg, line);
                for x in 0..DISPLAY_WIDTH {
                    if !frame.can_display_pixel(x, line, bg as u8) {
                        continue;
                    }
                    let mut coord_x = internal_x.wrapping_add(x as i32 * pa) >> 8;
                    let mut coord_y = internal_y.wrapping_add(x as i32 * pc) >> 8;
                    if wrap {
                        coord_x &= size - 1;
                        coord_y &= size - 1;
                    } else if coord_x < 0 || coord_x >= size || coord_y < 0 || coord_y >= size {
                        continue;
                    }
                    let coord_x = coord_x as u32;
                    let coord_y = coord_y as u32;

                    let entry = utils::read_from_mem::<u16>(mem.bg, screen_base + ((coord_y >> 3) * entries_width + (coord_x >> 3)) * 2);
                    let x_in = if entry & (1 << 10) != 0 { 7 - (coord_x & 7) } else { coord_x & 7 };
                    let y_in = if entry & (1 << 11) != 0 { 7 - (coord_y & 7) } else { coord_y & 7 };
                    let pal_index = utils::read_from_mem::<u8>(mem.bg, char_base + (entry as u32 & 0x3FF) * 64 + y_in * 8 + x_in);
                    if pal_index != 0 {
                        let color = if use_ext_pal {
                            let pal_addr = ext_pal_base + (u32::from(entry >> 12) << 9) + ((pal_index as u32) << 1);
                            utils::read_from_mem::<u16>(mem.bg_ext_pal, pal_addr) | (1 << 15)
                        } else {
                            utils::read_from_mem::<u16>(mem.pal, (pal_index as u32) << 1) | (1 << 15)
                        };
                        frame.set_bg_pixel(x, line, color, bg as u8, priority);
                    }
                }
            }
        }
    }

    unsafe fn draw_bg_large(frame: &mut Gpu2DFrame, mem: &Gpu2DMem, regs: &Gpu2DRenderRegs, from_line: usize, to_line: usize, bg: usize) {
        assert_unchecked(to_line <= DISPLAY_HEIGHT);
        assert_unchecked(from_line < to_line);
        assert_unchecked(bg == 2);

        let bg_cnt = BgCnt::from(regs.bg_cnt(from_line, bg));
        let priority = u8::from(bg_cnt.priority());
        let wrap = bg_cnt.ext_palette_slot_display_area_overflow();
        let (size_x, size_y) = if u8::from(bg_cnt.screen_size()) != 0 { (1024i32, 512i32) } else { (512i32, 1024i32) };

        // A full large bitmap needs 512KB of VRAM; engine B only maps 128KB and wraps its rows.
        let vram_rows = (mem.bg.len() / size_x as usize) as i32;
        debug_assert!((vram_rows as u32).is_power_of_two());

        for line in from_line..to_line {
            let (internal_x, internal_y, pa, pc) = Self::affine_start(regs, bg, line);
            for x in 0..DISPLAY_WIDTH {
                if !frame.can_display_pixel(x, line, bg as u8) {
                    continue;
                }
                let mut coord_x = internal_x.wrapping_add(x as i32 * pa) >> 8;
                let mut coord_y = internal_y.wrapping_add(x as i32 * pc) >> 8;
                if wrap {
                    coord_x &= size_x - 1;
                    coord_y &= size_y - 1;
                } else if coord_x < 0 || coord_x >= size_x || coord_y < 0 || coord_y >= size_y {
                    continue;
                }
                coord_y &= vram_rows - 1;
                let pal_index = utils::read_from_mem::<u8>(mem.bg, (coord_y * size_x + coord_x) as u32);
                if pal_index != 0 {
                    let color = utils::read_from_mem::<u16>(mem.pal, (pal_index as u32) << 1) | (1 << 15);
                    frame.set_bg_pixel(x, line, color, bg as u8, priority);
                }
            }
        }
    }

    unsafe fn draw_bg_3d(frame: &mut Gpu2DFrame, regs: &Gpu2DRenderRegs, from_line: usize, to_line: usize) {
        assert_unchecked(to_line <= DISPLAY_HEIGHT);
        assert_unchecked(from_line < to_line);

        let bg_cnt = BgCnt::from(regs.bg_cnt(from_line, 0));

        for y in from_line..to_line {
            for x in 0..DISPLAY_WIDTH {
                if frame.can_display_pixel(x, y, 0) {
                    frame.set_3d_pixel(x, y, u8::from(bg_cnt.priority()));
                }
            }
        }
    }

    unsafe fn draw_bg<const MODE: BgMode>(frame: &mut Gpu2DFrame, mem: &Gpu2DMem, regs: &Gpu2DRenderRegs, from_line: usize, to_line: usize, bg: usize) {
        match MODE {
            BgMode::Text => Self::draw_bg_text(frame, mem, regs, from_line, to_line, bg),
            BgMode::Affine => Self::draw_bg_affine(frame, mem, regs, from_line, to_line, bg),
            BgMode::Extended => Self::draw_bg_extended(frame, mem, regs, from_line, to_line, bg),
            BgMode::Large => Self::draw_bg_large(frame, mem, regs, from_line, to_line, bg),
            BgMode::Display3d => Self::draw_bg_3d(frame, regs, from_line, to_line),
        }
    }

    unsafe fn draw_bgs(frame: &mut Gpu2DFrame, mem: &Gpu2DMem, regs: &Gpu2DRenderRegs, from_line: usize, to_line: usize) {
        assert_unchecked(to_line <= DISPLAY_HEIGHT);
        assert_unchecked(from_line < to_line);

        let disp_cnt = DispCnt::from(regs.disp_cnts[from_line]);

        let mut line = from_line;
        while line < to_line {
            let from_line = line;
            let from_bg_cnts = &regs.bg_cnts[from_line * 4..from_line * 4 + 4];
            let from_bg_ofs = &regs.bg_ubo.ofs[from_line * 4..from_line * 4 + 4];
            line += 1;
            while line < to_line {
                if from_bg_cnts != &regs.bg_cnts[line * 4..line * 4 + 4] || from_bg_ofs != &regs.bg_ubo.ofs[line * 4..line * 4 + 4] {
                    break;
                }
                line += 1;
            }

            macro_rules! draw {
                ($bg3mode:expr, $bg2mode:expr, $bg1mode:expr, $bg0mode:expr) => {{
                    if disp_cnt.screen_display_bg3() {
                        Self::draw_bg::<{ $bg3mode }>(frame, mem, regs, from_line, line, 3);
                    }
                    if disp_cnt.screen_display_bg2() {
                        Self::draw_bg::<{ $bg2mode }>(frame, mem, regs, from_line, line, 2);
                    }
                    if disp_cnt.screen_display_bg1() {
                        Self::draw_bg::<{ $bg1mode }>(frame, mem, regs, from_line, line, 1);
                    }
                    if disp_cnt.screen_display_bg0() {
                        if disp_cnt.bg0_3d() {
                            Self::draw_bg::<{ BgMode::Display3d }>(frame, mem, regs, from_line, line, 0);
                        } else {
                            Self::draw_bg::<{ $bg0mode }>(frame, mem, regs, from_line, line, 0);
                        }
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
                        Self::draw_bg::<{ BgMode::Large }>(frame, mem, regs, from_line, line, 2);
                    }
                }
                7 => {}
                _ => unreachable_unchecked(),
            }
        }
    }

    unsafe fn draw_object_normal<const WINDOW: bool>(
        frame: &mut Gpu2DFrame,
        mem: &Gpu2DMem,
        regs: &Gpu2DRenderRegs,
        from_line: usize,
        to_line: usize,
        oam: &OamAttribs,
        coords: (i16, i16),
        size_shifts: (u8, u8),
    ) {
        assert_unchecked(to_line <= DISPLAY_HEIGHT);
        assert_unchecked(from_line < to_line);

        let (x, y) = coords;
        let (width_shift, height_shift) = size_shifts;

        let disp_cnt = DispCnt::from(regs.disp_cnts[from_line]);

        let attrib0 = OamAttrib0::from(oam.attr0);
        let attrib1 = OamAttrib1::from(oam.attr1);
        let attrib2 = OamAttrib2::from(oam.attr2);

        let alpha = ((attrib0.get_gfx_mode() != OamGfxMode::AlphaBlending) as u16) << 15;
        let priority = u8::from(attrib2.priority());

        let boundry_shift = if disp_cnt.tile_1d_obj_mapping() { 5 + u8::from(disp_cnt.tile_obj_1d_boundary()) } else { 5 };
        let tile_base = u32::from(attrib2.tile_index()) << boundry_shift;

        let tile_count_x = 1usize << (width_shift - 3);
        let tile_count_y = 1usize << (height_shift - 3);

        if attrib0.is_8bit() {
            // In 2D mapping the object map is a fixed 128 pixel (16 tile) wide grid
            let map_width_shift = if disp_cnt.tile_1d_obj_mapping() { width_shift } else { 7 };

            let (pal, pal_bank_addr): (&[u8], u32) = if disp_cnt.obj_extended_palettes() {
                (mem.obj_ext_pal, u32::from(attrib2.pal_bank()) << 9)
            } else {
                (mem.pal, 0x200)
            };

            for y_tile_index in 0..tile_count_y {
                let map_index = if attrib1.v_flip() { tile_count_y - y_tile_index - 1 } else { y_tile_index };
                let tile_row_base = tile_base + ((map_index << (map_width_shift + 3)) as u32);
                for x_tile_index in 0..tile_count_x {
                    let map_index = if attrib1.h_flip() { tile_count_x - x_tile_index - 1 } else { x_tile_index };
                    let tile_addr = tile_row_base + map_index as u32 * 64;

                    for y_tile_offset in 0..8 {
                        let y_pixel = (y as usize).wrapping_add((y_tile_index << 3) + y_tile_offset);
                        if y_pixel < from_line || y_pixel >= to_line {
                            continue;
                        }

                        let row = if attrib1.v_flip() { 7 - y_tile_offset } else { y_tile_offset };
                        let row_addr = tile_addr + row as u32 * 8;
                        for x_tile_offset in 0..8 {
                            let x_pixel = (x as usize).wrapping_add((x_tile_index << 3) + x_tile_offset);
                            if x_pixel >= DISPLAY_WIDTH || (!WINDOW && !frame.can_display_pixel(x_pixel, y_pixel, 4)) {
                                continue;
                            }

                            let col = if attrib1.h_flip() { 7 - x_tile_offset } else { x_tile_offset };
                            let pal_index = utils::read_from_mem::<u8>(mem.obj, row_addr + col as u32);
                            if pal_index != 0 {
                                if WINDOW {
                                    frame.mark_obj_window(x_pixel, y_pixel);
                                } else {
                                    let color = (utils::read_from_mem::<u16>(pal, pal_bank_addr + ((pal_index as u32) << 1)) & !(1 << 15)) | alpha;
                                    frame.set_obj_pixel(x_pixel, y_pixel, color, priority);
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // In 2D mapping the object map is a fixed 256 pixel (32 tile) wide grid
            let map_width_shift = if disp_cnt.tile_1d_obj_mapping() { width_shift } else { 8 };

            let pal_addr = 0x200 + (u32::from(attrib2.pal_bank()) << 5);

            for y_tile_index in 0..tile_count_y {
                let map_index = if attrib1.v_flip() { tile_count_y - y_tile_index - 1 } else { y_tile_index };
                let map_width = (map_index << (map_width_shift + 2)) as u32;
                let tile_base = tile_base + map_width;
                for x_tile_index in 0..tile_count_x {
                    let map_index = if attrib1.h_flip() { tile_count_x - x_tile_index - 1 } else { x_tile_index };
                    let tile_base = tile_base + map_index as u32 * 32;

                    for y_tile_offset in 0..8 {
                        let y_pixel = (y as usize).wrapping_add((y_tile_index << 3) + y_tile_offset);
                        if y_pixel < from_line || y_pixel >= to_line {
                            continue;
                        }

                        let map_index = if attrib1.v_flip() { 7 - y_tile_offset } else { y_tile_offset };
                        let tile_addr = tile_base + map_index as u32 * 4;
                        let pal_indices = utils::read_from_mem::<u32>(mem.obj, tile_addr);
                        for x_tile_offset in 0..8 {
                            let x_pixel = (x as usize).wrapping_add((x_tile_index << 3) + x_tile_offset);
                            if x_pixel >= DISPLAY_WIDTH || (!WINDOW && !frame.can_display_pixel(x_pixel, y_pixel, 4)) {
                                continue;
                            }

                            let x_tile_offset = if attrib1.h_flip() { 7 - x_tile_offset } else { x_tile_offset };
                            let pal_index = (pal_indices >> (x_tile_offset * 4)) & 0xF;
                            if pal_index != 0 {
                                if WINDOW {
                                    frame.mark_obj_window(x_pixel, y_pixel);
                                } else {
                                    let color = (utils::read_from_mem::<u16>(mem.pal, pal_addr + pal_index * 2) & !(1 << 15)) | alpha;
                                    frame.set_obj_pixel(x_pixel, y_pixel, color, priority);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    unsafe fn draw_object_affine<const WINDOW: bool>(
        frame: &mut Gpu2DFrame,
        mem: &Gpu2DMem,
        regs: &Gpu2DRenderRegs,
        from_line: usize,
        to_line: usize,
        oam: &OamAttribs,
        coords: (i16, i16),
        sprite_size_shifts: (u8, u8),
        size_shifts: (u8, u8),
    ) {
        assert_unchecked(to_line <= DISPLAY_HEIGHT);
        assert_unchecked(from_line < to_line);

        let (x, y) = coords;
        let (width_shift, height_shift) = size_shifts;
        let width = 1 << width_shift;
        let height = 1 << height_shift;

        let (sprite_width_shift, sprite_height_shift) = sprite_size_shifts;
        let sprite_width = 1 << sprite_width_shift;
        let sprite_height = 1 << sprite_height_shift;

        let disp_cnt = DispCnt::from(regs.disp_cnts[from_line]);

        let attrib0 = OamAttrib0::from(oam.attr0);
        let attrib2 = OamAttrib2::from(oam.attr2);

        let alpha = ((attrib0.get_gfx_mode() != OamGfxMode::AlphaBlending) as u16) << 15;

        let boundry_shift = if disp_cnt.tile_1d_obj_mapping() { 5 + u8::from(disp_cnt.tile_obj_1d_boundary()) } else { 5 };
        let tile_base = u32::from(attrib2.tile_index()) << boundry_shift;

        let affine_addr = ((oam.attr1 >> 9) & 0xF) as usize * 0x20;
        let mat = [
            utils::read_from_mem::<u16>(mem.oam, affine_addr as u32 + 0x6) as i16,
            utils::read_from_mem::<u16>(mem.oam, affine_addr as u32 + 0xE) as i16,
            utils::read_from_mem::<u16>(mem.oam, affine_addr as u32 + 0x16) as i16,
            utils::read_from_mem::<u16>(mem.oam, affine_addr as u32 + 0x1E) as i16,
        ];

        let pal = if attrib0.is_8bit() {
            if disp_cnt.obj_extended_palettes() {
                let offset = u32::from(attrib2.pal_bank()) << 9;
                slice::from_raw_parts(mem.obj_ext_pal.as_ptr().add(offset as usize), 512)
            } else {
                slice::from_raw_parts(mem.pal.as_ptr().add(0x200), 512)
            }
        } else {
            let offset = u32::from(attrib2.pal_bank()) << 5;
            slice::from_raw_parts(mem.pal.as_ptr().add(0x200 + offset as usize), 32)
        };
        let map_width_shift = if disp_cnt.tile_1d_obj_mapping() {
            sprite_width_shift
        } else if attrib0.is_8bit() {
            7
        } else {
            8
        };

        let priority = u8::from(attrib2.priority());

        for y_sprite in 0..height {
            let y_pixel = (y as usize).wrapping_add(y_sprite as usize);
            if y_pixel < from_line || y_pixel >= to_line {
                continue;
            }

            let origin_y = y_sprite - height / 2;
            let y_column = [mat[1] as i32 * origin_y, mat[3] as i32 * origin_y];

            for x_sprite in 0..width {
                let x_pixel = (x as usize).wrapping_add(x_sprite as usize);
                if x_pixel >= DISPLAY_WIDTH || (!WINDOW && !frame.can_display_pixel(x_pixel, y_pixel, 4)) {
                    continue;
                }

                let origin_x = x_sprite - width / 2;
                let coords = [(origin_x * mat[0] as i32 + y_column[0]) >> 8, (origin_x * mat[2] as i32 + y_column[1]) >> 8];

                let x_tile = (coords[0] + sprite_width / 2) as u32;
                let y_tile = (coords[1] + sprite_height / 2) as u32;
                if x_tile >= sprite_width as u32 || y_tile >= sprite_height as u32 {
                    continue;
                }

                let tile_addr = (((y_tile >> 3) << map_width_shift) + (y_tile & 7)) * 8 + (x_tile >> 3) * 64 + (x_tile & 7);
                let tile_addr = tile_addr >> !attrib0.is_8bit() as u32;
                let mut pal_index = utils::read_from_mem::<u8>(mem.obj, tile_base + tile_addr);
                if !attrib0.is_8bit() {
                    pal_index = (pal_index >> ((x_tile & 1) << 2)) & 0xF;
                }
                if pal_index != 0 {
                    if WINDOW {
                        frame.mark_obj_window(x_pixel, y_pixel);
                    } else {
                        let color = (utils::read_from_mem::<u16>(pal, pal_index as u32 * 2) & !(1 << 15)) | alpha;
                        frame.set_obj_pixel(x_pixel, y_pixel, color, priority);
                    }
                }
            }
        }
    }

    // Bitmap objects hold direct RGB555 color. Like NooDS, they are drawn opaque (the per-object
    // alpha is not applied in the layer model); fully transparent objects (alpha 0) are skipped.
    unsafe fn draw_object_bitmap(
        frame: &mut Gpu2DFrame,
        mem: &Gpu2DMem,
        regs: &Gpu2DRenderRegs,
        from_line: usize,
        to_line: usize,
        oam: &OamAttribs,
        coords: (i16, i16),
        sprite_size_shifts: (u8, u8),
        size_shifts: (u8, u8),
    ) {
        assert_unchecked(to_line <= DISPLAY_HEIGHT);
        assert_unchecked(from_line < to_line);

        let (x, y) = coords;
        let disp_cnt = DispCnt::from(regs.disp_cnts[from_line]);
        let attrib0 = OamAttrib0::from(oam.attr0);
        let attrib1 = OamAttrib1::from(oam.attr1);
        let attrib2 = OamAttrib2::from(oam.attr2);
        let priority = u8::from(attrib2.priority());

        let (sprite_width_shift, sprite_height_shift) = sprite_size_shifts;
        let sprite_width = 1i32 << sprite_width_shift;
        let sprite_height = 1i32 << sprite_height_shift;

        // Determine the base address and stride of the bitmap
        let (data_base, bitmap_width) = if disp_cnt.bitmap_obj_mapping() {
            let boundary = if disp_cnt.bitmap_obj_1d_boundary() { 256u32 } else { 128 };
            (u32::from(attrib2.tile_index()) * boundary, sprite_width as u32)
        } else {
            let obj_2d = disp_cnt.bitmap_obj_2d();
            let x_mask: u32 = if obj_2d { 0x1F } else { 0x0F };
            let tile_index = u32::from(attrib2.tile_index());
            ((tile_index & x_mask) * 0x10 + (tile_index & !x_mask) * 0x80, if obj_2d { 256u32 } else { 128 })
        };

        if attrib0.get_obj_mode() == OamObjMode::Normal {
            for y_sprite in 0..sprite_height {
                let y_pixel = (y as i32).wrapping_add(y_sprite);
                if y_pixel < from_line as i32 || y_pixel >= to_line as i32 {
                    continue;
                }
                let y_pixel = y_pixel as usize;
                let sprite_row = if attrib1.v_flip() { sprite_height - y_sprite - 1 } else { y_sprite } as u32;
                let row_base = data_base + sprite_row * bitmap_width * 2;
                for x_sprite in 0..sprite_width {
                    let x_pixel = if attrib1.h_flip() { (x as i32) + sprite_width - x_sprite - 1 } else { (x as i32) + x_sprite };
                    if x_pixel < 0 || x_pixel >= DISPLAY_WIDTH as i32 {
                        continue;
                    }
                    let x_pixel = x_pixel as usize;
                    if !frame.can_display_pixel(x_pixel, y_pixel, 4) {
                        continue;
                    }
                    let pixel = utils::read_from_mem::<u16>(mem.obj, row_base + x_sprite as u32 * 2);
                    if pixel & (1 << 15) != 0 {
                        frame.set_obj_pixel(x_pixel, y_pixel, pixel, priority);
                    }
                }
            }
        } else {
            // Rotscale: iterate the (possibly doubled) bounding box and map back into the sprite
            let (width_shift, height_shift) = size_shifts;
            let width = 1i32 << width_shift;
            let height = 1i32 << height_shift;

            let affine_addr = ((oam.attr1 >> 9) & 0xF) as u32 * 0x20;
            let mat = [
                utils::read_from_mem::<u16>(mem.oam, affine_addr + 0x6) as i16 as i32,
                utils::read_from_mem::<u16>(mem.oam, affine_addr + 0xE) as i16 as i32,
                utils::read_from_mem::<u16>(mem.oam, affine_addr + 0x16) as i16 as i32,
                utils::read_from_mem::<u16>(mem.oam, affine_addr + 0x1E) as i16 as i32,
            ];

            for y_box in 0..height {
                let y_pixel = (y as i32).wrapping_add(y_box);
                if y_pixel < from_line as i32 || y_pixel >= to_line as i32 {
                    continue;
                }
                let y_pixel = y_pixel as usize;
                let origin_y = y_box - height / 2;
                for x_box in 0..width {
                    let x_pixel = (x as i32).wrapping_add(x_box);
                    if x_pixel < 0 || x_pixel >= DISPLAY_WIDTH as i32 {
                        continue;
                    }
                    let x_pixel = x_pixel as usize;
                    if !frame.can_display_pixel(x_pixel, y_pixel, 4) {
                        continue;
                    }
                    let origin_x = x_box - width / 2;
                    let tex_x = ((origin_x * mat[0] + origin_y * mat[1]) >> 8) + sprite_width / 2;
                    let tex_y = ((origin_x * mat[2] + origin_y * mat[3]) >> 8) + sprite_height / 2;
                    if tex_x < 0 || tex_x >= sprite_width || tex_y < 0 || tex_y >= sprite_height {
                        continue;
                    }
                    let pixel = utils::read_from_mem::<u16>(mem.obj, data_base + (tex_y as u32 * bitmap_width + tex_x as u32) * 2);
                    if pixel & (1 << 15) != 0 {
                        frame.set_obj_pixel(x_pixel, y_pixel, pixel, priority);
                    }
                }
            }
        }
    }

    unsafe fn draw_objects<const WINDOW: bool>(frame: &mut Gpu2DFrame, mem: &Gpu2DMem, regs: &Gpu2DRenderRegs, from_line: usize, to_line: usize) {
        assert_unchecked(to_line <= DISPLAY_HEIGHT);
        assert_unchecked(from_line < to_line);

        let disp_cnt = DispCnt::from(regs.disp_cnts[from_line]);
        if !disp_cnt.screen_display_obj() {
            return;
        }

        const OAM_COUNT: usize = regions::OAM_SIZE as usize / 2 / size_of::<OamAttribs>();
        let oams = slice::from_raw_parts(mem.oam.as_ptr() as *const OamAttribs, OAM_COUNT);

        for oam in oams {
            let attrib0 = OamAttrib0::from(oam.attr0);
            let obj_mode = attrib0.get_obj_mode();
            if obj_mode == OamObjMode::Disabled {
                continue;
            }

            // Window objects define the object window; all others are drawn normally
            let gfx_mode = attrib0.get_gfx_mode();
            if WINDOW {
                if gfx_mode != OamGfxMode::Window {
                    continue;
                }
            } else if gfx_mode == OamGfxMode::Window {
                continue;
            }

            if attrib0.is_mosaic() {
                // todo!();
            }

            let attrib1 = OamAttrib1::from(oam.attr1);
            let mut x = u16::from(attrib1.x()) as i16;
            if x >= DISPLAY_WIDTH as i16 {
                x -= 512;
            }
            let mut y = attrib0.y() as i16;
            if y >= DISPLAY_HEIGHT as i16 {
                y -= 256;
            }

            const SIZE_SHIFTS: [(u8, u8); 16] = [
                (3, 3),
                (4, 4),
                (5, 5),
                (6, 6),
                (4, 3),
                (5, 3),
                (5, 4),
                (6, 5),
                (3, 4),
                (3, 5),
                (4, 5),
                (5, 6),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
            ];

            let sprite_size_shifts = SIZE_SHIFTS[((u8::from(attrib0.shape()) << 2) | u8::from(attrib1.size())) as usize];
            let size_shift = u8::from(attrib0.obj_mode() >> 1) & 1;
            let size_shifts = (sprite_size_shifts.0 + size_shift, sprite_size_shifts.1 + size_shift);

            let (width_shift, height_shift) = size_shifts;
            let width = 1 << width_shift;
            let height = 1 << height_shift;
            if x >= DISPLAY_WIDTH as i16 || (x + width) < 0 || y >= to_line as i16 || (y + height) < from_line as i16 {
                continue;
            }

            if !WINDOW && gfx_mode == OamGfxMode::Bitmap {
                if u8::from(OamAttrib2::from(oam.attr2).pal_bank()) != 0 {
                    Self::draw_object_bitmap(frame, mem, regs, from_line, to_line, oam, (x, y), sprite_size_shifts, size_shifts);
                }
                continue;
            }

            match obj_mode {
                OamObjMode::Normal => Self::draw_object_normal::<WINDOW>(frame, mem, regs, from_line, to_line, oam, (x, y), size_shifts),
                OamObjMode::Affine | OamObjMode::AffineDouble => Self::draw_object_affine::<WINDOW>(frame, mem, regs, from_line, to_line, oam, (x, y), sprite_size_shifts, size_shifts),
                _ => unreachable_unchecked(),
            }
        }
    }

    // VRAM display mode: the engine outputs a raw RGB555 bitmap straight from an LCDC-mapped VRAM
    // bank, bypassing the layer/window/blend pipeline. Written as an opaque per-pixel backdrop
    // (layer 5, not a blend target) so the blend pass passes it through unchanged.
    unsafe fn draw_vram_display(frame: &mut Gpu2DFrame, mem: &Gpu2DMem, regs: &Gpu2DRenderRegs, from_line: usize, to_line: usize) {
        assert_unchecked(to_line <= DISPLAY_HEIGHT);
        assert_unchecked(from_line < to_line);

        let base = u8::from(DispCnt::from(regs.disp_cnts[from_line]).vram_block()) as u32 * 0x20000;
        for line in from_line..to_line {
            let row = base + (line * DISPLAY_WIDTH * 2) as u32;
            for x in 0..DISPLAY_WIDTH {
                let pixel = utils::read_from_mem::<u16>(mem.lcdc, row + (x * 2) as u32);
                let layer = FrameLayer::from(utils::rgba5_to_rgba8_non_norm(pixel));
                frame.layers[0][line * DISPLAY_WIDTH + x] = layer;
                frame.layers[1][line * DISPLAY_WIDTH + x] = layer;
            }
        }
    }

    unsafe fn draw(frame: &mut Gpu2DFrame, mem: Gpu2DMem, regs: &Gpu2DRenderRegs) {
        let backdrop = utils::read_from_mem::<u16>(mem.pal, 0) & !(1 << 15);
        frame.fill_pixels(backdrop);

        let mut line = 0;
        while line < DISPLAY_HEIGHT {
            let from_line = line;
            let from_disp_cnt = regs.disp_cnts[from_line];
            line += 1;
            while line < DISPLAY_HEIGHT {
                if from_disp_cnt != regs.disp_cnts[line] {
                    break;
                }
                line += 1;
            }

            let disp_cnt = DispCnt::from(from_disp_cnt);
            if u8::from(disp_cnt.display_mode()) == 2 {
                Self::draw_vram_display(frame, &mem, regs, from_line, line);
                continue;
            }
            if disp_cnt.screen_display_obj() && disp_cnt.obj_window_display_flag() {
                frame.reset_obj_window(from_line, line);
                Self::draw_objects::<true>(frame, &mem, regs, from_line, line);
            }
            Self::draw_windows(frame, regs, from_line, line);
            Self::draw_objects::<false>(frame, &mem, regs, from_line, line);
            Self::draw_bgs(frame, &mem, regs, from_line, line);
        }
    }

    unsafe fn blend(&self, frame: &mut Gpu2DFrame, regs: &Gpu2DRenderRegs, fb_tex_3d: GLuint) {
        for y in 0..DISPLAY_HEIGHT {
            for x in 0..DISPLAY_WIDTH {
                if !frame.can_display_pixel(x, y, 5) {
                    frame.layers[1][y * DISPLAY_HEIGHT + x].value |= 1 << 31;
                }
            }
        }

        frame.copy_textures();
        self.blend_program.draw(frame, regs, fb_tex_3d);
    }
}

pub struct Gpu2DSoftRenderer {
    frames: [Gpu2DFrame; 2],
    program: Gpu2DProgram,
}

impl Gpu2DSoftRenderer {
    pub fn new() -> Self {
        Gpu2DSoftRenderer {
            frames: array_init!({ Gpu2DFrame::new() }; 2),
            program: Gpu2DProgram::new(),
        }
    }

    pub unsafe fn draw<const ENGINE: Gpu2DEngine>(&mut self, mem_refs: &GpuMemRefs, regs: &Gpu2DRenderRegsShared) {
        match ENGINE {
            A => Gpu2DProgram::draw(&mut self.frames[0], Gpu2DMem::new::<{ A }>(mem_refs), &regs.regs_a[0]),
            B => Gpu2DProgram::draw(&mut self.frames[1], Gpu2DMem::new::<{ B }>(mem_refs), &regs.regs_b[0]),
        }
    }

    pub unsafe fn blend<const ENGINE: Gpu2DEngine>(&mut self, common: &GpuRendererCommon, regs: &Gpu2DRenderRegsShared, fb_tex_3d: GLuint) -> GLuint {
        match ENGINE {
            A => self.program.blend(&mut self.frames[0], &regs.regs_a[0], fb_tex_3d),
            B => self.program.blend(&mut self.frames[1], &regs.regs_b[0], 0),
        }
        self.frames[ENGINE as usize].fbo.color
    }
}
