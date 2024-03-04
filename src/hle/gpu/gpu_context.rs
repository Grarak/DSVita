use crate::hle::cpu_regs::{CpuRegs, InterruptFlag};
use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::gpu::gpu_2d_context::Gpu2DContext;
use crate::hle::gpu::gpu_2d_context::Gpu2DEngine::{A, B};
use crate::hle::memory::dma::{Dma, DmaTransferMode};
use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::utils::HeapMemU32;
use bilge::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicU16, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

pub const DISPLAY_WIDTH: usize = 256;
pub const DISPLAY_HEIGHT: usize = 192;
pub const DISPLAY_PIXEL_COUNT: usize = DISPLAY_WIDTH * DISPLAY_HEIGHT;

pub struct Swapchain {
    queue: Mutex<VecDeque<HeapMemU32<{ DISPLAY_PIXEL_COUNT * 2 }>>>,
    cond_var: Condvar,
}

impl Swapchain {
    pub fn new() -> Self {
        Swapchain {
            queue: Mutex::new(VecDeque::new()),
            cond_var: Condvar::new(),
        }
    }

    pub fn push(&self, fb_0: &[u32; DISPLAY_PIXEL_COUNT], fb_1: &[u32; DISPLAY_PIXEL_COUNT]) {
        let mut queue = self.queue.lock().unwrap();
        if queue.len() == 2 {
            queue.swap(0, 1);
            let fb = queue.back_mut().unwrap();
            fb[0..DISPLAY_PIXEL_COUNT].copy_from_slice(fb_0);
            fb[DISPLAY_PIXEL_COUNT..DISPLAY_PIXEL_COUNT * 2].copy_from_slice(fb_1);
        } else {
            let mut fb = HeapMemU32::new();
            fb[0..DISPLAY_PIXEL_COUNT].copy_from_slice(fb_0);
            fb[DISPLAY_PIXEL_COUNT..DISPLAY_PIXEL_COUNT * 2].copy_from_slice(fb_1);
            queue.push_back(fb);
            self.cond_var.notify_one();
        }
    }

    pub fn consume(&self) -> HeapMemU32<{ DISPLAY_PIXEL_COUNT * 2 }> {
        let mut fb = {
            let queue = self.queue.lock().unwrap();
            let mut queue = self
                .cond_var
                .wait_while(queue, |queue| queue.is_empty())
                .unwrap();
            queue.pop_front().unwrap()
        };
        fb.iter_mut().for_each(|value| {
            *value = Self::rgb6_to_rgb8(*value);
        });
        fb
    }

    fn rgb6_to_rgb8(color: u32) -> u32 {
        let r = (color & 0x3F) * 255 / 63;
        let g = ((color >> 6) & 0x3F) * 255 / 63;
        let b = ((color >> 12) & 0x3F) * 255 / 63;
        (0xFFu32 << 24) | (b << 16) | (g << 8) | r
    }
}

struct FrameRateCounter {
    frame_counter: u16,
    fps: Arc<AtomicU16>,
    last_update: Instant,
}

impl FrameRateCounter {
    fn new(fps: Arc<AtomicU16>) -> Self {
        FrameRateCounter {
            frame_counter: 0,
            fps,
            last_update: Instant::now(),
        }
    }

    fn on_frame_ready(&mut self) {
        self.frame_counter += 1;
        let now = Instant::now();
        if (now - self.last_update).as_secs_f32() >= 1f32 {
            self.fps.store(self.frame_counter, Ordering::Relaxed);
            self.frame_counter = 0;
            self.last_update = now;
        }
    }
}

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
struct DispStat {
    v_blank_flag: u1,
    h_blank_flag: u1,
    v_counter_flag: u1,
    v_blank_irq_enable: u1,
    h_blank_irq_enable: u1,
    v_counter_irq_enable: u1,
    not_used: u1,
    v_count_msb: u1,
    v_count_setting: u8,
}

#[bitsize(16)]
#[derive(FromBits)]
struct PowCnt1 {
    enable: u1,
    gpu_2d_a_enable: u1,
    rendering_3d_enable: u1,
    geometry_3d_enable: u1,
    not_used: u5,
    gpu_2d_b_enable: u1,
    not_used1: u5,
    display_swap: u1,
}

