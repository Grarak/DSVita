use std::intrinsics::likely;

use crate::core::emu::Emu;
use crate::core::memory::io_arm9_lut::*;
use crate::utils::Convert;

impl Emu {
    pub fn io_arm9_read<T: Convert>(&mut self, addr_offset: u32) -> T {
        unsafe {
            T::from(if likely(io_arm9::is_in_range(addr_offset)) {
                io_arm9::read(addr_offset, size_of::<T>(), self)
            } else if likely(io_arm9_upper::is_in_range(addr_offset)) {
                io_arm9_upper::read(addr_offset, size_of::<T>(), self)
            } else {
                0
            })
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
