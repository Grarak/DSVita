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
    fps: u16,
    last_update: Instant,
}

impl FrameRateCounter {
    fn new() -> Self {
        FrameRateCounter {
            frame_counter: 0,
            fps: 0,
            last_update: Instant::now(),
        }
    }

    fn on_frame_ready(&mut self) {
        self.frame_counter += 1;
        let now = Instant::now();
        if (now - self.last_update).as_secs_f32() >= 1f32 {
            self.fps = self.frame_counter;
            self.frame_counter = 0;
            self.last_update = now;
            #[cfg(target_os = "linux")]
            eprintln!("{}", self.fps);
        }
    }
}

#[bitsize(16)]
#[derive(Clone, FromBits)]
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

struct GpuInner {
    disp_stat: [u16; 2],
    pow_cnt1: u16,
    dma_arm9: Rc<RefCell<Dma<{ CpuType::ARM9 }>>>,
    dma_arm7: Rc<RefCell<Dma<{ CpuType::ARM7 }>>>,
    cpu_regs_arm9: Rc<CpuRegs<{ CpuType::ARM9 }>>,
    cpu_regs_arm7: Rc<CpuRegs<{ CpuType::ARM7 }>>,
    swapchain: Arc<Swapchain>,
    frame_rate_counter: FrameRateCounter,
    v_count: u16,
    gpu_2d_context_a: Rc<Gpu2DContext<{ A }>>,
    gpu_2d_context_b: Rc<Gpu2DContext<{ B }>>,
}

impl GpuInner {
    fn new(
        dma_arm9: Rc<RefCell<Dma<{ CpuType::ARM9 }>>>,
        dma_arm7: Rc<RefCell<Dma<{ CpuType::ARM7 }>>>,
        cpu_regs_arm9: Rc<CpuRegs<{ CpuType::ARM9 }>>,
        cpu_regs_arm7: Rc<CpuRegs<{ CpuType::ARM7 }>>,
        swapchain: Arc<Swapchain>,
        gpu_2d_context_a: Rc<Gpu2DContext<{ A }>>,
        gpu_2d_context_b: Rc<Gpu2DContext<{ B }>>,
    ) -> Self {
        GpuInner {
            disp_stat: [0u16; 2],
            pow_cnt1: 0,
            dma_arm9,
            dma_arm7,
            cpu_regs_arm9,
            cpu_regs_arm7,
            swapchain,
            frame_rate_counter: FrameRateCounter::new(),
            v_count: 0,
            gpu_2d_context_a,
            gpu_2d_context_b,
        }
    }
}

pub struct GpuContext {
    inner: Rc<RefCell<GpuInner>>,
    gpu_2d_context_a: Rc<Gpu2DContext<{ A }>>,
    gpu_2d_context_b: Rc<Gpu2DContext<{ B }>>,
}

unsafe impl Send for GpuContext {}

unsafe impl Sync for GpuContext {}

impl GpuContext {
    pub fn new(
        cycle_manager: Rc<CycleManager>,
        gpu_2d_context_a: Rc<Gpu2DContext<{ A }>>,
        gpu_2d_context_b: Rc<Gpu2DContext<{ B }>>,
        dma_arm9: Rc<RefCell<Dma<{ CpuType::ARM9 }>>>,
        dma_arm7: Rc<RefCell<Dma<{ CpuType::ARM7 }>>>,
        cpu_regs_arm9: Rc<CpuRegs<{ CpuType::ARM9 }>>,
        cpu_regs_arm7: Rc<CpuRegs<{ CpuType::ARM7 }>>,
        swapchain: Arc<Swapchain>,
    ) -> GpuContext {
        let inner = Rc::new(RefCell::new(GpuInner::new(
            dma_arm9,
            dma_arm7,
            cpu_regs_arm9,
            cpu_regs_arm7,
            swapchain,
            gpu_2d_context_a.clone(),
            gpu_2d_context_b.clone(),
        )));

        cycle_manager.schedule(
            // 8 pixel delay according to https://melonds.kuribo64.net/board/thread.php?id=13
            (256 + 8) * 6,
            Box::new(Scanline256Event::new(cycle_manager.clone(), inner.clone())),
        );
        cycle_manager.schedule(
            // 8 pixel delay according to https://melonds.kuribo64.net/board/thread.php?id=13
            (355 + 8) * 6,
            Box::new(Scanline355Event::new(cycle_manager.clone(), inner.clone())),
        );

        GpuContext {
            inner,
            gpu_2d_context_a,
            gpu_2d_context_b,
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
}

#[derive(Clone)]
struct Scanline256Event {
    cycle_manager: Rc<CycleManager>,
    inner: Rc<RefCell<GpuInner>>,
}

impl Scanline256Event {
    fn new(cycle_manager: Rc<CycleManager>, inner: Rc<RefCell<GpuInner>>) -> Self {
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
            inner.gpu_2d_context_a.draw_scanline(inner.v_count as u8);
            inner.gpu_2d_context_b.draw_scanline(inner.v_count as u8);

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
}

impl Scanline355Event {
    fn new(cycle_manager: Rc<CycleManager>, inner: Rc<RefCell<GpuInner>>) -> Self {
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
            }
            262 => {
                for i in 0..2 {
                    let mut disp_stat = DispStat::from(inner.disp_stat[i]);
                    disp_stat.set_v_blank_flag(u1::new(0));
                    inner.disp_stat[i] = u16::from(disp_stat);
                }
                inner.frame_rate_counter.on_frame_ready();
                inner.swapchain.push(
                    &inner.gpu_2d_context_a.framebuffer.borrow(),
                    &inner.gpu_2d_context_b.framebuffer.borrow(),
                )
            }
            263 => {
                inner.v_count = 0;
            }
            _ => {}
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
                inner.disp_stat[i] = u16::from(disp_stat.clone());
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