struct GpuInner {
    disp_stat: [u16; 2],
    pow_cnt1: u16,
    disp_cap_cnt: u32,
    dma_arm9: Rc<RefCell<Dma<{ CpuType::ARM9 }>>>,
    dma_arm7: Rc<RefCell<Dma<{ CpuType::ARM7 }>>>,
    cpu_regs_arm9: Rc<CpuRegs<{ CpuType::ARM9 }>>,
    cpu_regs_arm7: Rc<CpuRegs<{ CpuType::ARM7 }>>,
    swapchain: Arc<Swapchain>,
    frame_rate_counter: FrameRateCounter,
    v_count: u16,
    gpu_2d_context_a: Rc<RefCell<Gpu2DContext<{ A }>>>,
    gpu_2d_context_b: Rc<RefCell<Gpu2DContext<{ B }>>>,
}

impl GpuInner {
    fn new(
        dma_arm9: Rc<RefCell<Dma<{ CpuType::ARM9 }>>>,
        dma_arm7: Rc<RefCell<Dma<{ CpuType::ARM7 }>>>,
        cpu_regs_arm9: Rc<CpuRegs<{ CpuType::ARM9 }>>,
        cpu_regs_arm7: Rc<CpuRegs<{ CpuType::ARM7 }>>,
        swapchain: Arc<Swapchain>,
        gpu_2d_context_a: Rc<RefCell<Gpu2DContext<{ A }>>>,
        gpu_2d_context_b: Rc<RefCell<Gpu2DContext<{ B }>>>,
        fps: Arc<AtomicU16>,
    ) -> Self {
        GpuInner {
            disp_stat: [0u16; 2],
            pow_cnt1: 0,
            disp_cap_cnt: 0,
            dma_arm9,
            dma_arm7,
            cpu_regs_arm9,
            cpu_regs_arm7,
            swapchain,
            frame_rate_counter: FrameRateCounter::new(fps),
            v_count: 0,
            gpu_2d_context_a,
            gpu_2d_context_b,
        }
    }
}

struct DrawingThread {
    state: AtomicU8,
    running: Mutex<bool>,
    condvar: Condvar,
}

impl DrawingThread {
    fn new() -> Self {
        DrawingThread {
            state: AtomicU8::new(0),
            running: Mutex::new(false),
            condvar: Condvar::new(),
        }
    }
}

pub struct GpuContext {
    inner: Rc<RefCell<GpuInner>>,
    gpu_2d_context_a: Rc<RefCell<Gpu2DContext<{ A }>>>,
    gpu_2d_context_b: Rc<RefCell<Gpu2DContext<{ B }>>>,
    drawing_thread: Arc<DrawingThread>,
    fps: Arc<AtomicU16>,
}

unsafe impl Send for GpuContext {}
unsafe impl Sync for GpuContext {}

impl GpuContext {
    pub fn new(
        cycle_manager: Rc<CycleManager>,
        gpu_2d_context_a: Rc<RefCell<Gpu2DContext<{ A }>>>,
        gpu_2d_context_b: Rc<RefCell<Gpu2DContext<{ B }>>>,
        dma_arm9: Rc<RefCell<Dma<{ CpuType::ARM9 }>>>,
        dma_arm7: Rc<RefCell<Dma<{ CpuType::ARM7 }>>>,
        cpu_regs_arm9: Rc<CpuRegs<{ CpuType::ARM9 }>>,
        cpu_regs_arm7: Rc<CpuRegs<{ CpuType::ARM7 }>>,
        swapchain: Arc<Swapchain>,
    ) -> GpuContext {
        let drawing_thread = Arc::new(DrawingThread::new());
        let fps = Arc::new(AtomicU16::new(0));

        let inner = Rc::new(RefCell::new(GpuInner::new(
            dma_arm9,
            dma_arm7,
            cpu_regs_arm9,
            cpu_regs_arm7,
            swapchain,
            gpu_2d_context_a.clone(),
            gpu_2d_context_b.clone(),
            fps.clone(),
        )));

        cycle_manager.schedule(
            // 8 pixel delay according to https://melonds.kuribo64.net/board/thread.php?id=13
            (256 + 8) * 6,
            Box::new(Scanline256Event::new(
                cycle_manager.clone(),
                inner.clone(),
                drawing_thread.clone(),
            )),
        );
        cycle_manager.schedule(
            // 8 pixel delay according to https://melonds.kuribo64.net/board/thread.php?id=13
            (355 + 8) * 6,
            Box::new(Scanline355Event::new(
                cycle_manager.clone(),
                inner.clone(),
                drawing_thread.clone(),
            )),
        );

        GpuContext {
            inner,
            gpu_2d_context_a,
            gpu_2d_context_b,
            drawing_thread,
            fps,
        }
    }

