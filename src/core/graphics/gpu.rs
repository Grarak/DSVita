use crate::core::cpu_regs::InterruptFlag;
use crate::core::cycle_manager::{CycleManager, EventType};
use crate::core::emu::{get_arm7_hle_mut, get_cm_mut, get_common_mut, get_cpu_regs_mut, get_mem_mut, io_dma, Emu};
use crate::core::graphics::gpu_2d::registers_2d::Gpu2DRegisters;
use crate::core::graphics::gpu_2d::Gpu2DEngine::{A, B};
use crate::core::graphics::gpu_3d::registers_3d::Gpu3DRegisters;
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::core::hle::arm7_hle::Arm7Hle;
use crate::core::memory::dma::DmaTransferMode;
use crate::core::CpuType;
use crate::core::CpuType::ARM9;
use crate::logging::debug_println;
use bilge::prelude::*;
use std::intrinsics::{likely, unlikely};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub const DISPLAY_WIDTH: usize = 256;
pub const DISPLAY_HEIGHT: usize = 192;

struct FrameRateCounter {
    frame_counter: u16,
    fps: Arc<AtomicU16>,
    last_update: Instant,
    last_frame: Instant,
}

impl FrameRateCounter {
    fn new(fps: Arc<AtomicU16>) -> Self {
        FrameRateCounter {
            frame_counter: 0,
            fps,
            last_update: Instant::now(),
            last_frame: Instant::now(),
        }
    }

    fn on_frame_ready(&mut self, limit_frame: bool) {
        self.frame_counter += 1;
        let now = Instant::now();
        if likely(limit_frame) {
            let diff = now.duration_since(self.last_frame).as_millis();
            if unlikely(diff < 16) {
                thread::sleep(Duration::from_millis(16 - diff as u64));
            }
            self.last_frame = Instant::now();
        }
        if now.duration_since(self.last_update).as_millis() >= 1000 {
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
    v_blank_irq_enable: bool,
    h_blank_irq_enable: bool,
    v_counter_irq_enable: bool,
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
    pub disp_cap_cnt: u32,
    frame_rate_counter: FrameRateCounter,
    pub frame_limit: bool,
    pub arm7_hle: bool,
    pub v_count: u16,
    pub gpu_2d_regs_a: Gpu2DRegisters<{ A }>,
    pub gpu_2d_regs_b: Gpu2DRegisters<{ B }>,
    pub gpu_3d_regs: Gpu3DRegisters,
    pub gpu_renderer: Option<NonNull<GpuRenderer>>,
}

impl Gpu {
    pub fn new(fps: Arc<AtomicU16>) -> Gpu {
        Gpu {
            disp_stat: [DispStat::from(0); 2],
            pow_cnt1: 0,
            disp_cap_cnt: 0,
            frame_rate_counter: FrameRateCounter::new(fps),
            frame_limit: false,
            arm7_hle: false,
            v_count: 0,
            gpu_2d_regs_a: Gpu2DRegisters::default(),
            gpu_2d_regs_b: Gpu2DRegisters::default(),
            gpu_3d_regs: Gpu3DRegisters::default(),
            gpu_renderer: None,
        }
    }

    pub fn initialize_schedule(cycle_manager: &mut CycleManager) {
        cycle_manager.schedule(
            // 8 pixel delay according to https://melonds.kuribo64.net/board/thread.php?id=13
            (256 + 8) * 6,
            EventType::GpuScanline256,
        );
    }

    pub fn get_renderer(&self) -> &GpuRenderer {
        unsafe { self.gpu_renderer.unwrap().as_ref() }
    }

    pub fn get_renderer_mut(&mut self) -> &mut GpuRenderer {
        unsafe { self.gpu_renderer.unwrap().as_mut() }
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

    pub fn on_scanline256_event(emu: &mut Emu) {
        let gpu = &mut get_common_mut!(emu).gpu;

        if gpu.v_count < 192 {
            unsafe {
                gpu.gpu_renderer
                    .unwrap_unchecked()
                    .as_mut()
                    .on_scanline(&mut gpu.gpu_2d_regs_a, &mut gpu.gpu_2d_regs_b, gpu.v_count as u8)
            }
            io_dma!(emu, ARM9).trigger_all(DmaTransferMode::StartAtHBlank, get_cm_mut!(emu));
        }

        for i in 0..2 {
            let disp_stat = &mut gpu.disp_stat[i];
            disp_stat.set_h_blank_flag(u1::new(1));
            if disp_stat.h_blank_irq_enable() {
                get_cpu_regs_mut!(emu, CpuType::from(i as u8)).send_interrupt(InterruptFlag::LcdHBlank, emu);
            }
        }

        get_cm_mut!(emu).schedule((355 - 256) * 6, EventType::GpuScanline355);
    }

    pub fn on_scanline355_event(emu: &mut Emu) {
        let gpu = &mut get_common_mut!(emu).gpu;

        gpu.v_count += 1;
        match gpu.v_count {
            // 3d starts 48 cycles earlier for rendering
            143 => unsafe { gpu.gpu_renderer.unwrap_unchecked().as_mut().renderer_3d.finish_scanline(&gpu.gpu_3d_regs) },
            192 => {
                let pow_cnt1 = PowCnt1::from(gpu.pow_cnt1);
                gpu.get_renderer_mut().on_scanline_finish(get_mem_mut!(emu), pow_cnt1);

                if gpu.gpu_3d_regs.flushed {
                    gpu.gpu_3d_regs.swap_buffers();
                    gpu.get_renderer_mut().renderer_3d.invalidate();
                }

                for i in 0..2 {
                    let disp_stat = &mut gpu.disp_stat[i];
                    disp_stat.set_v_blank_flag(u1::new(1));
                    if disp_stat.v_blank_irq_enable() {
                        get_cpu_regs_mut!(emu, CpuType::from(i as u8)).send_interrupt(InterruptFlag::LcdVBlank, emu);
                        io_dma!(emu, CpuType::from(i as u8)).trigger_all(DmaTransferMode::StartAtVBlank, get_cm_mut!(emu));
                    }
                }
            }
            262 => {
                for i in 0..2 {
                    gpu.disp_stat[i].set_v_blank_flag(u1::new(0));
                }
                gpu.frame_rate_counter.on_frame_ready(gpu.frame_limit);
            }
            263 => {
                gpu.v_count = 0;
                gpu.get_renderer_mut().reload_registers();

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
                if gpu.disp_stat[i].v_counter_irq_enable() {
                    get_cpu_regs_mut!(emu, CpuType::from(i as u8)).send_interrupt(InterruptFlag::LcdVCounterMatch, emu);
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
