use std::marker::ConstParamTy;
use std::ops;
use std::ops::{Index, IndexMut};

pub mod bios_context;
mod bios_lookup_table;
pub mod cp15_context;
pub mod cpu_regs;
pub mod cycle_manager;
mod div_sqrt_context;
pub mod exception_handler;
pub mod gpu;
pub mod input_context;
pub mod ipc_handler;
pub mod memory;
pub mod rtc_context;
pub mod spi_context;
pub mod spu_context;
pub mod thread_context;
pub mod thread_regs;
pub mod timers_context;

#[derive(ConstParamTy, Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CpuType {
    ARM9 = 0,
    ARM7 = 1,
}

impl CpuType {
    pub const fn other(self) -> Self {
        match self {
            CpuType::ARM9 => CpuType::ARM7,
            CpuType::ARM7 => CpuType::ARM9,
        }
    }
}

impl const ops::Not for CpuType {
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