    pub fn get_disp_stat<const CPU: CpuType>(&self) -> u16 {
        self.inner.borrow().disp_stat[CPU]
    }

    pub fn get_pow_cnt1(&self) -> u16 {
        self.inner.borrow().pow_cnt1
    }

    pub fn get_v_count(&self) -> u16 {
        self.inner.borrow().v_count
    }

    pub fn set_disp_stat<const CPU: CpuType>(&self, mut mask: u16, value: u16) {
        mask &= 0xFFB8;
        let mut inner = self.inner.borrow_mut();
        inner.disp_stat[CPU] = (inner.disp_stat[CPU] & !mask) | (value & mask);
    }

    pub fn set_pow_cnt1(&self, mut mask: u16, value: u16) {
        mask &= 0x820F;
        let mut inner = self.inner.borrow_mut();
        inner.pow_cnt1 = (inner.pow_cnt1 & !mask) | (value & mask);
    }

    pub fn set_disp_cap_cnt(&self, mut mask: u32, value: u32) {
        mask &= 0xEF3F1F1F;
        let mut inner = self.inner.borrow_mut();
        inner.disp_cap_cnt = (inner.disp_cap_cnt & !mask) | (value & mask);
    }

    pub fn get_fps(&self) -> u16 {
        self.fps.load(Ordering::Relaxed)
    }

    pub fn draw_scanline_thread(&self) {
        let drawing_thread = &self.drawing_thread;
        if drawing_thread
            .state
            .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            unsafe {
                let v_count = (*self.inner.as_ptr()).v_count as u8;
                (*self.gpu_2d_context_a.as_ptr()).draw_scanline(v_count);
                if drawing_thread
                    .state
                    .compare_exchange(2, 3, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    (*self.gpu_2d_context_b.as_ptr()).draw_scanline(v_count);
                }
                drawing_thread.state.store(0, Ordering::Release);
                if v_count == 191 {
                    let mut running = drawing_thread.running.lock().unwrap();
                    *running = false;
                    let _guard = drawing_thread
                        .condvar
                        .wait_while(running, |running| !*running)
                        .unwrap();
                }
            }
        }
    }
}

#[derive(Clone)]
struct Scanline256Event {
    cycle_manager: Rc<CycleManager>,
    inner: Rc<RefCell<GpuInner>>,
    drawing_thread: Arc<DrawingThread>,
}

impl Scanline256Event {
    fn new(
        cycle_manager: Rc<CycleManager>,
        inner: Rc<RefCell<GpuInner>>,
        drawing_thread: Arc<DrawingThread>,
    ) -> Self {
        Scanline256Event {
            cycle_manager,
            inner,
            drawing_thread,
        }
    }
}

impl CycleEvent for Scanline256Event {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, delay: u16) {
        let mut inner = self.inner.borrow_mut();
        if inner.v_count < 192 {
            if self
                .drawing_thread
                .state
                .compare_exchange(2, 3, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                inner
                    .gpu_2d_context_b
                    .borrow_mut()
                    .draw_scanline(inner.v_count as u8);
            }
            while self.drawing_thread.state.load(Ordering::Acquire) != 0 {}

            inner
                .dma_arm9
                .borrow()
                .trigger_all(DmaTransferMode::StartAtHBlank);
        }

        for i in 0..2 {
            let mut disp_stat = DispStat::from(inner.disp_stat[i]);
            disp_stat.set_h_blank_flag(u1::new(1));
            let irq = bool::from(disp_stat.h_blank_irq_enable());
            inner.disp_stat[i] = u16::from(disp_stat);

            if irq {
                todo!()
            }
        }

        self.cycle_manager
            .schedule(355 * 6 - delay as u32, Box::new(self.clone()));
    }
}

#[derive(Clone)]
struct Scanline355Event {
    cycle_manager: Rc<CycleManager>,
    inner: Rc<RefCell<GpuInner>>,
    drawing_thread: Arc<DrawingThread>,
}

