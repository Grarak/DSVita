use crate::core::emu::Emu;
use crate::core::memory::io_arm7_lut::{IoArm7ReadLut, IoArm7ReadLutUpper, IoArm7ReadLutWifi, IoArm7WriteLut, IoArm7WriteLutWifi};
use crate::utils::Convert;

impl Emu {
    pub fn io_arm7_read<T: Convert>(&mut self, addr_offset: u32) -> T {
        match addr_offset & 0xF00000 {
            0x0 if IoArm7ReadLut::is_in_range(addr_offset) => T::from(IoArm7ReadLut::read(addr_offset, size_of::<T>() as u8, self)),
            0x100000 if IoArm7ReadLutUpper::is_in_range(addr_offset) => T::from(IoArm7ReadLutUpper::read(addr_offset, size_of::<T>() as u8, self)),
            0x800000 if IoArm7ReadLutWifi::is_in_range(addr_offset) => T::from(IoArm7ReadLutWifi::read(addr_offset, size_of::<T>() as u8, self)),
            _ => T::from(0),
        }
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
