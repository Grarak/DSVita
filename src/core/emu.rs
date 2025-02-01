macro_rules! get_common {
    ($emu:expr) => {{
        unsafe {
            std::arch::asm!("");
            $emu.common.get().as_ref_unchecked()
        }
    }};
}
pub(crate) use get_common;

macro_rules! get_common_mut {
    ($emu:expr) => {{
        unsafe {
            std::arch::asm!("");
            $emu.common.get().as_mut_unchecked()
        }
    }};
}
pub(crate) use get_common_mut;

macro_rules! get_mem {
    ($emu:expr) => {{
        unsafe {
            std::arch::asm!("");
            $emu.mem.get().as_ref_unchecked()
        }
    }};
}
pub(crate) use get_mem;

macro_rules! get_mem_mut {
    ($emu:expr) => {{
        unsafe {
            std::arch::asm!("");
            $emu.mem.get().as_mut_unchecked()
        }
    }};
}
pub(crate) use get_mem_mut;

macro_rules! get_regs {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::core::CpuType::ARM9 => &crate::core::emu::get_common!($emu).cpus.thread_regs_arm9,
            crate::core::CpuType::ARM7 => &crate::core::emu::get_common!($emu).cpus.thread_regs_arm7,
        }
    }};
}
pub(crate) use get_regs;

macro_rules! get_regs_mut {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::core::CpuType::ARM9 => &mut crate::core::emu::get_common_mut!($emu).cpus.thread_regs_arm9,
            crate::core::CpuType::ARM7 => &mut crate::core::emu::get_common_mut!($emu).cpus.thread_regs_arm7,
        }
    }};
}
pub(crate) use get_regs_mut;

macro_rules! get_cpu_regs {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::core::CpuType::ARM9 => &crate::core::emu::get_regs!($emu, $cpu).cpu,
            crate::core::CpuType::ARM7 => &crate::core::emu::get_regs!($emu, $cpu).cpu,
        }
    }};
}
pub(crate) use get_cpu_regs;

macro_rules! get_cpu_regs_mut {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::core::CpuType::ARM9 => &mut crate::core::emu::get_regs_mut!($emu, $cpu).cpu,
            crate::core::CpuType::ARM7 => &mut crate::core::emu::get_regs_mut!($emu, $cpu).cpu,
        }
    }};
}
pub(crate) use get_cpu_regs_mut;

macro_rules! get_cp15 {
    ($emu:expr) => {{
        &crate::core::emu::get_common!($emu).cpus.cp15
    }};
}
pub(crate) use get_cp15;

macro_rules! get_cp15_mut {
    ($emu:expr) => {{
        &mut crate::core::emu::get_common_mut!($emu).cpus.cp15
    }};
}
pub(crate) use get_cp15_mut;

macro_rules! io_dma {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::core::CpuType::ARM9 => &crate::core::emu::get_mem!($emu).io_arm9.dma,
            crate::core::CpuType::ARM7 => &crate::core::emu::get_mem!($emu).io_arm7.dma,
        }
    }};
}
pub(crate) use io_dma;

macro_rules! io_dma_mut {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::core::CpuType::ARM9 => &mut crate::core::emu::get_mem_mut!($emu).io_arm9.dma,
            crate::core::CpuType::ARM7 => &mut crate::core::emu::get_mem_mut!($emu).io_arm7.dma,
        }
    }};
}
pub(crate) use io_dma_mut;

macro_rules! io_timers {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::core::CpuType::ARM9 => &crate::core::emu::get_mem!($emu).io_arm9.timers,
            crate::core::CpuType::ARM7 => &crate::core::emu::get_mem!($emu).io_arm7.timers,
        }
    }};
}
pub(crate) use io_timers;

macro_rules! io_timers_mut {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::core::CpuType::ARM9 => &mut crate::core::emu::get_mem_mut!($emu).io_arm9.timers,
            crate::core::CpuType::ARM7 => &mut crate::core::emu::get_mem_mut!($emu).io_arm7.timers,
        }
    }};
}
pub(crate) use io_timers_mut;

macro_rules! io_div_sqrt {
    ($emu:expr) => {{
        &crate::core::emu::get_mem!($emu).io_arm9.div_sqrt
    }};
}
pub(crate) use io_div_sqrt;

macro_rules! io_div_sqrt_mut {
    ($emu:expr) => {{
        &mut crate::core::emu::get_mem_mut!($emu).io_arm9.div_sqrt
    }};
}
pub(crate) use io_div_sqrt_mut;

macro_rules! io_spi {
    ($emu:expr) => {{
        &crate::core::emu::get_mem!($emu).io_arm7.spi
    }};
}
pub(crate) use io_spi;

macro_rules! io_spi_mut {
    ($emu:expr) => {{
        &mut crate::core::emu::get_mem_mut!($emu).io_arm7.spi
    }};
}
pub(crate) use io_spi_mut;

macro_rules! io_rtc {
    ($emu:expr) => {{
        &crate::core::emu::get_mem!($emu).io_arm7.rtc
    }};
}
pub(crate) use io_rtc;

macro_rules! io_rtc_mut {
    ($emu:expr) => {{
        &mut crate::core::emu::get_mem_mut!($emu).io_arm7.rtc
    }};
}
pub(crate) use io_rtc_mut;

macro_rules! io_wifi {
    ($emu:expr) => {{
        &crate::core::emu::get_mem!($emu).io_arm7.wifi
    }};
}
pub(crate) use io_wifi;

macro_rules! io_wifi_mut {
    ($emu:expr) => {{
        &mut crate::core::emu::get_mem_mut!($emu).io_arm7.wifi
    }};
}
pub(crate) use io_wifi_mut;

