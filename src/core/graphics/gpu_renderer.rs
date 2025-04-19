use crate::core::graphics::gl_glyph::GlGlyph;
use crate::core::graphics::gpu::{PowCnt1, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_2d::registers_2d::Gpu2DRegisters;
use crate::core::graphics::gpu_2d::renderer_2d::Gpu2DRenderer;
use crate::core::graphics::gpu_2d::Gpu2DEngine::{A, B};
use crate::core::graphics::gpu_3d::registers_3d::Gpu3DRegisters;
use crate::core::graphics::gpu_3d::renderer_3d::Gpu3DRenderer;
use crate::core::graphics::gpu_mem_buf::GpuMemBuf;
use crate::core::memory::mem::Memory;
use crate::presenter::{Presenter, PresenterScreen, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH, PRESENTER_SUB_BOTTOM_SCREEN, PRESENTER_SUB_TOP_SCREEN, PRESENTER_SUB_ROTATED_BOTTOM_SCREEN, PRESENTER_SUB_ROTATED_TOP_SCREEN, SETTINGS_ROTATE_SCREEN};
use crate::settings::Settings;
use gl::types::GLuint;
use std::intrinsics::unlikely;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

pub struct GpuRendererCommon {
    pub mem_buf: GpuMemBuf,
    pub pow_cnt1: PowCnt1,
}

impl GpuRendererCommon {
    fn new() -> Self {
        GpuRendererCommon {
            mem_buf: GpuMemBuf::default(),
            pow_cnt1: PowCnt1::from(0),
        }
    }
}

pub struct GpuRenderer {
    pub renderer_2d: Gpu2DRenderer,
    pub renderer_3d: Gpu3DRenderer,

    common: GpuRendererCommon,
    gl_glyph: GlGlyph,

    rendering: Mutex<bool>,
    rendering_condvar: Condvar,
    rendering_3d: bool,

    render_time_measure_count: u8,
    render_time_sum: u32,
    average_render_time: u16,
}

impl GpuRenderer {
    pub fn new() -> Self {
        GpuRenderer {
            renderer_2d: Gpu2DRenderer::new(),
            renderer_3d: Gpu3DRenderer::default(),

            common: GpuRendererCommon::new(),
            gl_glyph: GlGlyph::new(),

            rendering: Mutex::new(false),
            rendering_condvar: Condvar::new(),
            rendering_3d: false,

            render_time_measure_count: 0,
            render_time_sum: 0,
            average_render_time: 0,
        }
    }

    pub fn on_scanline(&mut self, inner_a: &mut Gpu2DRegisters, inner_b: &mut Gpu2DRegisters, line: u8) {
        self.renderer_2d.on_scanline(inner_a, inner_b, line);
    }

    pub fn on_scanline_finish(&mut self, mem: &mut Memory, pow_cnt1: PowCnt1, registers_3d: &mut Gpu3DRegisters) {
        let mut rendering = self.rendering.lock().unwrap();

        if !*rendering {
            self.common.pow_cnt1 = pow_cnt1;
            self.renderer_2d.on_scanline_finish();

            self.common.mem_buf.read_vram(&mut mem.vram);
            self.common.mem_buf.read_palettes_oam(mem);
            if self.renderer_3d.dirty {
                self.renderer_3d.finish_scanline(registers_3d);
                self.renderer_3d.dirty = false;
                self.rendering_3d = true;
            }

            *rendering = true;
            self.rendering_condvar.notify_one();
        }
    }

    pub fn reload_registers(&mut self) {
        self.renderer_2d.reload_registers();
    }

    pub fn render_loop(&mut self, presenter: &mut Presenter, fps: &Arc<AtomicU16>, last_save_time: &Arc<Mutex<Option<(Instant, bool)>>>, settings: &Settings) {
        {
            let rendering = self.rendering.lock().unwrap();
            let _drawing = self.rendering_condvar.wait_while(rendering, |rendering| !*rendering).unwrap();
        }

        let render_time_start = Instant::now();

        unsafe {
            SETTINGS_ROTATE_SCREEN = settings.rotate_screens();
            let used_sub_bottom_screen = if SETTINGS_ROTATE_SCREEN { PRESENTER_SUB_ROTATED_BOTTOM_SCREEN } else { PRESENTER_SUB_BOTTOM_SCREEN };
            let used_sub_top_screen = if SETTINGS_ROTATE_SCREEN { PRESENTER_SUB_ROTATED_TOP_SCREEN } else { PRESENTER_SUB_TOP_SCREEN };
            let used_fbo = if SETTINGS_ROTATE_SCREEN { self.renderer_2d.common.rotate_fbo.fbo } else { self.renderer_2d.common.blend_fbo.fbo };
            let src_x1 = if SETTINGS_ROTATE_SCREEN { DISPLAY_HEIGHT } else { DISPLAY_WIDTH };
            let src_y1 = if SETTINGS_ROTATE_SCREEN { DISPLAY_WIDTH } else { DISPLAY_HEIGHT };

            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
            gl::ClearColor(0f32, 0f32, 0f32, 1f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            if self.common.pow_cnt1.enable() {
                let blit_fb = |fbo: GLuint, screen: &PresenterScreen, src_x1: usize, src_y1: usize| {
                    gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
                    gl::BindFramebuffer(gl::READ_FRAMEBUFFER, fbo);
                    gl::BlitFramebuffer(
                        0,
                        0,
                        src_x1 as _,
                        src_y1 as _,
                        screen.x as _,
                        screen.y as _,
                        (screen.x + screen.width) as _,
                        (screen.y + screen.height) as _,
                        gl::COLOR_BUFFER_BIT,
                        gl::NEAREST,
                    );
                };


                self.common.mem_buf.rebuild_vram_maps();
                if self.rendering_3d {
                    self.rendering_3d = false;
                    self.common.mem_buf.read_3d();
                    self.renderer_3d.render(&self.common);
                }
                self.common.mem_buf.read_2d(self.renderer_2d.has_vram_display[0]);
                self.renderer_2d.render::<{ A }>(&self.common, self.renderer_3d.gl.fbo.color, SETTINGS_ROTATE_SCREEN);
                blit_fb(
                    used_fbo,
                    if self.common.pow_cnt1.display_swap() {
                        &used_sub_top_screen
                    } else {
                        &used_sub_bottom_screen
                    },
                    src_x1, src_y1
                );
                self.renderer_2d.render::<{ B }>(&self.common, 0, SETTINGS_ROTATE_SCREEN);
                blit_fb(
                    used_fbo,
                    if self.common.pow_cnt1.display_swap() {
                        &used_sub_bottom_screen
                    } else {
                        &used_sub_top_screen
                    },
                    src_x1, src_y1
                );
            }
        }

        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);

            let fps = fps.load(Ordering::Relaxed) as u32;
            let per = fps * 100 / 60;

            let last_time_saved = *last_save_time.lock().unwrap();
            let info_text = match last_time_saved {
                None => "",
                Some((last_time_saved, success)) => {
                    if Instant::now().duration_since(last_time_saved).as_secs() < 3 {
                        if success {
                            "Written to save file"
                        } else {
                            "Failed to save"
                        }
                    } else {
                        ""
                    }
                }
            };

            let arm7_emu: &str = settings.arm7_hle().into();
            self.gl_glyph.draw(format!("{}ms {arm7_emu}\n{per}% ({fps}fps)\n{info_text}", self.average_render_time));

            presenter.gl_swap_window();
        }

        let render_time_diff = Instant::now().duration_since(render_time_start);
        self.render_time_sum += render_time_diff.as_millis() as u32;
        self.render_time_measure_count += 1;
        if unlikely(self.render_time_measure_count == 30) {
            self.render_time_measure_count = 0;
            self.average_render_time = (self.render_time_sum / 30) as u16;
            self.render_time_sum = 0;
        }

        {
            let mut rendering = self.rendering.lock().unwrap();
            *rendering = false;
        }
    }
}
