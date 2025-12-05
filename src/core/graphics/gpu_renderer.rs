use crate::core::graphics::gl_glyph::GlGlyph;
use crate::core::graphics::gl_utils::{create_program, create_shader, shader_source, GpuFbo};
use crate::core::graphics::gpu::PowCnt1;
use crate::core::graphics::gpu_2d::registers_2d::Gpu2DRegisters;
use crate::core::graphics::gpu_2d::renderer_2d::Gpu2DRenderer;
use crate::core::graphics::gpu_2d::Gpu2DEngine::{A, B};
use crate::core::graphics::gpu_3d::registers_3d::Gpu3DRegisters;
use crate::core::graphics::gpu_3d::renderer_3d::Gpu3DRenderer;
use crate::core::graphics::gpu_mem_buf::GpuMemBuf;
use crate::core::memory::regions::{OAM_SIZE, STANDARD_PALETTES_SIZE};
use crate::core::memory::vram;
use crate::core::memory::vram::Vram;
use crate::presenter::{Presenter, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use crate::screen_layouts::ScreenLayout;
use crate::settings::Arm7Emu;
use gl::types::{GLint, GLuint};
use std::intrinsics::unlikely;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::thread::Thread;
use std::time::Instant;

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
    merge_program: GLuint,
    merge_alpha_uniform: GLint,
    final_fbo: GpuFbo,
    gl_glyph: GlGlyph,

    rendering: Mutex<bool>,
    rendering_condvar: Condvar,

    processed_3d: Mutex<bool>,
    processed_3d_condvar: Condvar,

    rendering_3d: bool,
    pause: bool,
    quit: AtomicBool,

    vram_read: AtomicBool,
    sample_2d: bool,
    ready_2d: bool,

    render_time_measure_count: u8,
    render_time_sum: u32,
    average_render_time: u32,

    read_vram: Mutex<()>,
    read_vram_condvar: Condvar,
}

impl GpuRenderer {
    pub fn new() -> Self {
        let (merge_program, merge_alpha_uniform) = unsafe {
            let vert_shader = create_shader("merge", shader_source!("merge_vert"), gl::VERTEX_SHADER).unwrap();
            let frag_shader = create_shader("merge", shader_source!("merge_frag"), gl::FRAGMENT_SHADER).unwrap();
            let program = create_program(&[vert_shader, frag_shader]).unwrap();
            gl::DeleteShader(vert_shader);
            gl::DeleteShader(frag_shader);

            gl::UseProgram(program);

            gl::BindAttribLocation(program, 0, c"position".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(program, c"tex".as_ptr() as _), 0);

            let merge_alpha_uniform = gl::GetUniformLocation(program, c"alpha".as_ptr() as _);

            gl::UseProgram(0);

            (program, merge_alpha_uniform)
        };

        GpuRenderer {
            renderer_2d: Gpu2DRenderer::new(),
            renderer_3d: Gpu3DRenderer::default(),

            common: GpuRendererCommon::new(),
            merge_program,
            merge_alpha_uniform,
            final_fbo: GpuFbo::new(PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _, false).unwrap(),
            gl_glyph: GlGlyph::new(),

            rendering: Mutex::new(false),
            rendering_condvar: Condvar::new(),

            processed_3d: Mutex::new(false),
            processed_3d_condvar: Condvar::new(),

            rendering_3d: false,
            pause: false,
            quit: AtomicBool::new(false),

            vram_read: AtomicBool::new(false),
            sample_2d: true,
            ready_2d: false,

            render_time_measure_count: 0,
            render_time_sum: 0,
            average_render_time: 0,

            read_vram: Mutex::new(()),
            read_vram_condvar: Condvar::new(),
        }
    }

    pub fn init(&mut self) {
        self.renderer_2d.init();
        self.renderer_3d.init();
        self.common.mem_buf.init();
        self.common.pow_cnt1[0] = PowCnt1::from(0);
        *self.processed_3d.lock().unwrap() = false;
        *self.rendering.lock().unwrap() = false;
        self.vram_read.store(false, Ordering::SeqCst);
        self.sample_2d = true;
        self.ready_2d = false;
        self.rendering_3d = false;
    }

