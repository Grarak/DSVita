use crate::core::CpuType::{ARM7, ARM9};
use std::marker::ConstParamTy;
use std::ops;
use std::ops::{Index, IndexMut};

pub mod cp15;
pub mod cpu_regs;
pub mod cycle_manager;
mod div_sqrt;
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

const GUEST_REGS_ARM9_ADDR: usize = 0xA0000000;
const GUEST_REGS_ARM7_ADDR: usize = 0xA8000000;

const MMU_TCM_ARM9_ADDR: usize = 0xB0000000;
const MMU_TCM_ARM7_ADDR: usize = 0xC0000000;

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

    pub const fn guest_regs_addr(self) -> usize {
        match self {
            ARM9 => GUEST_REGS_ARM9_ADDR,
            ARM7 => GUEST_REGS_ARM7_ADDR,
        }
    }

    pub const fn mmu_tcm_addr(self) -> usize {
        match self {
            ARM9 => MMU_TCM_ARM9_ADDR,
            ARM7 => MMU_TCM_ARM7_ADDR,
        }
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
