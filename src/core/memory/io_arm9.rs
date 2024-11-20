use crate::core::div_sqrt::DivSqrt;
use crate::core::emu::Emu;
use crate::core::memory::dma::Dma;
use crate::core::memory::io_arm9_lut::{IoArm9ReadLut, IoArm9ReadLutUpper, IoArm9WriteLut};
use crate::core::timers::Timers;
use crate::core::CpuType::ARM9;
use crate::utils::Convert;
use std::intrinsics::likely;

pub struct IoArm9 {
    pub div_sqrt: DivSqrt,
    pub dma: Dma,
    pub timers: Timers,
}

impl IoArm9 {
    pub fn new() -> Self {
        IoArm9 {
            div_sqrt: DivSqrt::new(),
            dma: Dma::new(ARM9),
            timers: Timers::new(ARM9),
        }
    }

    pub fn read<T: Convert>(&mut self, addr_offset: u32, emu: &mut Emu) -> T {
        if likely(IoArm9ReadLut::is_in_range(addr_offset)) {
            T::from(IoArm9ReadLut::read(addr_offset, size_of::<T>() as u8, emu))
        } else if IoArm9ReadLutUpper::is_in_range(addr_offset) {
            T::from(IoArm9ReadLutUpper::read(addr_offset, size_of::<T>() as u8, emu))
        } else {
            T::from(0)
        }
    }

    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T, emu: &mut Emu) {
        IoArm9WriteLut::write(value.into(), addr_offset, size_of::<T>() as u8, emu);
    }

    pub fn write_fixed_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T], emu: &mut Emu) {
        IoArm9WriteLut::write_fixed_slice(addr_offset, slice, emu);
    }
}
