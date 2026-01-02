use crate::core::cp15::Cp15;
use crate::core::cpu_regs::CpuRegs;
use crate::core::cycle_manager::CycleManager;
use crate::core::div_sqrt::DivSqrt;
use crate::core::graphics::gpu::Gpu;
use crate::core::hle::arm7_hle::Arm7Hle;
use crate::core::input::Input;
use crate::core::ipc::Ipc;
use crate::core::memory::cartridge::Cartridge;
use crate::core::memory::dma::Dma;
use crate::core::memory::mem::Memory;
use crate::core::rtc::Rtc;
use crate::core::spi::{MicSampler, Spi};
use crate::core::spu::{SoundSampler, Spu};
use crate::core::thread_regs::ThreadRegs;
use crate::core::timers::Timers;
use crate::core::wifi::Wifi;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::jit_memory::JitMemory;
use crate::settings::{Settings, DEFAULT_SETTINGS};
use bilge::prelude::*;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU16, AtomicU32};
use std::sync::{Arc, Mutex};

#[bitsize(32)]
#[derive(Copy, Clone, DebugBits, FromBits)]
pub struct NitroSdkVersion {
    relstep: u16,
    minor: u8,
    major: u8,
}

impl NitroSdkVersion {
    pub fn is_valid(self) -> bool {
        u32::from(self) != u32::MAX
    }

    pub fn rely_on_fs_invalidation(self) -> bool {
        self.major() < 5
    }

    pub fn is_twl_sdk(self) -> bool {
        self.major() >= 5
    }
}

impl Default for NitroSdkVersion {
    fn default() -> Self {
        NitroSdkVersion::from(u32::MAX)
    }
}

pub struct Emu {
    pub ipc: Ipc,
    pub cartridge: Cartridge,
    pub gpu: Gpu,
    pub cm: CycleManager,
    pub cpu: [CpuRegs; 2],
    pub cp15: Cp15,
    pub input: Input,
    pub mem: Memory,
    pub hle: Arm7Hle,
    pub div_sqrt: DivSqrt,
    pub spi: Spi,
    pub rtc: Rtc,
    pub spu: Spu,
    pub dma: [Dma; 2],
    pub timers: [Timers; 2],
    pub wifi: Wifi,
    pub jit: JitMemory,
    pub settings: Settings,
    pub nitro_sdk_version: NitroSdkVersion,
    pub os_irq_table_addr: u32,
    pub os_irq_handler_thread_switch_addr: u32,
    pub fs_clear_overlay_image_addr: u32,
    pub breakout_imm: bool,
    initialized: bool,
}

impl Emu {
    pub fn new(fps: Arc<AtomicU16>, key_map: Arc<AtomicU32>, touch_points: Arc<AtomicU16>, mic_sampler: Arc<Mutex<MicSampler>>, sound_sampler: NonNull<SoundSampler>, jit: JitMemory) -> Self {
        Emu {
            ipc: Ipc::new(),
            cartridge: Cartridge::new(),
            gpu: Gpu::new(fps),
            cm: CycleManager::new(),
            cpu: [CpuRegs::new(), CpuRegs::new()],
            cp15: Cp15::new(),
            input: Input::new(key_map),
            mem: Memory::new(),
            hle: Arm7Hle::new(),
            div_sqrt: DivSqrt::new(),
            spi: Spi::new(touch_points, mic_sampler),
            rtc: Rtc::new(),
            spu: Spu::new(sound_sampler),
            dma: [Dma::new(), Dma::new()],
            timers: [Timers::new(), Timers::new()],
            wifi: Wifi::new(),
            jit,
            settings: DEFAULT_SETTINGS.clone(),
            nitro_sdk_version: NitroSdkVersion::default(),
            os_irq_table_addr: 0,
            os_irq_handler_thread_switch_addr: 0,
            fs_clear_overlay_image_addr: 0,
            breakout_imm: false,
            initialized: true,
        }
    }

    pub fn reset(&mut self) {
        self.jit.init(&self.settings);
        self.ipc.init(&self.settings);
        self.spi.init(&self.settings);
        if !self.initialized {
            *ARM9.thread_regs() = ThreadRegs::default();
            *ARM7.thread_regs() = ThreadRegs::default();
            self.gpu.init();
            self.cm.init();
            self.cpu = [CpuRegs::new(), CpuRegs::new()];
            self.cp15 = Cp15::new();
            self.mem.init();
            self.hle = Arm7Hle::new();
            self.div_sqrt = DivSqrt::new();
            self.rtc = Rtc::new();
            self.spu.init();
            self.dma = [Dma::new(), Dma::new()];
            self.timers = [Timers::new(), Timers::new()];
            self.wifi = Wifi::new();
        }
        self.nitro_sdk_version = NitroSdkVersion::default();
        self.os_irq_table_addr = 0;
        self.os_irq_handler_thread_switch_addr = 0;
        self.fs_clear_overlay_image_addr = 0;
        self.initialized = false;
    }
}
