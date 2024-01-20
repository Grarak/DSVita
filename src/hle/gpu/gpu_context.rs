use crate::hle::cpu_regs::{CpuRegs, InterruptFlag};
use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::gpu::gpu_2d_context::Gpu2DContext;
use crate::hle::gpu::gpu_2d_context::Gpu2DEngine::{A, B};
use crate::hle::memory::dma::{Dma, DmaTransferMode};
use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::utils::{FastCell, HeapMemU32};
use bilge::prelude::*;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex, RwLock};

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
        }
        self.cond_var.notify_one();
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
        fb.iter_mut()
            .for_each(|pixel| *pixel = Self::rgb6_to_rgb8(*pixel));
        fb
    }

    fn rgb6_to_rgb8(color: u32) -> u32 {
        let r = (color & 0x3F) * 255 / 63;
        let g = ((color >> 6) & 0x3F) * 255 / 63;
        let b = ((color >> 12) & 0x3F) * 255 / 63;
        (0xFFu32 << 24) | (b << 16) | (g << 8) | r
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
    v_count: u16,
    gpu_2d_context_a: Rc<FastCell<Gpu2DContext<{ A }>>>,
    gpu_2d_context_b: Rc<FastCell<Gpu2DContext<{ B }>>>,
    dma_arm9: Arc<RwLock<Dma<{ CpuType::ARM9 }>>>,
    dma_arm7: Arc<RwLock<Dma<{ CpuType::ARM7 }>>>,
    cpu_regs_arm9: Arc<CpuRegs<{ CpuType::ARM9 }>>,
    cpu_regs_arm7: Arc<CpuRegs<{ CpuType::ARM7 }>>,
    swapchain: Arc<Swapchain>,
}

impl GpuInner {
    fn new(
        gpu_2d_context_a: Rc<FastCell<Gpu2DContext<{ A }>>>,
        gpu_2d_context_b: Rc<FastCell<Gpu2DContext<{ B }>>>,
        dma_arm9: Arc<RwLock<Dma<{ CpuType::ARM9 }>>>,
        dma_arm7: Arc<RwLock<Dma<{ CpuType::ARM7 }>>>,
        cpu_regs_arm9: Arc<CpuRegs<{ CpuType::ARM9 }>>,
        cpu_regs_arm7: Arc<CpuRegs<{ CpuType::ARM7 }>>,
        swapchain: Arc<Swapchain>,
    ) -> Self {
        GpuInner {
            disp_stat: [0u16; 2],
            pow_cnt1: 0,
            v_count: 0,
            gpu_2d_context_a,
            gpu_2d_context_b,
            dma_arm9,
            dma_arm7,
            cpu_regs_arm9,
            cpu_regs_arm7,
            swapchain,
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
        dma_arm9: Arc<RwLock<Dma<{ CpuType::ARM9 }>>>,
        dma_arm7: Arc<RwLock<Dma<{ CpuType::ARM7 }>>>,
        cpu_regs_arm9: Arc<CpuRegs<{ CpuType::ARM9 }>>,
        cpu_regs_arm7: Arc<CpuRegs<{ CpuType::ARM7 }>>,
        swapchain: Arc<Swapchain>,
    ) -> GpuContext {
        let inner = Rc::new(FastCell::new(GpuInner::new(
            gpu_2d_context_a,
            gpu_2d_context_b,
            dma_arm9,
            dma_arm7,
            cpu_regs_arm9,
            cpu_regs_arm7,
            swapchain,
        )));

        cycle_manager.schedule::<{ CpuType::ARM9 }, _>(
            // 8 pixel delay according to https://melonds.kuribo64.net/board/thread.php?id=13
            (256 + 8) * 6,
            Box::new(Scanline256Event::new(cycle_manager.clone(), inner.clone())),
        );
        cycle_manager.schedule::<{ CpuType::ARM9 }, _>(
            // 8 pixel delay according to https://melonds.kuribo64.net/board/thread.php?id=13
            (355 + 8) * 6,
            Box::new(Scanline355Event::new(cycle_manager.clone(), inner.clone())),
        );

        GpuContext { inner }
    }

    pub fn get_disp_stat(&self, cpu_type: CpuType) -> u16 {
        self.inner.borrow().disp_stat[cpu_type as usize]
    }

    pub fn set_disp_stat<const CPU: CpuType>(&mut self, mut mask: u16, value: u16) {
        mask &= 0xFFB8;
        let mut inner = self.inner.borrow_mut();
        inner.disp_stat[CPU as usize] = (inner.disp_stat[CPU as usize] & !mask) | (value & mask);
    }

    pub fn set_pow_cnt1(&mut self, mut mask: u16, value: u16) {
        mask &= 0x820F;
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

            inner
                .dma_arm9
                .read()
                .unwrap()
                .trigger_all(DmaTransferMode::StartAtHBlank);
        }

        for (index, stat) in inner.disp_stat.iter_mut().enumerate() {
            let mut disp_stat = DispStat::from(*stat);
            disp_stat.set_h_blank_flag(u1::new(1));
            let irq = bool::from(disp_stat.h_blank_irq_enable());
            *stat = u16::from(disp_stat);

            if irq {
                todo!()
            }
        }

        self.cycle_manager.schedule::<{ CpuType::ARM9 }, _>(
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
                            .read()
                            .unwrap()
                            .trigger_all(DmaTransferMode::StartAtVBlank);
                    } else {
                        if irq {
                            inner.cpu_regs_arm7.send_interrupt(InterruptFlag::LcdVBlank);
                        }
                        inner
                            .dma_arm7
                            .read()
                            .unwrap()
                            .trigger_all(DmaTransferMode::StartAtVBlank);
                    }
                }

                let gpu_2d_a = inner.gpu_2d_context_a.borrow();
                let gpu_2d_b = inner.gpu_2d_context_b.borrow();
                inner
                    .swapchain
                    .push(&gpu_2d_a.framebuffer, &gpu_2d_b.framebuffer);
            }
            262 => {
                for stat in &mut inner.disp_stat {
                    let mut disp_stat = DispStat::from(*stat);
                    disp_stat.set_v_blank_flag(u1::new(0));
                    *stat = u16::from(disp_stat);
                }
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

        self.cycle_manager.schedule::<{ CpuType::ARM9 }, _>(
            355 * 6 - delay as u32,
            Box::new(Scanline355Event::new(
                self.cycle_manager.clone(),
                self.inner.clone(),
            )),
        );
    }
}