    pub fn on_scanline(&mut self, inner_a: &mut Gpu2DRegisters, inner_b: &mut Gpu2DRegisters, line: u8) {
        if self.sample_2d {
            self.renderer_2d.on_scanline(inner_a, inner_b, line);
        }
    }

    pub fn on_scanline_finish(
        &mut self,
        palettes: &[u8; STANDARD_PALETTES_SIZE as usize],
        oam: &[u8; OAM_SIZE as usize],
        pow_cnt1: PowCnt1,
        registers_3d: &mut Gpu3DRegisters,
        breakout_imm: &mut bool,
    ) {
        if self.sample_2d {
            self.common.mem_buf.read_palettes_oam(palettes, oam);
            self.common.pow_cnt1[1] = pow_cnt1;
            self.sample_2d = false;
            self.ready_2d = true;

            let _guard = self.read_vram.lock().unwrap();
        }

        let mut rendering = self.rendering.lock().unwrap();

        if !*rendering && self.ready_2d {
            if unlikely(self.pause) {
                thread::park();
                if self.is_quit() {
                    *breakout_imm = true;
                    return;
                }
            }
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
            self.rendering_condvar.notify_all();
        }
    }

    pub fn reload_registers(&mut self, vram: &Vram) {
        if !self.ready_2d && self.vram_read.load(Ordering::SeqCst) {
            self.sample_2d = true;
        }

        if self.sample_2d {
            self.renderer_2d.reload_registers();
            self.common.mem_buf.set_vram_cnt(vram);
            self.read_vram_condvar.notify_one();
        }
    }

    unsafe fn merge_screens(&self, screens: [(GLuint, &[f32; 16], f32); 2], top_index: usize) {
        let bottom_index = (top_index + 1) & 1;
        let (top_fbo_color, top_vertices_coords, top_alpha) = screens[top_index];
        let (bottom_fbo_color, bottom_vertices_coords, bottom_alpha) = screens[bottom_index];
        if top_alpha < bottom_alpha {
            return self.merge_screens(screens, bottom_index);
        }

        gl::BindFramebuffer(gl::FRAMEBUFFER, self.final_fbo.fbo);
        gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);

        gl::UseProgram(self.merge_program);

        gl::Enable(gl::BLEND);
        gl::ActiveTexture(gl::TEXTURE0);

