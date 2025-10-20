use crate::core::emu::Emu;
use crate::core::memory::io_arm7_lut::*;
use crate::utils::Convert;

impl Emu {
    pub fn io_arm7_read<T: Convert>(&mut self, addr_offset: u32) -> T {
        let ret = unsafe {
            match addr_offset & 0xF00000 {
                0x0 if io_arm7::is_in_range(addr_offset) => io_arm7::read(addr_offset, size_of::<T>(), self),
                0x100000 if io_arm7_upper::is_in_range(addr_offset) => io_arm7_upper::read(addr_offset, size_of::<T>(), self),
                0x800000 if io_arm7_wifi::is_in_range(addr_offset) => io_arm7_wifi::read(addr_offset, size_of::<T>(), self),
                _ => 0,
            }
        };
        T::from(ret)
    }

    pub fn io_arm7_write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        match addr_offset & 0xF00000 {
            0x0 if IoArm7WriteLut::is_in_range(addr_offset) => IoArm7WriteLut::write(value.into(), addr_offset, size_of::<T>() as u8, self),
            0x800000 if IoArm7WriteLutWifi::is_in_range(addr_offset) => IoArm7WriteLutWifi::write(value.into(), addr_offset, size_of::<T>() as u8, self),
            _ => {}
        }
    }

    pub fn io_arm7_write_fixed_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        match addr_offset & 0xF00000 {
            0x0 if IoArm7WriteLut::is_in_range(addr_offset) => IoArm7WriteLut::write_fixed_slice(addr_offset, slice, self),
            0x800000 if IoArm7WriteLutWifi::is_in_range(addr_offset) => IoArm7WriteLutWifi::write_fixed_slice(addr_offset, slice, self),
            _ => {}
        }
    }
}
