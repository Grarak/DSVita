use crate::cartridge_reader::CartridgeReader;
use crate::emu::cpu::{CpuArm7, CpuArm9};
use crate::emu::cycle_manager::CycleManager;
use crate::emu::gpu::gpu::{Gpu, Swapchain};
use crate::emu::input::Input;
use crate::emu::ipc::Ipc;
use crate::emu::memory::cartridge::Cartridge;
use crate::emu::memory::mem::Memory;
use crate::emu::CpuType;
use crate::utils::Convert;
use std::ptr;
use std::sync::atomic::AtomicU16;
use std::sync::Arc;

macro_rules! get_regs {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::emu::CpuType::ARM9 => $emu.common.cpus.arm9.regs(),
            crate::emu::CpuType::ARM7 => $emu.common.cpus.arm7.regs(),
        }
    }};
}
pub(crate) use get_regs;

macro_rules! get_regs_mut {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::emu::CpuType::ARM9 => $emu.common.cpus.arm9.regs_mut(),
            crate::emu::CpuType::ARM7 => $emu.common.cpus.arm7.regs_mut(),
        }
    }};
}
pub(crate) use get_regs_mut;

macro_rules! get_cpu_regs {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::emu::CpuType::ARM9 => &$emu.common.cpus.arm9.regs().cpu,
            crate::emu::CpuType::ARM7 => &$emu.common.cpus.arm7.regs().cpu,
        }
    }};
}
pub(crate) use get_cpu_regs;

macro_rules! get_cpu_regs_mut {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::emu::CpuType::ARM9 => &mut $emu.common.cpus.arm9.regs_mut().cpu,
            crate::emu::CpuType::ARM7 => &mut $emu.common.cpus.arm7.regs_mut().cpu,
        }
    }};
}
pub(crate) use get_cpu_regs_mut;

macro_rules! get_cp15 {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::emu::CpuType::ARM9 => $emu.common.cpus.arm9.cp15(),
            crate::emu::CpuType::ARM7 => $emu.common.cpus.arm7.cp15(),
        }
    }};
}
pub(crate) use get_cp15;

macro_rules! get_cp15_mut {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::emu::CpuType::ARM9 => $emu.common.cpus.arm9.cp15_mut(),
            crate::emu::CpuType::ARM7 => $emu.common.cpus.arm7.cp15_mut(),
        }
    }};
}
pub(crate) use get_cp15_mut;

macro_rules! io_dma {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::emu::CpuType::ARM9 => &$emu.mem.io_arm9.dma,
            crate::emu::CpuType::ARM7 => &$emu.mem.io_arm7.dma,
        }
    }};
}
pub(crate) use io_dma;

macro_rules! io_dma_mut {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::emu::CpuType::ARM9 => &mut $emu.mem.io_arm9.dma,
            crate::emu::CpuType::ARM7 => &mut $emu.mem.io_arm7.dma,
        }
    }};
}
pub(crate) use io_dma_mut;

macro_rules! io_timers {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::emu::CpuType::ARM9 => &$emu.mem.io_arm9.timers,
            crate::emu::CpuType::ARM7 => &$emu.mem.io_arm7.timers,
        }
    }};
}
pub(crate) use io_timers;

macro_rules! io_timers_mut {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::emu::CpuType::ARM9 => &mut $emu.mem.io_arm9.timers,
            crate::emu::CpuType::ARM7 => &mut $emu.mem.io_arm7.timers,
        }
    }};
}
pub(crate) use io_timers_mut;

macro_rules! get_cm {
    ($emu:expr) => {
        &$emu.common.cycle_manager
    };
}
pub(crate) use get_cm;

macro_rules! get_jit {
    ($emu:expr) => {
        &$emu.mem.jit
    };
}
pub(crate) use get_jit;

macro_rules! get_jit_mut {
    ($emu:expr) => {
        &mut $emu.mem.jit
    };
}
pub(crate) use get_jit_mut;

macro_rules! get_mmu {
    ($emu:expr, $cpu:expr) => {{
        match $cpu {
            crate::emu::CpuType::ARM9 => &$emu.mem.mmu_arm9 as &dyn crate::emu::memory::mmu::Mmu,
            crate::emu::CpuType::ARM7 => &$emu.mem.mmu_arm7 as &dyn crate::emu::memory::mmu::Mmu,
        }
    }};
}
pub(crate) use get_mmu;

pub struct Cpus {
    pub arm9: CpuArm9,
    pub arm7: CpuArm7,
}

impl Cpus {
    fn new() -> Self {
        Cpus {
            arm9: CpuArm9::new(),
            arm7: CpuArm7::new(),
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
    fn new(
        cartridge_reader: CartridgeReader,
        swapchain: Arc<Swapchain>,
        fps: Arc<AtomicU16>,
        key_map: Arc<AtomicU16>,
    ) -> Self {
        Common {
            ipc: Ipc::new(),
            cartridge: Cartridge::new(cartridge_reader),
            gpu: Gpu::new(swapchain, fps),
            cycle_manager: CycleManager::new(),
            cpus: Cpus::new(),
            input: Input::new(key_map),
        }
    }
}

pub struct Emu {
    pub common: Common,
    pub mem: Memory,
}

impl Emu {
    pub fn new(
        cartridge_reader: CartridgeReader,
        swapchain: Arc<Swapchain>,
        fps: Arc<AtomicU16>,
        key_map: Arc<AtomicU16>,
    ) -> Self {
        Emu {
            common: Common::new(cartridge_reader, swapchain, fps, key_map),
            mem: Memory::new(),
        }
    }

    pub fn mem_read<const CPU: CpuType, T: Convert>(&mut self, addr: u32) -> T {
        let mem = ptr::addr_of_mut!(self.mem);
        unsafe { (*mem).read::<CPU, T>(addr, self) }
    }

    pub fn mem_read_no_tcm<const CPU: CpuType, T: Convert>(&mut self, addr: u32) -> T {
        let mem = ptr::addr_of_mut!(self.mem);
        unsafe { (*mem).read_no_tcm::<CPU, T>(addr, self) }
    }

    pub fn mem_read_with_options<
        const CPU: CpuType,
        const TCM: bool,
        const MMU: bool,
        T: Convert,
    >(
        &mut self,
        addr: u32,
    ) -> T {
        let mem = ptr::addr_of_mut!(self.mem);
        unsafe { (*mem).read_with_options::<CPU, TCM, MMU, T>(addr, self) }
    }

    pub fn mem_write<const CPU: CpuType, T: Convert>(&mut self, addr: u32, value: T) {
        let mem = ptr::addr_of_mut!(self.mem);
        unsafe { (*mem).write::<CPU, T>(addr, value, self) };
    }

    pub fn mem_write_no_tcm<const CPU: CpuType, T: Convert>(&mut self, addr: u32, value: T) {
        let mem = ptr::addr_of_mut!(self.mem);
        unsafe { (*mem).write_no_tcm::<CPU, T>(addr, value, self) };
    }
}
