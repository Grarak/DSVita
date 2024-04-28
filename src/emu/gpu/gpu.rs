use std::collections::VecDeque;
use std::sync::atomic::{AtomicU16, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use bilge::prelude::*;

use crate::emu::cpu_regs::InterruptFlag;
use crate::emu::cycle_manager::{CycleEvent, CycleManager};
use crate::emu::emu::{get_cm, get_cpu_regs_mut, io_dma, Emu};
use crate::emu::gpu::gpu_2d::Gpu2D;
use crate::emu::gpu::gpu_2d::Gpu2DEngine::{A, B};
use crate::emu::gpu::gpu_3d::Gpu3D;
use crate::emu::gpu::gpu_3d_renderer::Gpu3dRenderer;
use crate::emu::memory::dma::DmaTransferMode;
use crate::emu::memory::mem::Memory;
use crate::emu::CpuType;
use crate::emu::CpuType::ARM9;
use crate::logging::debug_println;
use crate::utils::HeapMemU32;

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

    fn push(&self, fb_0: &[u32; DISPLAY_PIXEL_COUNT], fb_1: &[u32; DISPLAY_PIXEL_COUNT]) {
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
            #[cfg(target_os = "linux")]
            eprintln!("{}", self.frame_counter);
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

pub struct Gpu {
    disp_stat: [DispStat; 2],
    pub pow_cnt1: u16,
    disp_cap_cnt: u32,
    swapchain: Arc<Swapchain>,
    frame_rate_counter: FrameRateCounter,
    pub frame_skip: bool,
    pub v_count: u16,
    pub gpu_2d_a: Gpu2D<{ A }>,
    pub gpu_2d_b: Gpu2D<{ B }>,
    pub gpu_3d: Gpu3D,
    pub gpu_3d_renderer: Gpu3dRenderer,
    draw_state: AtomicU8,
    draw_idling: Mutex<bool>,
    draw_condvar: Condvar,
}

impl Gpu {
    pub fn new(swapchain: Arc<Swapchain>, fps: Arc<AtomicU16>) -> Gpu {
        Gpu {
            disp_stat: [DispStat::from(0); 2],
            pow_cnt1: 0,
            disp_cap_cnt: 0,
            swapchain,
            frame_rate_counter: FrameRateCounter::new(fps),
            frame_skip: false,
            v_count: 0,
            gpu_2d_a: Gpu2D::new(),
            gpu_2d_b: Gpu2D::new(),
            gpu_3d: Gpu3D::new(),
            gpu_3d_renderer: Gpu3dRenderer::new(),
            draw_state: AtomicU8::new(0),
            draw_idling: Mutex::new(false),
            draw_condvar: Condvar::new(),
        }
    }

    pub fn initialize_schedule(cycle_manager: &CycleManager) {
        cycle_manager.schedule(
            // 8 pixel delay according to https://melonds.kuribo64.net/board/thread.php?id=13
            (256 + 8) * 6,
            Box::new(Scanline256Event::new()),
        );
    }

    pub fn get_disp_stat<const CPU: CpuType>(&self) -> u16 {
        self.disp_stat[CPU].into()
    }

    pub fn set_disp_stat<const CPU: CpuType>(&mut self, mut mask: u16, value: u16) {
        mask &= 0xFFB8;
        self.disp_stat[CPU] = ((u16::from(self.disp_stat[CPU]) & !mask) | (value & mask)).into();
    }

    pub fn set_pow_cnt1(&mut self, mut mask: u16, value: u16) {
        mask &= 0x820F;
        self.pow_cnt1 = (self.pow_cnt1 & !mask) | (value & mask);
    }

    pub fn set_disp_cap_cnt(&mut self, mut mask: u32, value: u32) {
        mask &= 0xEF3F1F1F;
        self.disp_cap_cnt = (self.disp_cap_cnt & !mask) | (value & mask);
    }

    pub fn get_fps(&self) -> u16 {
        self.frame_rate_counter.fps.load(Ordering::Relaxed)
    }

    pub fn draw_scanline_thread(&mut self, mem: &Memory) {
        if self
            .draw_state
            .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let v_count = self.v_count as u8;
            self.gpu_2d_a.draw_scanline(v_count, mem);
            if self
                .draw_state
                .compare_exchange(2, 3, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.gpu_2d_b.draw_scanline(v_count, mem);
            }
            self.draw_state.store(0, Ordering::Release);
            if v_count == 191 {
                let mut draw_idling = self.draw_idling.lock().unwrap();
                *draw_idling = true;
                let _guard = self
                    .draw_condvar
                    .wait_while(draw_idling, |idling| *idling)
                    .unwrap();
            }
        }
    }
}

struct Scanline256Event {}

impl Scanline256Event {
    fn new() -> Self {
        Scanline256Event {}
    }
}

impl CycleEvent for Scanline256Event {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, emu: &mut Emu) {
        let gpu = &mut emu.common.gpu;

        if gpu.v_count < 192 {
            if !gpu.frame_skip || gpu.frame_rate_counter.frame_counter & 1 == 0 {
                if gpu
                    .draw_state
                    .compare_exchange(2, 3, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    gpu.gpu_2d_b.draw_scanline(gpu.v_count as u8, &emu.mem);
                }
                while gpu.draw_state.load(Ordering::Acquire) != 0 {}
            }

            io_dma!(emu, ARM9).trigger_all(DmaTransferMode::StartAtHBlank, get_cm!(emu));
        }

        for i in 0..2 {
            let disp_stat = &mut gpu.disp_stat[i];
            disp_stat.set_h_blank_flag(u1::new(1));
            if bool::from(disp_stat.h_blank_irq_enable()) {
                get_cpu_regs_mut!(emu, CpuType::from(i as u8))
                    .send_interrupt(InterruptFlag::LcdHBlank, get_cm!(emu));
            }
        }

        get_cm!(emu).schedule((355 - 256) * 6, Box::new(Scanline355Event::new()));
    }
}