macro_rules! get_cm {
    ($emu:expr) => {
        &crate::core::emu::get_common!($emu).cycle_manager
    };
}
pub(crate) use get_cm;

macro_rules! get_cm_mut {
    ($emu:expr) => {
        &mut crate::core::emu::get_common_mut!($emu).cycle_manager
    };
}
pub(crate) use get_cm_mut;

macro_rules! get_jit {
    ($emu:expr) => {
        &crate::core::emu::get_mem!($emu).jit
    };
}
pub(crate) use get_jit;

macro_rules! get_jit_mut {
    ($emu:expr) => {
        &mut crate::core::emu::get_mem_mut!($emu).jit
    };
}
pub(crate) use get_jit_mut;

macro_rules! get_mmu {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::core::CpuType::ARM9 => &crate::core::emu::get_mem!($emu).mmu_arm9 as &dyn crate::core::memory::mmu::Mmu,
            crate::core::CpuType::ARM7 => &crate::core::emu::get_mem!($emu).mmu_arm7 as &dyn crate::core::memory::mmu::Mmu,
        }
    }};
}
pub(crate) use get_mmu;

macro_rules! get_spi {
    ($emu:expr) => {{
        &crate::core::emu::get_mem!($emu).io_arm7.spi
    }};
}
pub(crate) use get_spi;

macro_rules! get_spu {
    ($emu:expr) => {{
        &crate::core::emu::get_mem!($emu).io_arm7.spu
    }};
}
pub(crate) use get_spu;

macro_rules! get_spu_mut {
    ($emu:expr) => {{
        &mut crate::core::emu::get_mem_mut!($emu).io_arm7.spu
    }};
}
pub(crate) use get_spu_mut;

macro_rules! get_ipc {
    ($emu:expr) => {
        &crate::core::emu::get_common!($emu).ipc
    };
}
pub(crate) use get_ipc;

macro_rules! get_ipc_mut {
    ($emu:expr) => {
        &mut crate::core::emu::get_common_mut!($emu).ipc
    };
}
pub(crate) use get_ipc_mut;

macro_rules! get_arm7_hle_mut {
    ($emu:expr) => {{
        unsafe {
            std::arch::asm!("");
            $emu.arm7_hle.get().as_mut_unchecked()
        }
    }};
}
pub(crate) use get_arm7_hle_mut;

use crate::cartridge_io::CartridgeIo;
use crate::core::cp15::Cp15;
use crate::core::cycle_manager::CycleManager;
use crate::core::graphics::gpu::Gpu;
use crate::core::hle::arm7_hle::Arm7Hle;
use crate::core::input::Input;
use crate::core::ipc::Ipc;
use crate::core::memory::cartridge::Cartridge;
use crate::core::memory::mem::Memory;
use crate::core::spu::SoundSampler;
use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::settings::Settings;
use crate::utils::Convert;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU16, AtomicU32};
use std::sync::Arc;

pub struct Cpus {
    pub thread_regs_arm9: ThreadRegs,
    pub cp15: Cp15,
    pub thread_regs_arm7: ThreadRegs,
}

impl Cpus {
    fn new() -> Self {
        Cpus {
            thread_regs_arm9: ThreadRegs::new(ARM9),
            cp15: Cp15::new(),
            thread_regs_arm7: ThreadRegs::new(ARM7),
        }
    }
}

pub struct Common {
    pub ipc: Ipc,
    pub cartridge: Cartridge,
    pub gpu: Gpu,
    pub cycle_manager: CycleManager,
    pub cpus: Cpus,
    pub input: Input,
}

impl Common {
    fn new(cartridge_io: CartridgeIo, fps: Arc<AtomicU16>, key_map: Arc<AtomicU32>) -> Self {
        Common {
            ipc: Ipc::new(),
            cartridge: Cartridge::new(cartridge_io),
            gpu: Gpu::new(fps),
            cycle_manager: CycleManager::new(),
            cpus: Cpus::new(),
            input: Input::new(key_map),
        }
    }
}

pub struct Emu {
    pub common: UnsafeCell<Common>,
    pub mem: UnsafeCell<Memory>,
    pub arm7_hle: UnsafeCell<Arm7Hle>,
    pub settings: Settings,
}

impl Emu {
    pub fn new(cartridge_io: CartridgeIo, fps: Arc<AtomicU16>, key_map: Arc<AtomicU32>, touch_points: Arc<AtomicU16>, sound_sampler: Arc<SoundSampler>, settings: Settings) -> Self {
        Emu {
            common: UnsafeCell::new(Common::new(cartridge_io, fps, key_map)),
            mem: UnsafeCell::new(Memory::new(&settings, touch_points, sound_sampler)),
            arm7_hle: UnsafeCell::new(Arm7Hle::new()),
            settings,
        }
    }

    pub fn mem_read<const CPU: CpuType, T: Convert>(&mut self, addr: u32) -> T {
        get_mem_mut!(self).read::<CPU, T>(addr, self)
    }

    pub fn mem_read_no_tcm<const CPU: CpuType, T: Convert>(&mut self, addr: u32) -> T {
        get_mem_mut!(self).read_no_tcm::<CPU, T>(addr, self)
    }

    pub fn mem_read_with_options<const CPU: CpuType, const TCM: bool, T: Convert>(&mut self, addr: u32) -> T {
        get_mem_mut!(self).read_with_options::<CPU, TCM, T>(addr, self)
    }

    pub fn mem_write<const CPU: CpuType, T: Convert>(&mut self, addr: u32, value: T) {
        get_mem_mut!(self).write::<CPU, T>(addr, value, self);
    }

    pub fn mem_write_no_tcm<const CPU: CpuType, T: Convert>(&mut self, addr: u32, value: T) {
        get_mem_mut!(self).write_no_tcm::<CPU, T>(addr, value, self);
    }
}
