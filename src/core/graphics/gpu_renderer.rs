use crate::core::graphics::gl_glyph::GlGlyph;
use crate::core::graphics::gl_utils::GpuFbo;
use crate::core::graphics::gpu::{DispCapCnt, PowCnt1, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_2d::registers_2d::Gpu2DRegisters;
use crate::core::graphics::gpu_2d::renderer_2d::Gpu2DRenderer;
use crate::core::graphics::gpu_2d::renderer_regs_2d::Gpu2DRenderRegsShared;
use crate::core::graphics::gpu_2d::renderer_soft_2d::Gpu2DSoftRenderer;
use crate::core::graphics::gpu_2d::Gpu2DEngine::{A, B};
use crate::core::graphics::gpu_3d::registers_3d::Gpu3DRegisters;
use crate::core::graphics::gpu_3d::renderer_3d::{Gpu3DRenderer, WidescreenOption};
use crate::core::graphics::gpu_mem_buf::{GpuMemBuf, GpuMemRefs};
use crate::core::graphics::gpu_shaders::GpuShadersPrograms;
use crate::core::memory::regions::{OAM_SIZE, STANDARD_PALETTES_SIZE};
use crate::core::memory::vram;
use crate::core::memory::vram::{Vram, VramBanks};
use crate::logging::info_println;
use crate::presenter::{Presenter, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use crate::ra_context::RaContext;
use crate::screen_layouts::ScreenLayout;
use crate::screen_overlays;
use crate::settings::Settings;
use crate::utils::HeapArrayU8;
use gl::types::{GLint, GLuint};
use glyph_brush::{HorizontalAlign, Layout, VerticalAlign};
use png::{BitDepth, ColorType};
use std::intrinsics::unlikely;
use std::ops::Deref;
use std::path::PathBuf;
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

#[derive(Default)]
struct GpuStreamInner {
    sample_to: usize,
    sample_from: usize,
    ptrs: [*const u8; 2],
}

struct GpuStream {
    fbo: [GpuFbo; 2],
    query: GLuint,
    mutex: Mutex<GpuStreamInner>,
    ready_condvar: Condvar,
}

#[cfg(target_os = "vita")]
impl GpuStream {
    fn new() -> GpuStream {
        GpuStream {
            fbo: crate::utils::array_init!({ unsafe {
                let mut tex = 0;
                gl::GenTextures(1, &mut tex);
                gl::BindTexture(gl::TEXTURE_2D, tex);
                Presenter::gl_tex_image_2d_rgba5(PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
                GpuFbo::from_tex(PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _, false, false, tex).unwrap()
            }}; 2),
            query: unsafe {
                let mut query = 0;
                gl::GenQueries(1, &mut query);
                query
            },
            mutex: Mutex::new(GpuStreamInner::default()),
            ready_condvar: Condvar::new(),
        }
    }

    fn init(&mut self) {
        *self.mutex.lock().unwrap() = GpuStreamInner::default();
    }
}

// DSVITA_2D_TRACE=1 (debug builds): once a second, print the 2d sampling cadence —
// the isolation counters for the frozen-table class of bugs.
#[derive(Copy, Clone)]
enum Trace2D {
    ScanlineCall,
    ScanlineSampled,
    Snapshot,
    Handoff,
    ReloadArmed,
    ReloadSkipBusy,
    ReloadSkipReady,
}

fn trace_2d(what: Trace2D) {
    use std::sync::atomic::AtomicU32;
    if !crate::IS_DEBUG {
        return;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ENABLED.get_or_init(|| std::env::var("DSVITA_2D_TRACE").is_ok_and(|v| v == "1")) {
        return;
    }
    static COUNTS: [AtomicU32; 7] = [const { AtomicU32::new(0) }; 7];
    COUNTS[what as usize].fetch_add(1, Ordering::Relaxed);
    if matches!(what, Trace2D::ScanlineCall) {
        static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
        let mut last = LAST.lock().unwrap();
        let now = std::time::Instant::now();
        if last.is_none_or(|t| now.duration_since(t).as_secs() >= 1) {
            *last = Some(now);
            let c: Vec<u32> = COUNTS.iter().map(|c| c.swap(0, Ordering::Relaxed)).collect();
            eprintln!(
                "2DTRACE lines={} sampled={} snapshots={} handoffs={} reload_armed={} skip_busy={} skip_ready={}",
                c[0], c[1], c[2], c[3], c[4], c[5], c[6]
            );
        }
    }
}

pub struct GpuRenderer {
    renderer_regs_2d_shared: Gpu2DRenderRegsShared,
    renderer_2d: Gpu2DRenderer,
    // DSVITA_SOFT_2D=1 (debug builds): scanline-software 2D instead of the GL renderer —
    // the isolation tool for the a64 2D-corruption class of bugs (GL-side vs guest-side).
    renderer_soft_2d: Option<Gpu2DSoftRenderer>,
    pub renderer_3d: Gpu3DRenderer,
    gpu_mem_refs: GpuMemRefs,

    pub common: GpuRendererCommon,
    capture_program: GLuint,
    capture_size_scalers_uniform: GLint,
    capture_fbo: GpuFbo,
    capture_mem: HeapArrayU8<{ vram::BANK_A_SIZE * 4 }>,
    capture_query: GLuint,

    #[cfg(target_os = "vita")]
    stream: GpuStream,
    stream_top_screen: bool,

    merge_program: GLuint,
    merge_width_coefficient_uniform: GLint,
    merge_alpha_uniform: GLint,

    ra_program: GLuint,
    ra_alpha_loc: GLint,
    ra_last_event_instant: Instant,
    ra_badge_texture: Option<GLuint>,

    overlay_program: GLuint,
    // Cached overlay: the resolved PNG path it was loaded from + its GL texture.
    // Reloaded when the active layout selects a different (valid) overlay.
    overlay_texture: Option<(PathBuf, GLuint)>,

    gl_glyph: GlGlyph,
    final_fbo: GpuFbo,

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

    // The Vita presents at a fixed 960x544 and keeps those dimensions hardcoded (below);
    // only the other platforms need a parameterized present rect (Android letterboxes).
    #[cfg(not(target_os = "vita"))]
    present_rect: (i32, i32, i32, i32),
    #[cfg(not(target_os = "vita"))]
    present_surface: (i32, i32),
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
            #[cfg(not(target_os = "vita"))]
            gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RG8 as _, DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, 0, gl::RG, gl::UNSIGNED_BYTE, std::ptr::null());
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

        let (merge_width_coefficient_uniform, merge_alpha_uniform) = unsafe {
            gl::UseProgram(gpu_programs.merge);

            gl::BindAttribLocation(gpu_programs.merge, 0, c"position".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(gpu_programs.merge, c"tex".as_ptr() as _), 0);

            let merge_width_coefficient_uniform = gl::GetUniformLocation(gpu_programs.merge, c"widthCoefficient".as_ptr() as _);
            let merge_alpha_uniform = gl::GetUniformLocation(gpu_programs.merge, c"alpha".as_ptr() as _);

            (merge_width_coefficient_uniform, merge_alpha_uniform)
        };

        let ra_alpha_loc = unsafe {
            gl::UseProgram(gpu_programs.ra);

            gl::BindAttribLocation(gpu_programs.ra, 0, c"position".as_ptr() as _);
            gl::BindAttribLocation(gpu_programs.ra, 1, c"texCoordsColor".as_ptr() as _);

            gl::Uniform1i(gl::GetUniformLocation(gpu_programs.ra, c"tex".as_ptr() as _), 0);

            let alpha_loc = gl::GetUniformLocation(gpu_programs.ra, c"alpha".as_ptr() as _);

            gl::UseProgram(0);

            alpha_loc
        };

        unsafe {
            gl::UseProgram(gpu_programs.overlay);
            gl::BindAttribLocation(gpu_programs.overlay, 0, c"position".as_ptr() as _);
            gl::BindAttribLocation(gpu_programs.overlay, 1, c"texCoordsColor".as_ptr() as _);
            gl::Uniform1i(gl::GetUniformLocation(gpu_programs.overlay, c"tex".as_ptr() as _), 0);
            gl::UseProgram(0);
        }

        GpuRenderer {
            renderer_regs_2d_shared: Gpu2DRenderRegsShared::new(),
            renderer_2d: Gpu2DRenderer::new(gpu_programs),
            renderer_soft_2d: (crate::IS_DEBUG && std::env::var("DSVITA_SOFT_2D").is_ok_and(|v| v == "1")).then(Gpu2DSoftRenderer::new),
            renderer_3d: Gpu3DRenderer::new(gpu_programs),
            gpu_mem_refs: GpuMemRefs::default(),

            common: GpuRendererCommon::new(),
            capture_program: gpu_programs.capture,
            capture_size_scalers_uniform,
            capture_fbo: GpuFbo::from_tex(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _, false, false, capture_fbo_tex).unwrap(),
            capture_mem: HeapArrayU8::default(),
            capture_query,

            #[cfg(target_os = "vita")]
            stream: GpuStream::new(),
            stream_top_screen: false,

            merge_program: gpu_programs.merge,
            merge_width_coefficient_uniform,
            merge_alpha_uniform,

            ra_program: gpu_programs.ra,
            ra_alpha_loc,
            ra_last_event_instant: Instant::now(),
            ra_badge_texture: None,

            overlay_program: gpu_programs.overlay,
            overlay_texture: None,

            gl_glyph: GlGlyph::new(gpu_programs),
            final_fbo: GpuFbo::new(PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _, false, false).unwrap(),

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

            #[cfg(not(target_os = "vita"))]
            present_rect: (0, 0, PRESENTER_SCREEN_WIDTH as i32, PRESENTER_SCREEN_HEIGHT as i32),
            #[cfg(not(target_os = "vita"))]
            present_surface: (PRESENTER_SCREEN_WIDTH as i32, PRESENTER_SCREEN_HEIGHT as i32),
        }
    }

    pub fn init(&mut self, stream_top_screen: bool) {
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
        #[cfg(target_os = "vita")]
        self.stream.init();
        self.stream_top_screen = stream_top_screen;
    }

    pub fn on_scanline(&mut self, inner_a: &mut Gpu2DRegisters, inner_b: &mut Gpu2DRegisters, line: u8) {
        trace_2d(Trace2D::ScanlineCall);
        if self.sample_2d {
            trace_2d(Trace2D::ScanlineSampled);
            self.renderer_regs_2d_shared.on_scanline(inner_a, inner_b, line);
        }
    }

    pub fn on_scanline_finish(
        &mut self,
        vram_banks: &mut VramBanks,
        palettes: &[u8; STANDARD_PALETTES_SIZE as usize],
        oam: &[u8; OAM_SIZE as usize],
        pow_cnt1: PowCnt1,
        disp_cap_cnt: DispCapCnt,
        registers_3d: &mut Gpu3DRegisters,
        sync_3d: bool,
        breakout_imm: &mut bool,
    ) {
        if self.sample_2d {
            trace_2d(Trace2D::Snapshot);
            self.common.mem_buf.read_vram(vram_banks);
            self.common.mem_buf.read_palettes_oam(palettes, oam);
            self.common.pow_cnt1[1] = pow_cnt1;
            self.common.disp_cap_cnt[1] = disp_cap_cnt;
            self.ready_2d = true;
            self.sample_2d = sync_3d;
        }

        let mut rendering = self.rendering.lock().unwrap();

        if !*rendering && self.ready_2d {
            trace_2d(Trace2D::Handoff);
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
            self.sample_2d = false;
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
        trace_2d(if self.ready_2d {
            Trace2D::ReloadSkipReady
        } else if self.renderer_vram_busy.load(Ordering::SeqCst) {
            Trace2D::ReloadSkipBusy
        } else {
            Trace2D::ReloadArmed
        });
        if !self.ready_2d && !self.renderer_vram_busy.load(Ordering::SeqCst) {
            self.common.mem_buf.queue_vram(vram);
            self.renderer_regs_2d_shared.reload_registers();
            self.sample_2d = true;
        }
    }

    unsafe fn merge_screens(&self, screens: [(GLuint, &[f32; 16], f32, f32); 2], top_index: usize) {
        let bottom_index = (top_index + 1) & 1;
        let (top_fbo_color, top_vertices_coords, top_wide_screen_coefficient, top_alpha) = screens[top_index];
        let (bottom_fbo_color, bottom_vertices_coords, bottom_wide_screen_coefficient, bottom_alpha) = screens[bottom_index];
        if top_alpha < bottom_alpha {
            return self.merge_screens(screens, bottom_index);
        }

        gl::UseProgram(self.merge_program);

        gl::Enable(gl::BLEND);
        gl::ActiveTexture(gl::TEXTURE0);

        gl::Uniform1f(self.merge_width_coefficient_uniform, top_wide_screen_coefficient);
        gl::Uniform1f(self.merge_alpha_uniform, top_alpha);
        gl::BindTexture(gl::TEXTURE_2D, top_fbo_color);
        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 4, gl::FLOAT, gl::FALSE, 0, top_vertices_coords.as_ptr() as _);
        gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);

        gl::Uniform1f(self.merge_width_coefficient_uniform, bottom_wide_screen_coefficient);
        gl::Uniform1f(self.merge_alpha_uniform, bottom_alpha);
        gl::BindTexture(gl::TEXTURE_2D, bottom_fbo_color);
        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 4, gl::FLOAT, gl::FALSE, 0, bottom_vertices_coords.as_ptr() as _);
        gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);

        gl::BindTexture(gl::TEXTURE_2D, 0);
        gl::Disable(gl::BLEND);
        gl::UseProgram(0);
    }

    #[inline(never)]
    unsafe fn draw_overlay(&mut self, screen_layout: &ScreenLayout) {
        let path = screen_layout.overlay_path.as_ref().unwrap();

        let stale = self.overlay_texture.as_ref().map(|(cached, _)| cached != path).unwrap_or(true);
        if stale {
            if let Some((_, tex)) = self.overlay_texture.take() {
                gl::DeleteTextures(1, &tex);
            }
            if let Some(overlay) = screen_overlays::load(path) {
                let mut tex = 0;
                gl::GenTextures(1, &mut tex);
                gl::BindTexture(gl::TEXTURE_2D, tex);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::RGBA as _,
                    overlay.width as _,
                    overlay.height as _,
                    0,
                    gl::RGBA,
                    gl::UNSIGNED_BYTE,
                    overlay.data.as_ptr() as _,
                );
                gl::BindTexture(gl::TEXTURE_2D, 0);
                self.overlay_texture = Some((path.clone(), tex));
            }
        }

        let Some((_, tex)) = self.overlay_texture.as_ref() else {
            return;
        };

        gl::UseProgram(self.overlay_program);
        gl::Enable(gl::BLEND);
        gl::BlendEquation(gl::FUNC_ADD);
        gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindTexture(gl::TEXTURE_2D, *tex);

        #[rustfmt::skip]
        const POSITION: [f32; 4 * 2] = [
            -1.0,  1.0,
             1.0,  1.0,
             1.0, -1.0,
            -1.0, -1.0,
        ];
        // texFactor (third component) is unused by overlay_frag; only the ra
        // vertex shader's xy texCoords matter.
        #[rustfmt::skip]
        const TEX_COORDS: [f32; 4 * 3] = [
            0.0, 0.0, 0.0,
            1.0, 0.0, 0.0,
            1.0, 1.0, 0.0,
            0.0, 1.0, 0.0,
        ];
        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, POSITION.as_ptr() as _);
        gl::EnableVertexAttribArray(1);
        gl::VertexAttribPointer(1, 3, gl::FLOAT, gl::FALSE, 0, TEX_COORDS.as_ptr() as _);
        gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);

        gl::BindTexture(gl::TEXTURE_2D, 0);
        gl::Disable(gl::BLEND);
        gl::UseProgram(0);
    }

    #[inline(never)]
    unsafe fn show_retroachievements(&mut self, ra_context: &RaContext) {
        let ra_event = ra_context.event.lock().unwrap();
        let (event, instant) = ra_event.deref();
        let elapsed = instant.elapsed();
        const EVENT_DURATION_SECS: u8 = 5;
        if elapsed <= Duration::from_secs(EVENT_DURATION_SECS as _) && !event.title.is_empty() {
            if self.ra_last_event_instant != *instant {
                if let Some(texture) = self.ra_badge_texture {
                    gl::DeleteTextures(1, &texture);
                    self.ra_badge_texture = None;
                }

                if let Some((badge_data, info)) = &event.badge {
                    if info.bit_depth == BitDepth::Eight {
                        match info.color_type {
                            ColorType::Rgb | ColorType::Rgba => {
                                let format = if info.color_type == ColorType::Rgb { gl::RGB } else { gl::RGBA };
                                let mut texture = 0;
                                gl::GenTextures(1, &mut texture);
                                gl::BindTexture(gl::TEXTURE_2D, texture);
                                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
                                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
                                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
                                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
                                gl::TexImage2D(
                                    gl::TEXTURE_2D,
                                    0,
                                    format as _,
                                    info.width as _,
                                    info.height as _,
                                    0,
                                    format,
                                    gl::UNSIGNED_BYTE,
                                    badge_data.as_ptr() as _,
                                );
                                self.ra_badge_texture = Some(texture);
                            }
                            _ => {}
                        }
                    }
                }

                self.ra_last_event_instant = *instant;
            }

            let elapsed_ms = elapsed.as_millis() as u16 as f32;
            let alpha = 1.0 - (elapsed_ms - (EVENT_DURATION_SECS as f32 * 1000.0 / 2.0)).abs() / (EVENT_DURATION_SECS as f32 * 1000.0 / 2.0);

            const OFFSET_X: u32 = 0;
            const OFFSET_Y: u32 = 416;
            const WIDTH: u32 = 500;
            const HEIGHT: u32 = PRESENTER_SCREEN_HEIGHT - OFFSET_Y;
            gl::Viewport(OFFSET_X as _, OFFSET_Y as _, WIDTH as _, HEIGHT as _);

            gl::Enable(gl::BLEND);
            gl::BlendEquation(gl::FUNC_ADD);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            gl::UseProgram(self.ra_program);

            gl::Uniform1f(self.ra_alpha_loc, alpha);

            const BADGE_NORMALIZED_WIDTH: f32 = HEIGHT as f32 / WIDTH as f32 * 2.0;
            #[rustfmt::skip]
                    const VERTICES: [f32; 8 * 2] = [
                        -1.0, 1.0,
                        -1.0 + BADGE_NORMALIZED_WIDTH, 1.0,
                        -1.0 + BADGE_NORMALIZED_WIDTH, -1.0,
                        -1.0, -1.0,

                        -1.0 + BADGE_NORMALIZED_WIDTH, 1.0,
                        1.0, 1.0,
                        1.0, -1.0,
                        -1.0 + BADGE_NORMALIZED_WIDTH, -1.0,
                    ];

            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, VERTICES.as_ptr() as _);

            let tex_factor = match self.ra_badge_texture {
                None => 0.0,
                Some(tex) => {
                    gl::ActiveTexture(gl::TEXTURE0);
                    gl::BindTexture(gl::TEXTURE_2D, tex);
                    1.0
                }
            };

            #[rustfmt::skip]
                    let tex_coords_factor: [f32; 8 * 3] = [
                        0.0, 0.0, tex_factor,
                        1.0, 0.0, tex_factor,
                        1.0, 1.0, tex_factor,
                        0.0, 1.0, tex_factor,

                        0.0, 0.0, 0.0,
                        0.0, 0.0, 0.0,
                        0.0, 0.0, 0.0,
                        0.0, 0.0, 0.0,
                    ];

            gl::EnableVertexAttribArray(1);
            gl::VertexAttribPointer(1, 3, gl::FLOAT, gl::FALSE, 0, tex_coords_factor.as_ptr() as _);

            const INDICES: [u16; 12] = [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
            gl::DrawElements(gl::TRIANGLES, INDICES.len() as _, gl::UNSIGNED_SHORT, INDICES.as_ptr() as _);

            self.gl_glyph.draw(
                format!("{}\n{}", event.title, event.description),
                (WIDTH as f32, HEIGHT as f32),
                (-200.0, 0.0),
                40.0,
                Layout::default().h_align(HorizontalAlign::Left).v_align(VerticalAlign::Center),
                alpha,
            );

            gl::Disable(gl::BLEND);
        }
    }

    pub fn render_loop(
        &mut self,
        presenter: &mut Presenter,
        fps: &Arc<AtomicU16>,
        last_save_time: &Arc<Mutex<Option<(Instant, bool)>>>,
        screen_layout: &ScreenLayout,
        ra_context: &RaContext,
        settings: &Settings,
        pause: bool,
    ) {
        let upscale_3d_factor_index = settings.upscale_3d_factor();
        let mut widescreen = settings.widescreen();
        let widescreen_coefficient = if widescreen != WidescreenOption::Off {
            if screen_layout.wide_screen_coefficient == 1.0 {
                widescreen = WidescreenOption::Off
            }
            screen_layout.wide_screen_coefficient
        } else {
            1.0
        };

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
            if self.rendering_3d {
                self.renderer_3d.on_vram_ready();
            }

            if disp_cap_cnt.capture_enabled() && u8::from(disp_cap_cnt.capture_source()) != 0 {
                // todo!()
            }

            self.renderer_vram_busy.store(false, Ordering::SeqCst);

            gl::BindFramebuffer(gl::FRAMEBUFFER, self.final_fbo.fbo);
            gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
            gl::ClearColor(0f32, 0f32, 0f32, 1f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            let b_fbo_color = if self.renderer_soft_2d.is_some() {
                let soft = self.renderer_soft_2d.as_mut().unwrap();
                soft.draw::<{ B }>(&self.gpu_mem_refs, &self.renderer_regs_2d_shared);
                let b_fbo_color = soft.blend::<{ B }>(&self.common, &self.renderer_regs_2d_shared, 0);
                soft.draw::<{ A }>(&self.gpu_mem_refs, &self.renderer_regs_2d_shared);
                b_fbo_color
            } else {
                self.renderer_2d.draw::<{ B }>(&self.gpu_mem_refs, &self.renderer_regs_2d_shared);
                let b_fbo_color = self.renderer_2d.blend::<{ B }>(&self.gpu_mem_refs, &self.renderer_regs_2d_shared, None);
                self.renderer_2d.draw::<{ A }>(&self.gpu_mem_refs, &self.renderer_regs_2d_shared);
                b_fbo_color
            };

            if self.rendering_3d {
                self.rendering_3d = false;
                let processed_3d = self.processed_3d.lock().unwrap();
                let (_processed_3d, timeout) = self
                    .processed_3d_condvar
                    .wait_timeout_while(processed_3d, Duration::from_millis(1000), |processed_3d| !*processed_3d)
                    .unwrap();
                if unlikely(timeout.timed_out()) {
                    info_println!("waiting for 3d processing timed out");
                }
                self.renderer_3d.render(&self.common, upscale_3d_factor_index, widescreen, widescreen_coefficient);
            }

            let fbo_3d = self
                .renderer_3d
                .get_fbo(self.common.pow_cnt1[0].display_swap(), upscale_3d_factor_index, widescreen, widescreen_coefficient);
            let a_fbo_color = if self.renderer_soft_2d.is_some() {
                let color_3d = fbo_3d.color();
                self.renderer_soft_2d.as_mut().unwrap().blend::<{ A }>(&self.common, &self.renderer_regs_2d_shared, color_3d)
            } else {
                self.renderer_2d.blend::<{ A }>(&self.gpu_mem_refs, &self.renderer_regs_2d_shared, Some(fbo_3d))
            };

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
                gl::BindTexture(gl::TEXTURE_2D, if disp_cap_cnt.source_a() { fbo_3d.color() } else { a_fbo_color });

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

            #[cfg(target_os = "vita")]
            let stream_sample_index = {
                let mut inner = self.stream.mutex.lock().unwrap();
                let index = inner.sample_to;
                if !inner.ptrs[index].is_null() {
                    None
                } else {
                    inner.sample_to ^= 1;
                    Some(index)
                }
            };

            #[cfg(target_os = "vita")]
            if let Some(index) = stream_sample_index {
                gl::BindFramebuffer(gl::FRAMEBUFFER, self.stream.fbo[index].fbo);
                gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
                gl::ClearColor(0.0, 0.0, 0.0, 0.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);

                gl::UseProgram(self.capture_program);

                gl::ActiveTexture(gl::TEXTURE0);
                gl::BindTexture(gl::TEXTURE_2D, if self.common.pow_cnt1[0].display_swap() { a_fbo_color } else { b_fbo_color });

                gl::Uniform2f(self.capture_size_scalers_uniform, 1.0, 1.0);

                const COORDS: [f32; 4 * 4] = [-1f32, 1f32, 0f32, 0f32, 1f32, 1f32, 1f32, 0f32, 1f32, -1f32, 1f32, 1f32, -1f32, -1f32, 0f32, 1f32];

                gl::EnableVertexAttribArray(0);
                gl::VertexAttribPointer(0, 4, gl::FLOAT, gl::FALSE, 0, COORDS.as_ptr() as _);
                gl::BeginQuery(gl::ANY_SAMPLES_PASSED, self.stream.query);
                gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
                gl::EndQuery(gl::ANY_SAMPLES_PASSED);

                gl::BindTexture(gl::TEXTURE_2D, 0);
                gl::UseProgram(0);
            }

            gl::BindFramebuffer(gl::FRAMEBUFFER, self.final_fbo.fbo);
            gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
            gl::ClearColor(0.0, 0.0, 0.0, 0.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            if screen_layout.overlay_path.is_some() {
                self.draw_overlay(screen_layout);
            }

            if self.common.pow_cnt1[0].enable() {
                let top_screen = if self.common.pow_cnt1[0].display_swap() {
                    screen_layout.get_screen_top()
                } else {
                    screen_layout.get_screen_bottom()
                };

                let is_physical_top = top_screen.2;
                let top_screen = (a_fbo_color, top_screen.0, if is_physical_top { widescreen_coefficient } else { 1.0 }, top_screen.1);

                let bottom_screen = if self.common.pow_cnt1[0].display_swap() {
                    screen_layout.get_screen_bottom()
                } else {
                    screen_layout.get_screen_top()
                };
                let bottom_screen = (b_fbo_color, bottom_screen.0, 1.0, bottom_screen.1);
                self.merge_screens([top_screen, bottom_screen], 0);
            }

            if settings.show_debug_stats() {
                let fps = fps.load(Ordering::Relaxed) as u32;
                let per = fps * 100 / 60;

                let last_time_saved = *last_save_time.lock().unwrap();
                let mut info_text = {
                    #[cfg(target_os = "vita")]
                    {
                        format!("CPU: {}MHz", vitasdk_sys::scePowerGetArmClockFrequency())
                    }
                    #[cfg(not(target_os = "vita"))]
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

                let arm7_emu: &str = settings.arm7_emu().into();

                let text = format!(
                    "{}ms ({}fps) {arm7_emu}\n{per}% ({fps}/60)\n{info_text}",
                    self.average_render_time / 1000,
                    if self.average_render_time == 0 { 0 } else { 1000000 / self.average_render_time }
                );

                // No GL-drawn OSD on Android — publish the line; the Activity polls it
                // into a TextView.
                #[cfg(target_os = "android")]
                crate::presenter::set_debug_stats(text);

                #[cfg(not(target_os = "android"))]
                {
                    const OFFSET_X: u32 = 500;
                    const OFFSET_Y: u32 = 450;
                    const WIDTH: u32 = PRESENTER_SCREEN_WIDTH - OFFSET_X;
                    const HEIGHT: u32 = PRESENTER_SCREEN_HEIGHT - OFFSET_Y;
                    gl::Viewport(OFFSET_X as _, OFFSET_Y as _, WIDTH as _, HEIGHT as _);

                    self.gl_glyph.draw(
                        text,
                        (WIDTH as f32, HEIGHT as f32),
                        (430.0, 0.0),
                        40.0,
                        Layout::default().h_align(HorizontalAlign::Right).v_align(VerticalAlign::Center),
                        1.0,
                    );
                }
            } else {
                // Clear the published line so the TextView hides when stats get toggled off.
                #[cfg(target_os = "android")]
                crate::presenter::set_debug_stats(String::new());
            }

            if unlikely(settings.retroachievements()) {
                self.show_retroachievements(ra_context);
            }

            if !pause {
                gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
                #[cfg(target_os = "vita")]
                let (sw, sh) = (PRESENTER_SCREEN_WIDTH as i32, PRESENTER_SCREEN_HEIGHT as i32);
                #[cfg(not(target_os = "vita"))]
                let (sw, sh) = self.present_surface;
                gl::Viewport(0, 0, sw, sh);
                gl::ClearColor(0f32, 0f32, 0f32, 1f32);
                gl::Clear(gl::COLOR_BUFFER_BIT);
                self.blit_main_framebuffer();
                // Frame-dump instrument (DSVITA_FRAME_DUMP=N[,M...]): glReadPixels the
                // final composed framebuffer at presented-frame #N into raw RGBA files —
                // cross-arch pixel comparison of the actual render output, downstream of
                // every emulation and renderer stage.
                if crate::IS_DEBUG {
                    use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};
                    static FRAMES: AtomicU32 = AtomicU32::new(0);
                    static TARGETS: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
                    let targets = TARGETS.get_or_init(|| std::env::var("DSVITA_FRAME_DUMP").map(|v| v.split(',').filter_map(|p| p.parse().ok()).collect()).unwrap_or_default());
                    if !targets.is_empty() {
                        let n = FRAMES.fetch_add(1, AtomicOrdering::Relaxed) + 1;
                        if targets.contains(&n) {
                            unsafe {
                                let renderer = gl::GetString(gl::RENDERER);
                                let version = gl::GetString(gl::VERSION);
                                if !renderer.is_null() && !version.is_null() {
                                    eprintln!("GLINFO renderer={:?} version={:?}", std::ffi::CStr::from_ptr(renderer as _), std::ffi::CStr::from_ptr(version as _));
                                }
                            }
                            let w = PRESENTER_SCREEN_WIDTH as usize;
                            let h = PRESENTER_SCREEN_HEIGHT as usize;
                            let mut pixels = vec![0u8; w * h * 4];
                            unsafe {
                                gl::BindFramebuffer(gl::READ_FRAMEBUFFER, self.final_fbo.fbo);
                                gl::ReadPixels(0, 0, w as _, h as _, gl::RGBA, gl::UNSIGNED_BYTE, pixels.as_mut_ptr() as _);
                                gl::BindFramebuffer(gl::READ_FRAMEBUFFER, 0);
                            }
                            let path = format!("frame_{n}.rgba");
                            let _ = std::fs::write(&path, &pixels);
                            // The 2D shader inputs for this frame: the per-scanline
                            // register tables (repr(C)/plain arrays — layout is
                            // arch-independent, so the hashes compare across builds).
                            let regs_hash = |r: &crate::core::graphics::gpu_2d::renderer_regs_2d::Gpu2DRenderRegs| {
                                use xxhash_rust::xxh32::xxh32;
                                let mut h = xxh32(unsafe { std::slice::from_raw_parts(r.disp_cnts.as_ptr() as *const u8, std::mem::size_of_val(&r.disp_cnts)) }, 0);
                                h = xxh32(unsafe { std::slice::from_raw_parts(r.bg_cnts.as_ptr() as *const u8, std::mem::size_of_val(&r.bg_cnts)) }, h);
                                h = xxh32(unsafe { std::slice::from_raw_parts(&r.win_bg_ubo as *const _ as *const u8, std::mem::size_of_val(&r.win_bg_ubo)) }, h);
                                h = xxh32(unsafe { std::slice::from_raw_parts(&r.bg_ubo as *const _ as *const u8, std::mem::size_of_val(&r.bg_ubo)) }, h);
                                h = xxh32(unsafe { std::slice::from_raw_parts(&r.blend_ubo as *const _ as *const u8, std::mem::size_of_val(&r.blend_ubo)) }, h);
                                h
                            };
                            let ha = regs_hash(&self.renderer_regs_2d_shared.regs_a[0]);
                            let hb = regs_hash(&self.renderer_regs_2d_shared.regs_b[0]);
                            // Raw engine-A table dump for content inspection (frozen vs live).
                            {
                                let r = &self.renderer_regs_2d_shared.regs_a[0];
                                let dc = unsafe { std::slice::from_raw_parts(r.disp_cnts.as_ptr() as *const u8, std::mem::size_of_val(&r.disp_cnts)) };
                                let bc = unsafe { std::slice::from_raw_parts(r.bg_cnts.as_ptr() as *const u8, std::mem::size_of_val(&r.bg_cnts)) };
                                let ofs = unsafe { std::slice::from_raw_parts(r.bg_ubo.ofs.as_ptr() as *const u8, std::mem::size_of_val(&r.bg_ubo.ofs)) };
                                let mut blob = Vec::new();
                                blob.extend_from_slice(dc);
                                blob.extend_from_slice(bc);
                                blob.extend_from_slice(ofs);
                                let _ = std::fs::write(format!("ubo_a_{n}.bin"), &blob);
                                eprintln!(
                                    "UBODUMP#{n} disp_cnt[0]={:08x} disp_cnt[96]={:08x} bg_cnt[0..4]={:04x},{:04x},{:04x},{:04x} ofs[0..4]={:08x},{:08x},{:08x},{:08x}",
                                    r.disp_cnts[0], r.disp_cnts[96], r.bg_cnts[0], r.bg_cnts[1], r.bg_cnts[2], r.bg_cnts[3], r.bg_ubo.ofs[0], r.bg_ubo.ofs[1], r.bg_ubo.ofs[2], r.bg_ubo.ofs[3]
                                );
                            }
                            eprintln!("FRAMEDUMP#{n} {w}x{h} -> {path} hash={:08x} ubo_a={ha:08x} ubo_b={hb:08x}", xxhash_rust::xxh32::xxh32(&pixels, 0));
                        }
                    }
                }
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

                #[cfg(not(target_os = "vita"))]
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

            #[cfg(target_os = "vita")]
            if let Some(index) = stream_sample_index {
                let mut query_result = 0;
                gl::GetQueryObjectiv(self.stream.query, gl::QUERY_RESULT, &mut query_result);

                gl::BindTexture(gl::TEXTURE_2D, self.stream.fbo[index].color);
                let ptr = Presenter::gl_get_tex_ptr();

                self.stream.mutex.lock().unwrap().ptrs[index] = ptr;
                self.stream.ready_condvar.notify_one();
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

    #[inline(never)]
    #[cfg(target_os = "vita")]
    pub fn process_streams(&mut self) {
        let (index, ptr) = {
            let lock = self.stream.mutex.lock().unwrap();
            let (mut inner, result) = self
                .stream
                .ready_condvar
                .wait_timeout_while(lock, Duration::from_millis(500), |inner| inner.ptrs[inner.sample_from].is_null())
                .unwrap();
            if result.timed_out() {
                return;
            }
            inner.sample_from ^= 1;
            (inner.sample_from, inner.ptrs[inner.sample_from])
        };

        unsafe { crate::presenter::udcd_uvc_dsvita_sendCustomFrame(ptr as _) };
        self.stream.mutex.lock().unwrap().ptrs[index] = std::ptr::null();
    }

    #[inline(never)]
    pub fn process_3d_loop(&mut self) {
        {
            let rendering = self.rendering.lock().unwrap();
            let _drawing = self.rendering_condvar.wait_while(rendering, |rendering| !*rendering).unwrap();
        }

        if self.is_quit() {
            return;
        }

        if self.rendering_3d {
            unsafe { self.renderer_3d.process_polygons(&mut self.common, &self.gpu_mem_refs) };
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
        }
    }

    // Reads the composed game frame back as a small jpeg for savestate thumbnails:
    // 4x downscale (960x544 -> 240x136, the exact size the savestate list renders).
    // Main thread only (owns the gl context); the pause menu blits this same fbo.
    pub fn capture_main_framebuffer_jpeg(&self) -> Vec<u8> {
        const W: usize = PRESENTER_SCREEN_WIDTH as usize;
        const H: usize = PRESENTER_SCREEN_HEIGHT as usize;
        const SCALE: usize = 4;
        const OUT_W: usize = W / SCALE;
        const OUT_H: usize = H / SCALE;
        #[cfg(not(target_os = "vita"))]
        let mut pixels = vec![0u8; W * H * 4];
        #[cfg(not(target_os = "vita"))]
        unsafe {
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, self.final_fbo.fbo);
            gl::ReadPixels(0, 0, W as _, H as _, gl::RGBA, gl::UNSIGNED_BYTE, pixels.as_mut_ptr() as _);
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, 0);
        }

        #[cfg(target_os = "vita")]
        let pixels: &[u8; W * H * 4] = unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.final_fbo.color);
            std::mem::transmute(Presenter::gl_get_tex_ptr())
        };

        let mut small = vec![0u8; OUT_W * OUT_H * 3];
        for out_y in 0..OUT_H {
            for out_x in 0..OUT_W {
                let mut sums = [0u32; 3];
                for sub_y in 0..SCALE {
                    let mut src_y = out_y * SCALE + sub_y;
                    if !cfg!(target_os = "vita") {
                        // Box-filter downscale to rgb; GL rows are bottom-up, flip while sampling
                        src_y = H - 1 - src_y;
                    }
                    for sub_x in 0..SCALE {
                        let src = (src_y * W + out_x * SCALE + sub_x) * 4;
                        for c in 0..3 {
                            sums[c] += pixels[src + c] as u32;
                        }
                    }
                }
                let dst = (out_y * OUT_W + out_x) * 3;
                for c in 0..3 {
                    small[dst + c] = (sums[c] / (SCALE * SCALE) as u32) as u8;
                }
            }
        }
        let mut jpeg = Vec::new();
        let encoder = jpeg_encoder::Encoder::new(&mut jpeg, 75);
        if encoder.encode(&small, OUT_W as u16, OUT_H as u16, jpeg_encoder::ColorType::Rgb).is_err() {
            jpeg.clear();
        }
        jpeg
    }

    /// Where the 960x544 frame lands on the default framebuffer, plus the full surface
    /// size (for the border clear). Set once from the presenter — the desktop window is
    /// exactly 960x544, Android letterboxes into an arbitrary surface. The Vita presents
    /// at fixed 960x544 and keeps those hardcoded, so it has no setter.
    #[cfg(not(target_os = "vita"))]
    pub fn set_present_rect(&mut self, rect: (i32, i32, i32, i32), surface: (i32, i32)) {
        self.present_rect = rect;
        self.present_surface = surface;
    }

    pub fn blit_main_framebuffer(&self) {
        unsafe {
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, self.final_fbo.fbo);
            #[cfg(target_os = "vita")]
            let (x, y, w, h) = (0, 0, PRESENTER_SCREEN_WIDTH as i32, PRESENTER_SCREEN_HEIGHT as i32);
            #[cfg(not(target_os = "vita"))]
            let (x, y, w, h) = self.present_rect;
            gl::BlitFramebuffer(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _, x, y, x + w, y + h, gl::COLOR_BUFFER_BIT, gl::NEAREST);
        }
    }

    pub fn is_quit(&self) -> bool {
        self.quit.load(Ordering::Relaxed)
    }

    pub fn set_quit(&self, value: bool) {
        self.quit.store(value, Ordering::Relaxed);
    }
}
