use crate::core::emu::Emu;
use crate::core::memory::io_arm9_lut::*;
use crate::utils::Convert;
use std::intrinsics::{likely, unlikely};

impl Emu {
    pub fn io_arm9_read<T: Convert>(&mut self, addr_offset: u32) -> T {
        unsafe {
            T::from(if likely(io_arm9_read::is_in_range(addr_offset)) {
                io_arm9_read::read(addr_offset, size_of::<T>(), self)
            } else if unlikely(io_arm9_read_upper::is_in_range(addr_offset)) {
                io_arm9_read_upper::read(addr_offset, size_of::<T>(), self)
            } else {
                0
            })
        }
    }

    pub fn io_arm9_write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        if likely(io_arm9_write::is_in_range(addr_offset)) {
            unsafe { io_arm9_write::write(value.into(), addr_offset, size_of::<T>(), self) };
        }
    }

    pub fn io_arm9_write_fixed_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        if likely(io_arm9_write::is_in_range(addr_offset)) {
            for value in slice {
                unsafe { io_arm9_write::write((*value).into(), addr_offset, size_of::<T>(), self) };
            }
        }
    }
}