struct Scanline355Event {}

impl Scanline355Event {
    fn new() -> Self {
        Scanline355Event {}
    }
}

impl CycleEvent for Scanline355Event {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, emu: &mut Emu) {
        let gpu = &mut emu.common.gpu;

        gpu.v_count += 1;
        match gpu.v_count {
            192 => {
                for i in 0..2 {
                    let disp_stat = &mut gpu.disp_stat[i];
                    disp_stat.set_v_blank_flag(u1::new(1));
                    if bool::from(disp_stat.v_blank_irq_enable()) {
                        get_cpu_regs_mut!(emu, CpuType::from(i as u8))
                            .send_interrupt(InterruptFlag::LcdVBlank, get_cm!(emu));
                        io_dma!(emu, CpuType::from(i as u8))
                            .trigger_all(DmaTransferMode::StartAtVBlank, get_cm!(emu));
                    }
                }

                if !gpu.frame_skip || gpu.frame_rate_counter.frame_counter & 1 == 0 {
                    let pow_cnt1 = PowCnt1::from(gpu.pow_cnt1);
                    if bool::from(pow_cnt1.enable()) {
                        if bool::from(pow_cnt1.display_swap()) {
                            gpu.swapchain
                                .push(&gpu.gpu_2d_a.framebuffer, &gpu.gpu_2d_b.framebuffer)
                        } else {
                            gpu.swapchain
                                .push(&gpu.gpu_2d_b.framebuffer, &gpu.gpu_2d_a.framebuffer);
                        }
                    } else {
                        gpu.swapchain
                            .push(&[0u32; DISPLAY_PIXEL_COUNT], &[0u32; DISPLAY_PIXEL_COUNT]);
                    }
                }
            }
            262 => {
                for i in 0..2 {
                    gpu.disp_stat[i].set_v_blank_flag(u1::new(0));
                }
                gpu.frame_rate_counter.on_frame_ready();
            }
            263 => {
                gpu.v_count = 0;
                gpu.gpu_2d_a.reload_registers();
                gpu.gpu_2d_b.reload_registers();
            }
            _ => {}
        }

        if gpu.v_count < 192 && (!gpu.frame_skip || gpu.frame_rate_counter.frame_counter & 1 == 0) {
            gpu.draw_state.store(1, Ordering::Release);
            if gpu.v_count == 0 {
                let mut draw_idling = gpu.draw_idling.lock().unwrap();
                *draw_idling = false;
                gpu.draw_condvar.notify_one();
            }
        }

        for i in 0..2 {
            let v_match = (u16::from(gpu.disp_stat[i].v_count_msb()) << 8)
                | gpu.disp_stat[i].v_count_setting() as u16;
            debug_println!(
                "v match {:x} {} {}",
                u16::from(gpu.disp_stat[i]),
                v_match,
                gpu.v_count
            );
            if gpu.v_count == v_match {
                gpu.disp_stat[i].set_v_counter_flag(u1::new(1));
                if bool::from(gpu.disp_stat[i].v_counter_irq_enable()) {
                    get_cpu_regs_mut!(emu, CpuType::from(i as u8))
                        .send_interrupt(InterruptFlag::LcdVCounterMatch, get_cm!(emu));
                }
            } else {
                gpu.disp_stat[i].set_v_counter_flag(u1::new(0));
            }
            gpu.disp_stat[i].set_h_blank_flag(u1::new(0));
        }

        get_cm!(emu).schedule(256 * 6, Box::new(Scanline256Event::new()));
    }
}
