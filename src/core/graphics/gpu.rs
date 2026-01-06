use crate::core::cpu_regs::InterruptFlag;
use crate::core::cycle_manager::{CycleManager, EventType};
use crate::core::emu::Emu;
use crate::core::graphics::gpu_2d::registers_2d::Gpu2DRegisters;
use crate::core::graphics::gpu_2d::Gpu2DEngine::{A, B};
use crate::core::graphics::gpu_3d::registers_3d::Gpu3DRegisters;
use crate::core::graphics::gpu_mem_buf::GpuMemBuf;
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::core::memory::dma::DmaTransferMode;
use crate::core::memory::vram::VramBanks;
use crate::core::CpuType;
use crate::core::CpuType::ARM9;
use crate::logging::debug_println;
use crate::settings::Arm7Emu;
use crate::utils::PtrWrapper;
use bilge::prelude::*;
use std::intrinsics::unlikely;
use std::mem;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub const DISPLAY_WIDTH: usize = 256;
pub const DISPLAY_HEIGHT: usize = 192;

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
        if unlikely(now.duration_since(self.last_update).as_millis() >= 1000) {
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
    v_blank_flag: bool,
    h_blank_flag: bool,
    v_counter_flag: bool,
    v_blank_irq_enable: bool,
    h_blank_irq_enable: bool,
    v_counter_irq_enable: bool,
    not_used: u1,
    v_count_msb: bool,
    v_count_setting: u8,
}

#[bitsize(16)]
#[derive(Copy, Clone, Default, Eq, FromBits, PartialEq)]
pub struct PowCnt1 {
    pub enable: bool,
    gpu_2d_a_enable: bool,
    rendering_3d_enable: bool,
    geometry_3d_enable: bool,
    not_used: u5,
    gpu_2d_b_enable: bool,
    not_used1: u5,
    pub display_swap: bool,
}

#[bitsize(32)]
#[derive(Copy, Clone, FromBits)]
pub struct DispCapCnt {
    pub eva: u5,
    not_used: u3,
    pub evb: u5,
    not_used2: u3,
    pub vram_write_block: u2,
    pub vram_write_offset: u2,
    pub capture_size: u2,
    not_used3: u2,
    pub source_a: bool,
    pub source_b: bool,
    pub vram_read_offset: u2,
    not_used4: u1,
    pub capture_source: u2,
    pub capture_enabled: bool,
}

impl DispCapCnt {
    pub fn size(self) -> (u16, u16) {
        const SIZES: [(u16, u16); 4] = [(128, 128), (256, 64), (256, 128), (256, 192)];
        SIZES[u8::from(self.capture_size()) as usize]
    }

    pub fn pixel_size(self) -> usize {
        let (width, height) = self.size();
        width as usize * height as usize
    }

    pub fn write_offset(self) -> u32 {
        u8::from(self.vram_write_offset()) as u32 * 0x8000
    }
}

pub struct Gpu {
    disp_stat: [DispStat; 2],
    pub pow_cnt1: PowCnt1,
    pub disp_cap_cnt: DispCapCnt,
    frame_rate_counter: FrameRateCounter,
    pub v_count: u16,
    pub gpu_2d_regs_a: Gpu2DRegisters,
    pub gpu_2d_regs_b: Gpu2DRegisters,
    pub gpu_3d_regs: Gpu3DRegisters,
    pub renderer: PtrWrapper<GpuRenderer>,
}

impl Gpu {
    pub fn new(fps: Arc<AtomicU16>) -> Gpu {
        Gpu {
            disp_stat: [DispStat::from(0); 2],
            pow_cnt1: PowCnt1::from(0),
            disp_cap_cnt: DispCapCnt::from(0),
            frame_rate_counter: FrameRateCounter::new(fps),
            v_count: 0,
            gpu_2d_regs_a: Gpu2DRegisters::new(A),
            gpu_2d_regs_b: Gpu2DRegisters::new(B),
            gpu_3d_regs: Gpu3DRegisters::new(),
            renderer: PtrWrapper::null(),
        }
    }

    pub fn init(&mut self) {
        self.disp_stat = [DispStat::from(0); 2];
        self.pow_cnt1 = PowCnt1::from(0);
        self.disp_cap_cnt = DispCapCnt::from(0);
        self.v_count = 0;
        self.gpu_2d_regs_a = Gpu2DRegisters::new(A);
        self.gpu_2d_regs_b = Gpu2DRegisters::new(B);
        self.gpu_3d_regs = Gpu3DRegisters::new();
    }

    pub fn set_gpu_renderer(&mut self, gpu_renderer: NonNull<GpuRenderer>) {
        self.renderer = PtrWrapper::new(gpu_renderer.as_ptr());
    }

    pub fn initialize_schedule(cm: &mut CycleManager) {
        cm.schedule(
            // 8 pixel delay according to https://melonds.kuribo64.net/board/thread.php?id=13
            (256 + 8) * 6,
            EventType::GpuScanline256,
        );
    }

    pub fn get_disp_stat(&self, cpu: CpuType) -> u16 {
        self.disp_stat[cpu].into()
    }

    pub fn get_pow_cnt1(&self) -> u16 {
        self.pow_cnt1.into()
    }

