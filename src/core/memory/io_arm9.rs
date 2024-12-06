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

    #[inline(never)]
    pub fn read<T: Convert>(&mut self, addr_offset: u32, emu: &mut Emu) -> T {
        match addr_offset & 0xF00000 {
            0x0 if IoArm9ReadLut::is_in_range(addr_offset) => T::from(IoArm9ReadLut::read(addr_offset, size_of::<T>() as u8, emu)),
            0x100000 if IoArm9ReadLutUpper::is_in_range(addr_offset) => T::from(IoArm9ReadLutUpper::read(addr_offset, size_of::<T>() as u8, emu)),
            _ => T::from(0),
        }
    }

    #[inline(never)]
    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T, emu: &mut Emu) {
        if likely(IoArm9WriteLut::is_in_range(addr_offset)) {
            IoArm9WriteLut::write(value.into(), addr_offset, size_of::<T>() as u8, emu);
        }
    }

    #[inline(never)]
    pub fn write_fixed_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T], emu: &mut Emu) {
        if likely(IoArm9WriteLut::is_in_range(addr_offset)) {
            IoArm9WriteLut::write_fixed_slice(addr_offset, slice, emu);
        }
    }
}