impl Scanline355Event {
    fn new(
        cycle_manager: Rc<CycleManager>,
        inner: Rc<RefCell<GpuInner>>,
        drawing_thread: Arc<DrawingThread>,
    ) -> Self {
        Scanline355Event {
            cycle_manager,
            inner,
            drawing_thread,
        }
    }
}

impl CycleEvent for Scanline355Event {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, delay: u16) {
        let mut inner = self.inner.borrow_mut();
        inner.v_count += 1;
        match inner.v_count {
            192 => {
                for i in 0..2 {
                    let mut disp_stat = DispStat::from(inner.disp_stat[i]);
                    disp_stat.set_v_blank_flag(u1::new(1));
                    let irq = bool::from(disp_stat.v_blank_irq_enable());
                    inner.disp_stat[i] = u16::from(disp_stat);

                    if i == 0 {
                        if irq {
                            inner.cpu_regs_arm9.send_interrupt(InterruptFlag::LcdVBlank);
                        }
                        inner
                            .dma_arm9
                            .borrow()
                            .trigger_all(DmaTransferMode::StartAtVBlank);
                    } else {
                        if irq {
                            inner.cpu_regs_arm7.send_interrupt(InterruptFlag::LcdVBlank);
                        }
                        inner
                            .dma_arm7
                            .borrow()
                            .trigger_all(DmaTransferMode::StartAtVBlank);
                    }
                }

                let pow_cnt1 = PowCnt1::from(inner.pow_cnt1);
                if bool::from(pow_cnt1.enable()) {
                    if bool::from(pow_cnt1.display_swap()) {
                        inner.swapchain.push(
                            &inner.gpu_2d_context_a.borrow().framebuffer,
                            &inner.gpu_2d_context_b.borrow().framebuffer,
                        )
                    } else {
                        inner.swapchain.push(
                            &inner.gpu_2d_context_b.borrow().framebuffer,
                            &inner.gpu_2d_context_a.borrow().framebuffer,
                        );
                    }
                } else {
                    inner
                        .swapchain
                        .push(&[0u32; DISPLAY_PIXEL_COUNT], &[0u32; DISPLAY_PIXEL_COUNT]);
                }
            }
            262 => {
                for i in 0..2 {
                    let mut disp_stat = DispStat::from(inner.disp_stat[i]);
                    disp_stat.set_v_blank_flag(u1::new(0));
                    inner.disp_stat[i] = u16::from(disp_stat);
                }
                inner.frame_rate_counter.on_frame_ready();
            }
            263 => {
                inner.v_count = 0;
                inner.gpu_2d_context_a.borrow_mut().reload_registers();
                inner.gpu_2d_context_b.borrow_mut().reload_registers();
            }
            _ => {}
        }

        if inner.v_count < 192 {
            self.drawing_thread.state.store(1, Ordering::Release);
            if inner.v_count == 0 {
                {
                    let mut running = self.drawing_thread.running.lock().unwrap();
                    *running = true;
                    self.drawing_thread.condvar.notify_one();
                }
            }
        }

        for i in 0..2 {
            let mut disp_stat = DispStat::from(inner.disp_stat[i]);
            let v_match =
                (u16::from(disp_stat.v_count_msb()) << 8) | disp_stat.v_count_setting() as u16;
            debug_println!(
                "v match {:x} {} {}",
                inner.disp_stat[i],
                v_match,
                inner.v_count
            );
            if inner.v_count == v_match {
                disp_stat.set_v_counter_flag(u1::new(1));
                let irq = bool::from(disp_stat.v_counter_irq_enable());
                inner.disp_stat[i] = u16::from(disp_stat);
                if irq {
                    if i == 0 {
                        inner
                            .cpu_regs_arm9
                            .send_interrupt(InterruptFlag::LcdVCounterMatch);
                    } else {
                        inner
                            .cpu_regs_arm7
                            .send_interrupt(InterruptFlag::LcdVCounterMatch);
                    }
                }
            } else {
                disp_stat.set_v_counter_flag(u1::new(0));
            }
            disp_stat.set_h_blank_flag(u1::new(0));

            inner.disp_stat[i] = u16::from(disp_stat);
        }

        self.cycle_manager
            .schedule(355 * 6 - delay as u32, Box::new(self.clone()));
    }
}
