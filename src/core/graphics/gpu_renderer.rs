use crate::core::graphics::gl_glyph::GlGlyph;
use crate::core::graphics::gl_utils::GpuFbo;
use crate::core::graphics::gpu::{DispCapCnt, PowCnt1, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_2d::registers_2d::Gpu2DRegisters;
use crate::core::graphics::gpu_2d::renderer_2d::Gpu2DRenderer;
use crate::core::graphics::gpu_2d::renderer_regs_2d::Gpu2DRenderRegsShared;
use crate::core::graphics::gpu_2d::Gpu2DEngine::{A, B};
use crate::core::graphics::gpu_3d::registers_3d::Gpu3DRegisters;
use crate::core::graphics::gpu_3d::renderer_3d::Gpu3DRenderer;
use crate::core::graphics::gpu_mem_buf::{GpuMemBuf, GpuMemRefs};
use crate::core::graphics::gpu_shaders::GpuShadersPrograms;
use crate::core::memory::regions::{OAM_SIZE, STANDARD_PALETTES_SIZE};
use crate::core::memory::vram;
use crate::core::memory::vram::Vram;
use crate::presenter::{Presenter, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use crate::screen_layouts::ScreenLayout;
use crate::settings::Arm7Emu;
use crate::utils::HeapArrayU8;
use gl::types::{GLint, GLuint};
use std::intrinsics::unlikely;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::thread::Thread;
use std::time::{Duration, Instant};

pub struct GpuRendererCommon {
    pub mem_buf: GpuMemBuf,
    disp_cap_cnt: [DispCapCnt; 2],
    pub pow_cnt1: [PowCnt1; 2],
}

impl GpuRendererCommon {
    fn new() -> Self {
        GpuRendererCommon {
            mem_buf: GpuMemBuf::default(),
            disp_cap_cnt: [DispCapCnt::from(0), DispCapCnt::from(0)],
            pow_cnt1: [PowCnt1::from(0), PowCnt1::from(0)],
        }
    }
}

pub struct GpuRenderer {
    renderer_regs_2d_shared: Gpu2DRenderRegsShared,
    renderer_2d: Gpu2DRenderer,
    // renderer_soft_2d: Gpu2DSoftRenderer,
    pub renderer_3d: Gpu3DRenderer,
    gpu_mem_refs: GpuMemRefs,

    common: GpuRendererCommon,
    capture_program: GLuint,
    capture_size_scalers_uniform: GLint,
    capture_fbo: GpuFbo,
    capture_mem: HeapArrayU8<{ vram::BANK_A_SIZE * 4 }>,
    capture_query: GLuint,

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

    renderer_vram_busy: AtomicBool,
    sample_2d: bool,
    ready_2d: bool,

    render_time_measure_count: u8,
    render_time_sum: u32,
    average_render_time: u32,

    read_vram: Mutex<()>,
    read_vram_condvar: Condvar,
}

impl GpuRenderer {
    pub fn new(gpu_programs: &GpuShadersPrograms) -> Self {
        let (capture_size_scalers_uniform, capture_fbo_tex, capture_query) = unsafe {
            gl::UseProgram(gpu_programs.capture);

            gl::BindAttribLocation(gpu_programs.capture, 0, c"position".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(gpu_programs.capture, c"tex".as_ptr() as _), 0);
            gl::Uniform1i(gl::GetUniformLocation(gpu_programs.capture, c"texTest".as_ptr() as _), 1);

            let capture_size_scalers_uniform = gl::GetUniformLocation(gpu_programs.capture, c"sizeScalar".as_ptr() as _);

            gl::UseProgram(0);

            let mut tex = 0;
            gl::GenTextures(1, &mut tex);
            gl::BindTexture(gl::TEXTURE_2D, tex);
            #[cfg(target_os = "linux")]
            gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RG as _, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, 0, gl::RG, gl::UNSIGNED_BYTE, std::ptr::null());
            #[cfg(target_os = "vita")]
            Presenter::gl_tex_image_2d_rgba5(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);

            let mut query = 0;
            gl::GenQueries(1, &mut query);

            gl::BindTexture(gl::TEXTURE_2D, 0);

            (capture_size_scalers_uniform, tex, query)
        };

        let merge_alpha_uniform = unsafe {
            gl::UseProgram(gpu_programs.merge);

            gl::BindAttribLocation(gpu_programs.merge, 0, c"position".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(gpu_programs.merge, c"tex".as_ptr() as _), 0);

            let merge_alpha_uniform = gl::GetUniformLocation(gpu_programs.merge, c"alpha".as_ptr() as _);

            gl::UseProgram(0);

            merge_alpha_uniform
        };

        GpuRenderer {
            renderer_regs_2d_shared: Gpu2DRenderRegsShared::new(),
            renderer_2d: Gpu2DRenderer::new(gpu_programs),
            // renderer_soft_2d: Gpu2DSoftRenderer::new(),
            renderer_3d: Gpu3DRenderer::new(gpu_programs),
            gpu_mem_refs: GpuMemRefs::default(),

            common: GpuRendererCommon::new(),
            capture_program: gpu_programs.capture,
            capture_size_scalers_uniform,
            capture_fbo: GpuFbo::from_tex(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false, capture_fbo_tex).unwrap(),
            capture_mem: HeapArrayU8::default(),
            capture_query,

            merge_program: gpu_programs.merge,
            merge_alpha_uniform,
            final_fbo: GpuFbo::new(PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _, false).unwrap(),
            gl_glyph: GlGlyph::new(gpu_programs),

            rendering: Mutex::new(false),
            rendering_condvar: Condvar::new(),

            processed_3d: Mutex::new(false),
            processed_3d_condvar: Condvar::new(),

            rendering_3d: false,
            pause: false,
            quit: AtomicBool::new(false),

            renderer_vram_busy: AtomicBool::new(false),
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
        self.renderer_regs_2d_shared.init();
        self.renderer_3d.init();
        self.common.mem_buf.init();
        self.common.pow_cnt1[0] = PowCnt1::from(0);
        *self.processed_3d.lock().unwrap() = false;
        *self.rendering.lock().unwrap() = false;
        self.renderer_vram_busy.store(false, Ordering::SeqCst);
        self.sample_2d = true;
        self.ready_2d = false;
        self.rendering_3d = false;
    }

    pub fn on_scanline(&mut self, inner_a: &mut Gpu2DRegisters, inner_b: &mut Gpu2DRegisters, line: u8) {
        if self.sample_2d {
            self.renderer_regs_2d_shared.on_scanline(inner_a, inner_b, line);
        }
    }

    pub fn on_scanline_finish(
        &mut self,
        palettes: &[u8; STANDARD_PALETTES_SIZE as usize],
        oam: &[u8; OAM_SIZE as usize],
        pow_cnt1: PowCnt1,
        disp_cap_cnt: DispCapCnt,
        registers_3d: &mut Gpu3DRegisters,
        breakout_imm: &mut bool,
    ) {
        if self.sample_2d {
            self.common.mem_buf.read_palettes_oam(palettes, oam);
            self.common.pow_cnt1[1] = pow_cnt1;
            self.common.disp_cap_cnt[1] = disp_cap_cnt;
            self.ready_2d = true;
            self.sample_2d = false;
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
            self.common.disp_cap_cnt[0] = self.common.disp_cap_cnt[1];
            self.common.pow_cnt1[0] = self.common.pow_cnt1[1];
            self.common.mem_buf.use_queued_vram();
            self.renderer_regs_2d_shared.on_scanline_finish();

            if self.renderer_3d.dirty {
                self.renderer_3d.finish_scanline(registers_3d);
                self.renderer_3d.dirty = false;
                self.rendering_3d = true;
            }

            self.ready_2d = false;
            self.renderer_3d.on_render_start();
            self.renderer_vram_busy.store(true, Ordering::SeqCst);
            *rendering = true;
            self.rendering_condvar.notify_all();
        }
    }

    pub fn queued_disp_cap_cnt(&self) -> DispCapCnt {
        self.common.disp_cap_cnt[1]
    }

    pub fn reload_registers(&mut self, vram: &Vram) {
        if !self.ready_2d && !self.renderer_vram_busy.load(Ordering::SeqCst) {
            self.common.mem_buf.queue_vram(vram);
            self.read_vram_condvar.notify_one();
            self.renderer_regs_2d_shared.reload_registers();
            self.sample_2d = true;
        }
    }

    unsafe fn merge_screens(&self, screens: [(GLuint, &[f32; 16], f32); 2], top_index: usize) {
        let bottom_index = (top_index + 1) & 1;
        let (top_fbo_color, top_vertices_coords, top_alpha) = screens[top_index];
        let (bottom_fbo_color, bottom_vertices_coords, bottom_alpha) = screens[bottom_index];
        if top_alpha < bottom_alpha {
            return self.merge_screens(screens, bottom_index);
        }

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

        if self.rendering_3d {
            self.renderer_3d.set_tex_ptrs(&mut self.gpu_mem_refs);
        }
        self.renderer_2d.set_tex_ptrs(&mut self.gpu_mem_refs);

        let render_time_start = Instant::now();

        unsafe {
            let disp_cap_cnt = self.common.disp_cap_cnt[0];
            self.common.mem_buf.rebuild_vram_maps();
            self.common.mem_buf.insert_capture_mem(&self.capture_mem);
            self.common
                .mem_buf
                .read_all(&mut self.gpu_mem_refs, self.renderer_regs_2d_shared.has_vram_display[0], self.rendering_3d);

            if disp_cap_cnt.capture_enabled() && u8::from(disp_cap_cnt.capture_source()) != 0 {
                // todo!()
            }

            self.renderer_vram_busy.store(false, Ordering::SeqCst);

            gl::BindFramebuffer(gl::FRAMEBUFFER, self.final_fbo.fbo);
            gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
            gl::ClearColor(0f32, 0f32, 0f32, 1f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            self.renderer_2d.draw::<{ B }>(&self.gpu_mem_refs, &self.renderer_regs_2d_shared);
            let b_fbo_color = self.renderer_2d.blend::<{ B }>(&self.gpu_mem_refs, &self.renderer_regs_2d_shared, 0);

            // self.renderer_soft_2d.draw::<{ A }>(&self.common, &self.renderer_regs_2d_shared);
            self.renderer_2d.draw::<{ A }>(&self.gpu_mem_refs, &self.renderer_regs_2d_shared);

            // self.renderer_soft_2d.draw::<{ B }>(&self.common, &self.renderer_regs_2d_shared);
            // let b_fbo_color = self.renderer_soft_2d.blend::<{ B }>(&self.common, &self.renderer_regs_2d_shared, 0);

            if self.rendering_3d {
                let processed_3d = self.processed_3d.lock().unwrap();
                let (_processed_3d, timeout) = self
                    .processed_3d_condvar
                    .wait_timeout_while(processed_3d, Duration::from_millis(1000), |processed_3d| !*processed_3d)
                    .unwrap();
                if timeout.timed_out() {
                    println!("waiting for 3d processing timed out");
                }
                self.rendering_3d = false;
                self.renderer_3d.render(&self.common, &self.gpu_mem_refs);
            }

            // let a_fbo_color = self.renderer_soft_2d.blend::<{ A }>(&self.common, &self.renderer_regs_2d_shared, self.renderer_3d.gl.fbo.color);
            let a_fbo_color = self.renderer_2d.blend::<{ A }>(
                &self.gpu_mem_refs,
                &self.renderer_regs_2d_shared,
                self.renderer_3d.get_fbo(self.common.pow_cnt1[0].display_swap()).color,
            );

            if disp_cap_cnt.capture_enabled() && u8::from(disp_cap_cnt.capture_source()) != 1 {
                if u8::from(disp_cap_cnt.capture_size()) == 0 {
                    // todo!()
                }

                gl::BindFramebuffer(gl::FRAMEBUFFER, self.capture_fbo.fbo);
                gl::Viewport(0, 0, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _);
                gl::ClearColor(0.0, 0.0, 0.0, 0.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);

                gl::UseProgram(self.capture_program);

                gl::ActiveTexture(gl::TEXTURE0);
                gl::BindTexture(
                    gl::TEXTURE_2D,
                    if disp_cap_cnt.source_a() {
                        self.renderer_3d.get_fbo(self.common.pow_cnt1[0].display_swap()).color
                    } else {
                        a_fbo_color
                    },
                );

                const SIZE_SCALARS: [(f32, f32); 4] = [
                    (128.0 / 256.0, 128.0 / 192.0),
                    (256.0 / 256.0, 64.0 / 192.0),
                    (256.0 / 256.0, 128.0 / 192.0),
                    (256.0 / 256.0, 192.0 / 192.0),
                ];
                let scalars = SIZE_SCALARS[u8::from(disp_cap_cnt.capture_size()) as usize];
                gl::Uniform2f(self.capture_size_scalers_uniform, scalars.0, scalars.1);

                const COORDS: [f32; 4 * 4] = [-1f32, 1f32, 0f32, 0f32, 1f32, 1f32, 1f32, 0f32, 1f32, -1f32, 1f32, 1f32, -1f32, -1f32, 0f32, 1f32];

                gl::EnableVertexAttribArray(0);
                gl::VertexAttribPointer(0, 4, gl::FLOAT, gl::FALSE, 0, COORDS.as_ptr() as _);
                gl::BeginQuery(gl::ANY_SAMPLES_PASSED, self.capture_query);
                gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
                gl::EndQuery(gl::ANY_SAMPLES_PASSED);

                gl::BindTexture(gl::TEXTURE_2D, 0);
                gl::UseProgram(0);
            }

            gl::BindFramebuffer(gl::FRAMEBUFFER, self.final_fbo.fbo);
            gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
            gl::ClearColor(0.0, 0.0, 0.0, 0.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            if self.common.pow_cnt1[0].enable() {
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

            if disp_cap_cnt.capture_enabled() && u8::from(disp_cap_cnt.capture_source()) != 1 {
                let bank_num = u8::from(disp_cap_cnt.vram_write_block());
                let (width, height) = disp_cap_cnt.size();
                let width = width as usize;
                let height = height as usize;
                let offset = disp_cap_cnt.write_offset() as usize;

                let read_pixels_ptr = self.capture_mem.as_mut_ptr().add(bank_num as usize * vram::BANK_A_SIZE + offset);

                // Use query to wait for capture program to finish on vita
                let mut query_result = 0;
                gl::GetQueryObjectiv(self.capture_query, gl::QUERY_RESULT, &mut query_result);

                #[cfg(target_os = "linux")]
                {
                    gl::BindFramebuffer(gl::READ_FRAMEBUFFER, self.capture_fbo.fbo);
                    gl::ReadPixels(0, 0, width as _, height as _, gl::RG, gl::UNSIGNED_BYTE, read_pixels_ptr as _);
                }

                #[cfg(target_os = "vita")]
                {
                    use crate::presenter::Presenter;
                    use std::mem;
                    gl::BindTexture(gl::TEXTURE_2D, self.capture_fbo.color);
                    let fbo: &[u16; DISPLAY_WIDTH * DISPLAY_HEIGHT] = mem::transmute(Presenter::gl_get_tex_ptr());
                    let read_pixels_ptr: &mut [u16; DISPLAY_WIDTH * DISPLAY_HEIGHT] = mem::transmute(read_pixels_ptr);
                    if u8::from(disp_cap_cnt.capture_size()) == 0 {
                        for i in 0..height {
                            let capture_mem = &mut read_pixels_ptr[i * width..i * width + width];
                            let read_pixels = &fbo[i * DISPLAY_WIDTH..i * DISPLAY_WIDTH + width];
                            capture_mem.copy_from_slice(read_pixels);
                        }
                    } else {
                        read_pixels_ptr[..width * height].copy_from_slice(&fbo[..width * height]);
                    }
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
    }

    pub fn process_3d_loop(&mut self) {
        {
            let rendering = self.rendering.lock().unwrap();
            let _drawing = self.rendering_condvar.wait_while(rendering, |rendering| !*rendering).unwrap();
        }

        if self.is_quit() {
            return;
        }

        if self.rendering_3d {
            unsafe { self.renderer_3d.process_polygons(&self.common) };
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
