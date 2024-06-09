use crate::emu::cpu_regs::InterruptFlag;
use crate::emu::cycle_manager::{CycleManager, EventType};
use crate::emu::emu::{get_arm7_hle_mut, get_cm_mut, get_common_mut, get_cpu_regs_mut, get_mem_mut, io_dma, Emu};
use crate::emu::gpu::gl::gpu_2d_renderer::Gpu2dRenderer;
use crate::emu::gpu::gpu_2d::Gpu2D;
use crate::emu::gpu::gpu_2d::Gpu2DEngine::{A, B};
use crate::emu::gpu::gpu_3d::Gpu3D;
use crate::emu::gpu::gpu_3d_renderer::Gpu3dRenderer;
use crate::emu::hle::arm7_hle::Arm7Hle;
use crate::emu::memory::dma::DmaTransferMode;
use crate::emu::CpuType;
use crate::emu::CpuType::ARM9;
use crate::logging::debug_println;
use crate::utils::HeapMemU32;
use bilge::prelude::*;
use std::collections::VecDeque;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU16, Ordering};
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
            let mut queue = self.cond_var.wait_while(queue, |queue| queue.is_empty()).unwrap();
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
            // #[cfg(target_os = "linux")]
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
#[derive(Copy, Clone, FromBits)]
pub struct PowCnt1 {
    pub enable: bool,
    gpu_2d_a_enable: u1,
    rendering_3d_enable: u1,
    geometry_3d_enable: u1,
    not_used: u5,
    gpu_2d_b_enable: u1,
    not_used1: u5,
    pub display_swap: bool,
}

pub struct Gpu {
    disp_stat: [DispStat; 2],
    pub pow_cnt1: u16,
    disp_cap_cnt: u32,
    swapchain: Arc<Swapchain>,
    frame_rate_counter: FrameRateCounter,
    pub frame_skip: bool,
    pub arm7_hle: bool,
    pub v_count: u16,
    pub gpu_2d_a: Gpu2D<{ A }>,
    pub gpu_2d_b: Gpu2D<{ B }>,
    pub gpu_3d: Gpu3D,
    pub gpu_3d_renderer: Gpu3dRenderer,
    pub gpu_2d_renderer: Option<NonNull<Gpu2dRenderer>>,
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
            arm7_hle: false,
            v_count: 0,
            gpu_2d_a: Gpu2D::new(),
            gpu_2d_b: Gpu2D::new(),
            gpu_3d: Gpu3D::new(),
            gpu_3d_renderer: Gpu3dRenderer::new(),
            gpu_2d_renderer: None,
        }
    }

    pub fn initialize_schedule(cycle_manager: &mut CycleManager) {
        cycle_manager.schedule(
            // 8 pixel delay according to https://melonds.kuribo64.net/board/thread.php?id=13
            (256 + 8) * 6,
            EventType::GpuScanline256,
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

    pub fn on_scanline256_event(emu: &mut Emu) {
        let gpu = &mut get_common_mut!(emu).gpu;

        if gpu.v_count < 192 {
            if !gpu.frame_skip || gpu.frame_rate_counter.frame_counter & 1 == 0 {
                unsafe {
                    gpu.gpu_2d_renderer
                        .unwrap_unchecked()
                        .as_mut()
                        .on_scanline(&mut gpu.gpu_2d_a.inner, &mut gpu.gpu_2d_b.inner, gpu.v_count as u8)
                }
            }
            io_dma!(emu, ARM9).trigger_all(DmaTransferMode::StartAtHBlank, get_cm_mut!(emu));
        }

        for i in 0..2 {
            let disp_stat = &mut gpu.disp_stat[i];
            disp_stat.set_h_blank_flag(u1::new(1));
            if bool::from(disp_stat.h_blank_irq_enable()) {
                get_cpu_regs_mut!(emu, CpuType::from(i as u8)).send_interrupt(InterruptFlag::LcdHBlank, get_cm_mut!(emu));
            }
        }

        get_cm_mut!(emu).schedule((355 - 256) * 6, EventType::GpuScanline355);
    }

    pub fn on_scanline355_event(emu: &mut Emu) {
        let gpu = &mut get_common_mut!(emu).gpu;

        gpu.v_count += 1;
        match gpu.v_count {
            192 => {
                if !gpu.frame_skip || gpu.frame_rate_counter.frame_counter & 1 == 0 {
                    // unsafe { gpu.gpu_2d_renderer.unwrap_unchecked().as_mut() }.on_frame(get_mem_mut!(emu));
                    unsafe { gpu.gpu_2d_renderer.unwrap_unchecked().as_mut() }.start_drawing(get_mem_mut!(emu), PowCnt1::from(gpu.pow_cnt1));
                }

                for i in 0..2 {
                    let disp_stat = &mut gpu.disp_stat[i];
                    disp_stat.set_v_blank_flag(u1::new(1));
                    if bool::from(disp_stat.v_blank_irq_enable()) {
                        get_cpu_regs_mut!(emu, CpuType::from(i as u8)).send_interrupt(InterruptFlag::LcdVBlank, get_cm_mut!(emu));
                        io_dma!(emu, CpuType::from(i as u8)).trigger_all(DmaTransferMode::StartAtVBlank, get_cm_mut!(emu));
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

                let gpu_2d_renderer = unsafe { gpu.gpu_2d_renderer.unwrap_unchecked().as_mut() };
                // gpu_2d_renderer.wait_for_drawing();
                gpu_2d_renderer.reload_registers();

                if gpu.arm7_hle {
                    Arm7Hle::on_frame(emu);
                }
            }
            _ => {}
        }

        for i in 0..2 {
            let v_match = (u16::from(gpu.disp_stat[i].v_count_msb()) << 8) | gpu.disp_stat[i].v_count_setting() as u16;
            debug_println!("v match {:x} {} {}", u16::from(gpu.disp_stat[i]), v_match, gpu.v_count);
            if gpu.v_count == v_match {
                gpu.disp_stat[i].set_v_counter_flag(u1::new(1));
                if bool::from(gpu.disp_stat[i].v_counter_irq_enable()) {
                    get_cpu_regs_mut!(emu, CpuType::from(i as u8)).send_interrupt(InterruptFlag::LcdVCounterMatch, get_cm_mut!(emu));
                }
            } else {
                gpu.disp_stat[i].set_v_counter_flag(u1::new(0));
            }
            gpu.disp_stat[i].set_h_blank_flag(u1::new(0));
        }

        if gpu.arm7_hle {
            get_arm7_hle_mut!(emu).on_scanline(gpu.v_count, emu);
        }

        get_cm_mut!(emu).schedule(256 * 6, EventType::GpuScanline256);
    }
}