        gl::Uniform1f(self.merge_alpha_uniform, top_alpha);
        gl::BindTexture(gl::TEXTURE_2D, top_fbo_color);
        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 4, gl::FLOAT, gl::FALSE, 0, top_vertices_coords.as_ptr() as _);
        gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);

        gl::Uniform1f(self.merge_alpha_uniform, bottom_alpha);
        gl::BindTexture(gl::TEXTURE_2D, bottom_fbo_color);
        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 4, gl::FLOAT, gl::FALSE, 0, bottom_vertices_coords.as_ptr() as _);
        gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);

        gl::BindTexture(gl::TEXTURE_2D, 0);
        gl::Disable(gl::BLEND);
        gl::UseProgram(0);
    }

    pub fn render_loop(&mut self, presenter: &mut Presenter, fps: &Arc<AtomicU16>, last_save_time: &Arc<Mutex<Option<(Instant, bool)>>>, arm7_emu: Arm7Emu, screen_layout: &ScreenLayout, pause: bool) {
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
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.final_fbo.fbo);
            gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
            gl::ClearColor(0f32, 0f32, 0f32, 1f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            if self.common.pow_cnt1[0].enable() {
                self.renderer_2d.draw::<{ B }>(&self.common);
                let b_fbo_color = self.renderer_2d.blend::<{ B }>(&self.common, 0);

                self.renderer_2d.draw::<{ A }>(&self.common);

                if self.rendering_3d {
                    let processed_3d = self.processed_3d.lock().unwrap();
                    let _processed_3d = self.processed_3d_condvar.wait_while(processed_3d, |processed_3d| !*processed_3d).unwrap();
                    self.rendering_3d = false;
                    self.renderer_3d.render(&self.common);
                }

                let a_fbo_color = self.renderer_2d.blend::<{ A }>(&self.common, self.renderer_3d.gl.fbo.color);
                let top_screen = if self.common.pow_cnt1[0].display_swap() {
                    screen_layout.get_screen_top()
                } else {
                    screen_layout.get_screen_bottom()
                };
                let top_screen = (a_fbo_color, top_screen.0, top_screen.1);
                let bottom_screen = if self.common.pow_cnt1[0].display_swap() {
                    screen_layout.get_screen_bottom()
                } else {
                    screen_layout.get_screen_top()
                };
                let bottom_screen = (b_fbo_color, bottom_screen.0, bottom_screen.1);
                self.merge_screens([top_screen, bottom_screen], 0);
            }

            gl::BindFramebuffer(gl::FRAMEBUFFER, self.final_fbo.fbo);
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

            let arm7_emu: &str = arm7_emu.into();
            self.gl_glyph.draw(format!(
                "{}ms ({}fps) {arm7_emu}\n{per}% ({fps}/60)\n{info_text}",
                self.average_render_time / 1000,
                if self.average_render_time == 0 { 0 } else { 1000000 / self.average_render_time }
            ));

            if !pause {
                gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
                gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
                gl::ClearColor(0f32, 0f32, 0f32, 1f32);
                gl::Clear(gl::COLOR_BUFFER_BIT);
                self.blit_main_framebuffer();
                presenter.gl_swap_window();
            }

            {
                self.pause = pause;
                let mut rendering = self.rendering.lock().unwrap();
                *rendering = false;
            }

            {
                let mut processed_3d = self.processed_3d.lock().unwrap();
                *processed_3d = false;
                self.processed_3d_condvar.notify_one();
            }
        }

        let render_time_diff = Instant::now().duration_since(render_time_start);
        self.render_time_sum += render_time_diff.as_micros() as u32;
        self.render_time_measure_count += 1;
        if unlikely(self.render_time_measure_count == 30) {
            self.render_time_measure_count = 0;
            self.average_render_time = self.render_time_sum / 30;
            self.render_time_sum = 0;
        }
    }

    pub fn process_3d_loop(&mut self) {
        {
            let rendering = self.rendering.lock().unwrap();
            let _drawing = self.rendering_condvar.wait_while(rendering, |rendering| !*rendering).unwrap();
        }

        if self.is_quit() {
            return;
        }

        if self.common.pow_cnt1[0].enable() && self.rendering_3d {
            self.renderer_3d.process_polygons(&self.common);
        }

        {
            let mut processed_3d = self.processed_3d.lock().unwrap();
            *processed_3d = true;
            self.processed_3d_condvar.notify_one();
        }

        {
            let processed_3d = self.processed_3d.lock().unwrap();
            let _processed_3d = self.processed_3d_condvar.wait_while(processed_3d, |processed_3d| *processed_3d).unwrap();
        }
    }

    pub fn unpause(&mut self, cpu_thread: &Thread) {
        self.pause = false;
        cpu_thread.unpark();

        if self.is_quit() {
            *self.rendering.lock().unwrap() = true;
            *self.processed_3d.lock().unwrap() = false;
            self.rendering_condvar.notify_all();
            self.processed_3d_condvar.notify_one();
            self.read_vram_condvar.notify_one();
        }
    }

    pub fn blit_main_framebuffer(&self) {
        unsafe {
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, self.final_fbo.fbo);
            gl::BlitFramebuffer(
                0,
                0,
                PRESENTER_SCREEN_WIDTH as _,
                PRESENTER_SCREEN_HEIGHT as _,
                0,
                0,
                PRESENTER_SCREEN_WIDTH as _,
                PRESENTER_SCREEN_HEIGHT as _,
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );
        }
    }

    pub fn read_vram(&mut self, vram: &[u8; vram::TOTAL_SIZE]) {
        if self.is_quit() {
            return;
        }

        let read_vram = self.read_vram.lock().unwrap();
        let _read_vram = self.read_vram_condvar.wait(read_vram).unwrap();

        self.common.mem_buf.read_vram(vram);
    }

    pub fn is_quit(&self) -> bool {
        self.quit.load(Ordering::Relaxed)
    }

    pub fn set_quit(&self, value: bool) {
        self.quit.store(value, Ordering::Relaxed);
    }
}
