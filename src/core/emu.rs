use crate::cartridge_io::CartridgeIo;
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
use crate::core::spi::Spi;
use crate::core::spu::{SoundSampler, Spu};
use crate::core::thread_regs::ThreadRegs;
use crate::core::timers::Timers;
use crate::core::wifi::Wifi;
use crate::jit::jit_memory::JitMemory;
use crate::settings::Settings;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU16, AtomicU32};
use std::sync::Arc;

pub struct Emu {
    pub ipc: Ipc,
    pub cartridge: Cartridge,
    pub gpu: Gpu,
    pub cm: CycleManager,
    pub thread: [&'static mut ThreadRegs; 2],
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
    pub breakout_imm: bool,
    pub settings: Settings,
}

impl Emu {
    pub fn new(
        thread_regs: [&'static mut ThreadRegs; 2],
        cartridge_io: CartridgeIo,
        fps: Arc<AtomicU16>,
        key_map: Arc<AtomicU32>,
        touch_points: Arc<AtomicU16>,
        sound_sampler: NonNull<SoundSampler>,
        jit: JitMemory,
        settings: Settings,
    ) -> Self {
        Emu {
            ipc: Ipc::new(&settings),
            cartridge: Cartridge::new(cartridge_io),
            gpu: Gpu::new(fps),
            cm: CycleManager::new(),
            thread: thread_regs,
            cpu: [CpuRegs::new(), CpuRegs::new()],
            cp15: Cp15::new(),
            input: Input::new(key_map),
            mem: Memory::new(),
            hle: Arm7Hle::new(),
            div_sqrt: DivSqrt::new(),
            spi: Spi::new(touch_points),
            rtc: Rtc::new(),
            spu: Spu::new(sound_sampler),
            dma: [Dma::new(), Dma::new()],
            timers: [Timers::new(), Timers::new()],
            wifi: Wifi::new(),
            jit,
            breakout_imm: false,
            settings,
        }
    }
}
