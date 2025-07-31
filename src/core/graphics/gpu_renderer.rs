use crate::core::graphics::gl_glyph::GlGlyph;
use crate::core::graphics::gpu::{PowCnt1, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_2d::registers_2d::Gpu2DRegisters;
use crate::core::graphics::gpu_2d::renderer_2d::Gpu2DRenderer;
use crate::core::graphics::gpu_2d::Gpu2DEngine::{A, B};
use crate::core::graphics::gpu_3d::registers_3d::Gpu3DRegisters;
use crate::core::graphics::gpu_3d::renderer_3d::Gpu3DRenderer;
use crate::core::graphics::gpu_mem_buf::GpuMemBuf;
use crate::core::memory::mem::Memory;
use crate::presenter::{Presenter, PresenterScreen, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH, PRESENTER_SUB_REGULAR, PRESENTER_SUB_RESIZED, PRESENTER_SUB_RESIZED_INV, PRESENTER_SUB_ROTATED};
use crate::settings::{ScreenMode, Settings};
use gl::types::GLuint;
use std::intrinsics::unlikely;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

pub struct ScreenTopology {
    pub top: PresenterScreen,
    pub bottom: PresenterScreen,
    pub mode: ScreenMode,
}

pub struct GpuRendererCommon {
    pub mem_buf: GpuMemBuf,
    pub pow_cnt1: [PowCnt1; 2],
}

impl GpuRendererCommon {
    fn new() -> Self {
        GpuRendererCommon {
            mem_buf: GpuMemBuf::default(),
            pow_cnt1: [PowCnt1::from(0), PowCnt1::from(0)],
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

    vram_read: AtomicBool,
    sample_2d: bool,
    ready_2d: bool,

    render_time_measure_count: u8,
    render_time_sum: u32,
    average_render_time: u16,

    #[cfg(feature = "profiling")]
    frame_capture: HeapMemU8<{ (PRESENTER_SCREEN_WIDTH * PRESENTER_SCREEN_HEIGHT * 4) as usize }>,
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

            vram_read: AtomicBool::new(false),
            sample_2d: true,
            ready_2d: false,

            render_time_measure_count: 0,
            render_time_sum: 0,
            average_render_time: 0,

            #[cfg(feature = "profiling")]
            frame_capture: HeapMemU8::new(),
        }
    }

    pub fn on_scanline(&mut self, inner_a: &mut Gpu2DRegisters, inner_b: &mut Gpu2DRegisters, line: u8) {
        if self.sample_2d {
            self.renderer_2d.on_scanline(inner_a, inner_b, line);
        }
    }

    pub fn on_scanline_finish(&mut self, mem: &mut Memory, pow_cnt1: PowCnt1, registers_3d: &mut Gpu3DRegisters) {
        if self.sample_2d {
            self.common.mem_buf.read_vram(&mut mem.vram);
            self.common.mem_buf.read_palettes_oam(mem);
            self.common.pow_cnt1[1] = pow_cnt1;
            self.sample_2d = false;
            self.ready_2d = true;
        }

        let mut rendering = self.rendering.lock().unwrap();

        if !*rendering && self.ready_2d {
            self.common.pow_cnt1[0] = self.common.pow_cnt1[1];
            self.renderer_2d.on_scanline_finish();

            if self.renderer_3d.dirty {
                self.renderer_3d.finish_scanline(registers_3d);
                self.renderer_3d.dirty = false;
                self.rendering_3d = true;
            }

            self.ready_2d = false;
            self.vram_read.store(false, Ordering::SeqCst);
            *rendering = true;
            self.rendering_condvar.notify_one();
        }
    }

    pub fn reload_registers(&mut self) {
        if !self.ready_2d && self.vram_read.load(Ordering::SeqCst) {
            self.sample_2d = true;
        }

        if self.sample_2d {
            self.renderer_2d.reload_registers();
        }
    }

    pub fn render_loop(&mut self, presenter: &mut Presenter, fps: &Arc<AtomicU16>, last_save_time: &Arc<Mutex<Option<(Instant, bool)>>>, settings: &Settings, swap_sizes: bool) {
        {
            let rendering = self.rendering.lock().unwrap();
            let _drawing = self.rendering_condvar.wait_while(rendering, |rendering| !*rendering).unwrap();
        }

        let render_time_start = Instant::now();

        if self.common.pow_cnt1[0].enable() {
            self.common.mem_buf.rebuild_vram_maps();
            if self.rendering_3d {
                self.common.mem_buf.read_3d();
            }
            self.common.mem_buf.read_2d(self.renderer_2d.has_vram_display[0]);
        }
        self.vram_read.store(true, Ordering::SeqCst);

        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
            gl::ClearColor(0f32, 0f32, 0f32, 1f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            if self.common.pow_cnt1[0].enable() {
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

                if self.rendering_3d {
                    self.rendering_3d = false;
                    self.renderer_3d.render(&self.common);
                }

                let screen_topology = match settings.screenmode() {
                    ScreenMode::Regular => PRESENTER_SUB_REGULAR,
                    ScreenMode::Rotated => PRESENTER_SUB_ROTATED,
                    ScreenMode::Resized => if swap_sizes { PRESENTER_SUB_RESIZED } else { PRESENTER_SUB_RESIZED_INV },
                };
                let used_fbo = match screen_topology.mode {
                    ScreenMode::Regular | ScreenMode::Resized => self.renderer_2d.common.blend_fbo.fbo,
                    ScreenMode::Rotated => self.renderer_2d.common.rotate_fbo.fbo,
                };
                let src_coords = match screen_topology.mode {
                    ScreenMode::Regular | ScreenMode::Resized => (DISPLAY_WIDTH, DISPLAY_HEIGHT),
                    ScreenMode::Rotated => (DISPLAY_HEIGHT, DISPLAY_WIDTH),
                };

                self.renderer_2d
                    .render::<{ A }>(&self.common, self.renderer_3d.gl.fbo.color, screen_topology.mode == ScreenMode::Rotated);
                blit_fb(
                    used_fbo,
                    if self.common.pow_cnt1[0].display_swap() { &screen_topology.top } else { &screen_topology.bottom },
                    src_coords.0,
                    src_coords.1,
                );
                self.renderer_2d.render::<{ B }>(&self.common, 0, screen_topology.mode == ScreenMode::Rotated);
                blit_fb(
                    used_fbo,
                    if self.common.pow_cnt1[0].display_swap() { &screen_topology.bottom } else { &screen_topology.top },
                    src_coords.0,
                    src_coords.1,
                );
            }

            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);

            let fps = fps.load(Ordering::Relaxed) as u32;
            let per = fps * 100 / 60;

            let last_time_saved = *last_save_time.lock().unwrap();
            let mut info_text = {
                #[cfg(target_os = "vita")]
                {
                    format!("CPU: {}MHz", vitasdk_sys::scePowerGetArmClockFrequency())
                }
                #[cfg(target_os = "linux")]
                "".to_string()
            };
            if let Some((last_time_saved, success)) = last_time_saved {
                if Instant::now().duration_since(last_time_saved).as_secs() < 3 {
                    if success {
                        info_text = "Written to save file".to_string();
                    } else {
                        info_text = "Failed to save".to_string();
                    }
                }
            }

            let arm7_emu: &str = settings.arm7_hle().into();
            self.gl_glyph.draw(format!("{}ms {arm7_emu}\n{per}% ({fps}fps)\n{info_text}", self.average_render_time));

            #[cfg(feature = "profiling")]
            gl::ReadPixels(
                0,
                0,
                PRESENTER_SCREEN_WIDTH as _,
                PRESENTER_SCREEN_HEIGHT as _,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                self.frame_capture.as_mut_ptr() as _,
            );

            presenter.gl_swap_window();

            {
                let mut rendering = self.rendering.lock().unwrap();
                *rendering = false;
            }

            #[cfg(feature = "profiling")]
            tracy_client::frame_image(self.frame_capture.as_ref(), PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _, 0, true);
        }

        let render_time_diff = Instant::now().duration_since(render_time_start);
        self.render_time_sum += render_time_diff.as_millis() as u32;
        self.render_time_measure_count += 1;
        if unlikely(self.render_time_measure_count == 30) {
            self.render_time_measure_count = 0;
            self.average_render_time = (self.render_time_sum / 30) as u16;
            self.render_time_sum = 0;
        }
    }
}
