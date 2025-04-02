use crate::core::emu::Emu;
use crate::core::memory::io_arm9_lut::{IoArm9ReadLut, IoArm9ReadLutUpper, IoArm9WriteLut};
use crate::utils::Convert;
use std::intrinsics::likely;

impl Emu {
    pub fn io_arm9_read<T: Convert>(&mut self, addr_offset: u32) -> T {
        match addr_offset & 0xF00000 {
            0x0 if IoArm9ReadLut::is_in_range(addr_offset) => T::from(IoArm9ReadLut::read(addr_offset, size_of::<T>() as u8, self)),
            0x100000 if IoArm9ReadLutUpper::is_in_range(addr_offset) => T::from(IoArm9ReadLutUpper::read(addr_offset, size_of::<T>() as u8, self)),
            _ => T::from(0),
        }
    }

    pub fn io_arm9_write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        if likely(IoArm9WriteLut::is_in_range(addr_offset)) {
            IoArm9WriteLut::write(value.into(), addr_offset, size_of::<T>() as u8, self);
        }
    }

    pub fn io_arm9_write_fixed_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        if likely(IoArm9WriteLut::is_in_range(addr_offset)) {
            IoArm9WriteLut::write_fixed_slice(addr_offset, slice, self);
        }
    }
}
