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

impl<T> Index<CpuType> for [T] {
    type Output = T;

    fn index(&self, index: CpuType) -> &Self::Output {
        &self[index as usize]
    }
}

impl<T> IndexMut<CpuType> for [T] {
    fn index_mut(&mut self, index: CpuType) -> &mut Self::Output {
        &mut self[index as usize]
    }
}
