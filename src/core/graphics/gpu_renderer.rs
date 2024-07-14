use crate::core::graphics::gl_glyph::GlGlyph;
use crate::core::graphics::gpu::PowCnt1;
use crate::core::graphics::gpu_2d::registers_2d::Gpu2DRegisters;
use crate::core::graphics::gpu_2d::renderer_2d::Gpu2DRenderer;
use crate::core::graphics::gpu_2d::Gpu2DEngine::{A, B};
use crate::core::graphics::gpu_3d::renderer_3d::Gpu3DRenderer;
use crate::core::graphics::gpu_mem_buf::GpuMemBuf;
use crate::core::memory::mem::Memory;
use crate::presenter::Presenter;
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

    render_time_measure_count: u8,
    render_time_sum: u32,
    average_render_time: u16,
}

impl GpuRenderer {
    pub fn new() -> Self {
        GpuRenderer {
            renderer_2d: Gpu2DRenderer::new(),
            renderer_3d: Gpu3DRenderer::new(),

            common: GpuRendererCommon::new(),
            gl_glyph: GlGlyph::new(),

            rendering: Mutex::new(false),
            rendering_condvar: Condvar::new(),

            render_time_measure_count: 0,
            render_time_sum: 0,
            average_render_time: 0,
        }
    }

    pub fn on_scanline(&mut self, inner_a: &mut Gpu2DRegisters<{ A }>, inner_b: &mut Gpu2DRegisters<{ B }>, line: u8) {
        self.renderer_2d.on_scanline(inner_a, inner_b, line);
    }

    pub fn on_scanline_finish(&mut self, mem: &mut Memory, pow_cnt1: PowCnt1) {
        let mut rendering = self.rendering.lock().unwrap();

        if !*rendering {
            self.common.pow_cnt1 = pow_cnt1;
            self.renderer_2d.on_scanline_finish();

            self.common.mem_buf.read(mem);

            *rendering = true;
            self.rendering_condvar.notify_one();
        }
    }

    pub fn reload_registers(&mut self) {
        self.renderer_2d.reload_registers();
    }

    pub fn render_loop(&mut self, presenter: &mut Presenter, fps: &Arc<AtomicU16>) {
        {
            let drawing = self.rendering.lock().unwrap();
            let _drawing = self.rendering_condvar.wait_while(drawing, |drawing| !*drawing).unwrap();
        }

        let render_time_start = Instant::now();

        unsafe { self.renderer_2d.render(&self.common) }

        let render_time_end = Instant::now();
        let render_time_diff = render_time_end - render_time_start;
        self.render_time_sum += render_time_diff.as_millis() as u32;
        self.render_time_measure_count += 1;
        if unlikely(self.render_time_measure_count == 30) {
            self.render_time_measure_count = 0;
            self.average_render_time = (self.render_time_sum / 30) as u16;
            self.render_time_sum = 0;
        }

        unsafe {
            let fps = fps.load(Ordering::Relaxed);
            let per = fps * 100 / 60;
            self.gl_glyph.draw(format!("Render time: {}ms\nFPS: {fps} ({per}%)", self.average_render_time));

            presenter.gl_swap_window();
        }

        {
            let mut rendering = self.rendering.lock().unwrap();
            *rendering = false;
        }
    }
}
