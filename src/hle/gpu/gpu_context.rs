use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::gpu::gpu_2d_context::Gpu2DContext;
use crate::hle::gpu::gpu_2d_context::Gpu2DEngine::{A, B};
use crate::hle::CpuType;
use crate::utils::FastCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

struct GpuInner {
    disp_stat: [u16; 2],
    pow_cnt1: u16,
    v_count: u16,
    last_frame: Instant,
    gpu_2d_context_a: Rc<FastCell<Gpu2DContext<{ A }>>>,
    gpu_2d_context_b: Rc<FastCell<Gpu2DContext<{ B }>>>,
}

impl GpuInner {
    fn new(
        gpu_2d_context_a: Rc<FastCell<Gpu2DContext<{ A }>>>,
        gpu_2d_context_b: Rc<FastCell<Gpu2DContext<{ B }>>>,
    ) -> Self {
        GpuInner {
            disp_stat: [0u16; 2],
            pow_cnt1: 0,
            v_count: 0,
            last_frame: Instant::now(),
            gpu_2d_context_a,
            gpu_2d_context_b,
        }
    }
}

pub struct GpuContext {
    inner: Rc<FastCell<GpuInner>>,
}

impl GpuContext {
    pub fn new(
        cycle_manager: Arc<CycleManager>,
        gpu_2d_context_a: Rc<FastCell<Gpu2DContext<{ A }>>>,
        gpu_2d_context_b: Rc<FastCell<Gpu2DContext<{ B }>>>,
    ) -> GpuContext {
        let inner = Rc::new(FastCell::new(GpuInner::new(
            gpu_2d_context_a,
            gpu_2d_context_b,
        )));

        cycle_manager.schedule::<{ CpuType::ARM9 }>(
            256 * 6,
            Box::new(Scanline256Event::new(cycle_manager.clone(), inner.clone())),
        );
        cycle_manager.schedule::<{ CpuType::ARM9 }>(
            355 * 6,
            Box::new(Scanline355Event::new(cycle_manager.clone(), inner.clone())),
        );

        GpuContext { inner }
    }

    pub fn get_disp_stat(&self, cpu_type: CpuType) -> u16 {
        self.inner.borrow().disp_stat[cpu_type as usize]
    }

    pub fn set_disp_stat<const CPU: CpuType>(&mut self, mask: u16, value: u16) {
        let mut inner = self.inner.borrow_mut();
        inner.disp_stat[CPU as usize] = (inner.disp_stat[CPU as usize] & !mask) | (value & mask);
    }

    pub fn set_pow_cnt1(&mut self, mask: u16, value: u16) {
        let mut inner = self.inner.borrow_mut();
        inner.pow_cnt1 = (inner.pow_cnt1 & !mask) | (value & mask);
    }
}

struct Scanline256Event {
    cycle_manager: Arc<CycleManager>,
    inner: Rc<FastCell<GpuInner>>,
}

impl Scanline256Event {
    fn new(cycle_manager: Arc<CycleManager>, inner: Rc<FastCell<GpuInner>>) -> Self {
        Scanline256Event {
            cycle_manager,
            inner,
        }
    }
}

impl CycleEvent for Scanline256Event {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, delay: u16) {
        let mut inner = self.inner.borrow_mut();
        if inner.v_count < 192 {
            inner
                .gpu_2d_context_a
                .borrow_mut()
                .draw_scanline(inner.v_count);
            inner
                .gpu_2d_context_b
                .borrow_mut()
                .draw_scanline(inner.v_count);
        }
        self.cycle_manager.schedule::<{ CpuType::ARM9 }>(
            355 * 6 - delay as u32,
            Box::new(Scanline256Event::new(
                self.cycle_manager.clone(),
                self.inner.clone(),
            )),
        );
    }
}

struct Scanline355Event {
    cycle_manager: Arc<CycleManager>,
    inner: Rc<FastCell<GpuInner>>,
}

impl Scanline355Event {
    fn new(cycle_manager: Arc<CycleManager>, inner: Rc<FastCell<GpuInner>>) -> Self {
        Scanline355Event {
            cycle_manager,
            inner,
        }
    }
}

impl CycleEvent for Scanline355Event {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, delay: u16) {
        let mut inner = self.inner.borrow_mut();
        inner.v_count += 1;
        match inner.v_count {
            192 => {}
            262 => {}
            263 => {
                inner.v_count = 0;
                let now = Instant::now();
                let elapsed = inner.last_frame.elapsed();
                println!("ms since last frame {}", elapsed.as_millis());
                inner.last_frame = now;
            }
            _ => {}
        }
        self.cycle_manager.schedule::<{ CpuType::ARM9 }>(
            355 * 6 - delay as u32,
            Box::new(Scanline355Event::new(
                self.cycle_manager.clone(),
                self.inner.clone(),
            )),
        );
    }
}
