use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType::{ARM7, ARM9};
use std::marker::ConstParamTy;
use std::ops;
use std::ops::{Index, IndexMut};

mod blow_mic_data;
pub mod cp15;
pub mod cpu_regs;
pub mod cycle_manager;
pub mod div_sqrt;
pub mod emu;
pub mod exception_handler;
pub mod graphics;
pub mod hle;
pub mod input;
pub mod ipc;
pub mod memory;
pub mod rtc;
pub mod spi;
pub mod spu;
pub mod thread_regs;
pub mod timers;
mod wifi;

// The guest-facing regions (ThreadRegs, JitAsm, the fastmem reservations) need
// process-lifetime-stable bases. On 32-bit hosts they must also be *compile-time*
// constants below 4 GiB: the arm32 backend bakes them as u32 asm/emitter immediates.
// The a64 backend materializes whatever pointer it is handed with movz/movk at emit
// time, so on aarch64 the bases are simply wherever the kernel put the startup mmaps
// (set_*_addr below) — mandatory on Android, where the low VA space belongs to
// ART/zygote and no fixed address is dependable.
#[cfg(target_arch = "aarch64")]
mod region_addrs {
    // Written once at startup before any thread spawns, read-only afterwards.
    pub static mut GUEST_REGS: [usize; 2] = [0; 2];
    pub static mut JIT_ASM: [usize; 2] = [0; 2];
    pub static mut MMU_TCM: [usize; 2] = [0; 2];
}

#[cfg(not(target_arch = "aarch64"))]
const GUEST_REGS_ARM9_ADDR: usize = if cfg!(target_os = "vita") { 0xA0000000 } else { 0xA0000000 };
#[cfg(not(target_arch = "aarch64"))]
const GUEST_REGS_ARM7_ADDR: usize = if cfg!(target_os = "vita") { 0xA8000000 } else { 0xA1000000 };

#[cfg(not(target_arch = "aarch64"))]
const JIT_ASM_ARM9_ADDR: usize = if cfg!(target_os = "vita") { 0xA1000000 } else { 0x70000000 };
#[cfg(not(target_arch = "aarch64"))]
const JIT_ASM_ARM7_ADDR: usize = if cfg!(target_os = "vita") { 0xA2000000 } else { 0x71000000 };

#[cfg(not(target_arch = "aarch64"))]
const MMU_TCM_ARM9_ADDR: usize = if cfg!(target_os = "vita") { 0xB0000000 } else { 0x80000000 };
#[cfg(not(target_arch = "aarch64"))]
const MMU_TCM_ARM7_ADDR: usize = if cfg!(target_os = "vita") { 0xC0000000 } else { 0x90000000 };

#[derive(ConstParamTy, Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CpuType {
    ARM9 = 0,
    ARM7 = 1,
}

impl CpuType {
    pub const fn other(self) -> Self {
        match self {
            ARM9 => ARM7,
            ARM7 => ARM9,
        }
    }

    #[cfg(not(target_arch = "aarch64"))]
    pub const fn guest_regs_addr(self) -> usize {
        match self {
            ARM9 => GUEST_REGS_ARM9_ADDR,
            ARM7 => GUEST_REGS_ARM7_ADDR,
        }
    }

    #[cfg(target_arch = "aarch64")]
    pub fn guest_regs_addr(self) -> usize {
        unsafe { region_addrs::GUEST_REGS[self as usize] }
    }

    // The set_*_addr calls sit right after the region mmaps in actual_main/Mmu::new,
    // before any thread spawns. On const-address hosts they are pure sanity checks.
    #[cfg(not(target_arch = "aarch64"))]
    pub fn set_guest_regs_addr(self, addr: usize) {
        debug_assert_eq!(addr, self.guest_regs_addr());
    }

    #[cfg(target_arch = "aarch64")]
    pub fn set_guest_regs_addr(self, addr: usize) {
        unsafe { region_addrs::GUEST_REGS[self as usize] = addr };
    }

    pub fn thread_regs(self) -> &'static mut ThreadRegs {
        unsafe { (self.guest_regs_addr() as *mut ThreadRegs).as_mut_unchecked() }
    }

    #[cfg(not(target_arch = "aarch64"))]
    pub const fn jit_asm_addr(self) -> usize {
        match self {
            ARM9 => JIT_ASM_ARM9_ADDR,
            ARM7 => JIT_ASM_ARM7_ADDR,
        }
    }

    #[cfg(target_arch = "aarch64")]
    pub fn jit_asm_addr(self) -> usize {
        unsafe { region_addrs::JIT_ASM[self as usize] }
    }

    #[cfg(not(target_arch = "aarch64"))]
    pub fn set_jit_asm_addr(self, addr: usize) {
        debug_assert_eq!(addr, self.jit_asm_addr());
    }

    #[cfg(target_arch = "aarch64")]
    pub fn set_jit_asm_addr(self, addr: usize) {
        unsafe { region_addrs::JIT_ASM[self as usize] = addr };
    }

    #[cfg(not(target_arch = "aarch64"))]
    pub const fn mmu_tcm_addr(self) -> usize {
        match self {
            ARM9 => MMU_TCM_ARM9_ADDR,
            ARM7 => MMU_TCM_ARM7_ADDR,
        }
    }

    #[cfg(target_arch = "aarch64")]
    pub fn mmu_tcm_addr(self) -> usize {
        unsafe { region_addrs::MMU_TCM[self as usize] }
    }

    #[cfg(not(target_arch = "aarch64"))]
    pub fn set_mmu_tcm_addr(self, addr: usize) {
        debug_assert_eq!(addr, self.mmu_tcm_addr());
    }

    #[cfg(target_arch = "aarch64")]
    pub fn set_mmu_tcm_addr(self, addr: usize) {
        unsafe { region_addrs::MMU_TCM[self as usize] = addr };
    }

    pub const fn max_loop_cycle_count(self) -> u32 {
        match self {
            ARM9 => 255,
            ARM7 => 128,
        }
    }

    pub const fn max_branch_loop_cycle_count(self) -> u32 {
        128
    }
}

impl From<bool> for CpuType {
    fn from(value: bool) -> Self {
        match value {
            false => ARM9,
            true => ARM7,
        }
    }
}

impl From<u8> for CpuType {
    fn from(value: u8) -> Self {
        CpuType::from(value != 0)
    }
}

impl ops::Not for CpuType {
    type Output = Self;

    fn not(self) -> Self::Output {
        self.other()
    }
}

impl<T> Index<CpuType> for [T; 2] {
    type Output = T;

    fn index(&self, index: CpuType) -> &Self::Output {
        &self[index as usize]
    }
}

impl<T> IndexMut<CpuType> for [T; 2] {
    fn index_mut(&mut self, index: CpuType) -> &mut Self::Output {
        &mut self[index as usize]
    }
}