    pub const fn get_disp_stat_offset(cpu: CpuType) -> usize {
        mem::offset_of!(Emu, gpu.disp_stat) + cpu as usize * size_of::<DispStat>()
    }

    pub fn get_disp_cap_cnt(&self) -> u32 {
        self.disp_cap_cnt.into()
    }

    pub fn set_disp_stat(&mut self, cpu: CpuType, mut mask: u16, value: u16) {
        mask &= 0xFFB8;
        self.disp_stat[cpu] = ((u16::from(self.disp_stat[cpu]) & !mask) | (value & mask)).into();
    }

    pub fn set_pow_cnt1(&mut self, mut mask: u16, value: u16) {
        mask &= 0x820F;
        self.pow_cnt1.value = (self.pow_cnt1.value & !mask) | (value & mask);
    }

    pub fn set_disp_cap_cnt(&mut self, mut mask: u32, value: u32) {
        mask &= 0xEF3F1F1F;
        self.disp_cap_cnt = ((u32::from(self.disp_cap_cnt) & !mask) | (value & mask)).into();
    }
}

impl Emu {
    pub fn gpu_on_scanline256_event(&mut self) {
        if self.gpu.v_count < 192 {
            self.gpu.renderer.on_scanline(&mut self.gpu.gpu_2d_regs_a, &mut self.gpu.gpu_2d_regs_b, self.gpu.v_count as u8);
            self.dma_trigger_all(ARM9, DmaTransferMode::StartAtHBlank);
        }

        for i in 0..2 {
            let disp_stat = &mut self.gpu.disp_stat[i];
            disp_stat.set_h_blank_flag(true);
            if disp_stat.h_blank_irq_enable() {
                self.cpu_send_interrupt(CpuType::from(i as u8), InterruptFlag::LcdHBlank);
            }
        }

        self.cm.schedule((355 - 256) * 6, EventType::GpuScanline355);
    }

    pub fn gpu_on_scanline355_event(&mut self) {
        self.gpu.v_count += 1;
        match self.gpu.v_count {
            1 => self.gpu.gpu_3d_regs.on_first_scanline(self.gpu.pow_cnt1),
            192 => {
                let palettes = self.mem_get_palettes();
                let oam = self.mem_get_oam();

                self.gpu
                    .renderer
                    .on_scanline_finish(palettes, oam, self.gpu.pow_cnt1, self.gpu.disp_cap_cnt, &mut self.gpu.gpu_3d_regs, &mut self.breakout_imm);

                if self.gpu.gpu_3d_regs.flushed {
                    self.gpu.gpu_3d_regs.swap_buffers(self.settings.geometry_3d_skip());
                    self.gpu.renderer.renderer_3d.invalidate();
                }

                for i in 0..2 {
                    let disp_stat = &mut self.gpu.disp_stat[i];
                    disp_stat.set_v_blank_flag(true);
                    if disp_stat.v_blank_irq_enable() {
                        self.cpu_send_interrupt(CpuType::from(i as u8), InterruptFlag::LcdVBlank);
                        self.dma_trigger_all(CpuType::from(i as u8), DmaTransferMode::StartAtVBlank);
                    }
                }

                let vram = self.vram_get_mem_mut();
                if self.gpu.disp_cap_cnt.capture_enabled() && u8::from(self.gpu.disp_cap_cnt.capture_source()) != 1 {
                    let bank_num = u8::from(self.gpu.disp_cap_cnt.vram_write_block());
                    let (vram_offset, _) = VramBanks::get(bank_num);
                    let bank = unsafe { mem::transmute(vram.as_mut_ptr().add(vram_offset)) };
                    GpuMemBuf::mark_block_as_captured(self.gpu.disp_cap_cnt, bank);
                } else {
                    GpuMemBuf::unmark_blocks_as_captured(vram);
                }

                self.gpu.disp_cap_cnt.set_capture_enabled(false);
            }
            262 => {
                for i in 0..2 {
                    self.gpu.disp_stat[i].set_v_blank_flag(false);
                }
                self.gpu.renderer.reload_registers(&self.mem.vram);
            }
            263 => {
                self.gpu.frame_rate_counter.on_frame_ready();
                self.gpu.v_count = 0;
                if self.settings.arm7_emu() == Arm7Emu::Hle {
                    self.arm7_hle_on_frame();
                }
            }
            _ => {}
        }

        for i in 0..2 {
            let v_match = (u16::from(self.gpu.disp_stat[i].v_count_msb()) << 8) | self.gpu.disp_stat[i].v_count_setting() as u16;
            debug_println!("v match {:x} {} {}", u16::from(self.gpu.disp_stat[i]), v_match, self.gpu.v_count);
            if self.gpu.v_count == v_match {
                self.gpu.disp_stat[i].set_v_counter_flag(true);
                if self.gpu.disp_stat[i].v_counter_irq_enable() {
                    self.cpu_send_interrupt(CpuType::from(i as u8), InterruptFlag::LcdVCounterMatch);
                }
            } else {
                self.gpu.disp_stat[i].set_v_counter_flag(false);
            }
            self.gpu.disp_stat[i].set_h_blank_flag(false);
        }

        if self.settings.arm7_emu() != Arm7Emu::AccurateLle {
            self.arm7_hle_on_scanline(self.gpu.v_count);
        }

        self.cm.schedule(256 * 6, EventType::GpuScanline256);
    }
}
