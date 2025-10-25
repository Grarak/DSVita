use crate::core::emu::Emu;
use crate::core::memory::io_arm7_lut::*;
use crate::utils::Convert;
use std::intrinsics::{likely, unlikely};

impl Emu {
    pub fn io_arm7_read<T: Convert>(&mut self, addr_offset: u32) -> T {
        unsafe {
            T::from(if likely(io_arm7_read::is_in_range(addr_offset)) {
                io_arm7_read::read(addr_offset, size_of::<T>(), self)
            } else if unlikely(io_arm7_read_upper::is_in_range(addr_offset)) {
                io_arm7_read_upper::read(addr_offset, size_of::<T>(), self)
            } else if unlikely(io_arm7_read_wifi::is_in_range(addr_offset)) {
                io_arm7_read_wifi::read(addr_offset, size_of::<T>(), self)
            } else {
                0
            })
        }
    }

    pub fn io_arm7_write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        unsafe {
            if likely(io_arm7_write::is_in_range(addr_offset)) {
                io_arm7_write::write(value.into(), addr_offset, size_of::<T>(), self);
            } else if unlikely(io_arm7_write_wifi::is_in_range(addr_offset)) {
                io_arm7_write_wifi::write(value.into(), addr_offset, size_of::<T>(), self);
            }
        }
    }

    pub fn io_arm7_write_fixed_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        unsafe {
            if likely(io_arm7_write::is_in_range(addr_offset)) {
                for value in slice {
                    io_arm7_write::write((*value).into(), addr_offset, size_of::<T>(), self);
                }
            } else if unlikely(io_arm7_write_wifi::is_in_range(addr_offset)) {
                for value in slice {
                    io_arm7_write_wifi::write((*value).into(), addr_offset, size_of::<T>(), self);
                }
            }
        }
    }
}
