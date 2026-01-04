use crate::core::memory::regions;
use crate::utils;
use crate::utils::{Convert, HeapArrayU8};

pub struct Wifi {
    pub mem: HeapArrayU8<{ regions::WIFI_RAM_SIZE as usize }>,
}

impl Wifi {
    pub fn new() -> Self {
        Wifi { mem: HeapArrayU8::default() }
    }

    pub fn get_ptr(&self, addr: u32) -> *const u8 {
        unsafe { self.mem.as_ptr().add((addr & (regions::WIFI_RAM_SIZE - 1)) as usize) }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem(self.mem.as_slice(), addr_offset & (regions::WIFI_RAM_SIZE - 1))
    }

    pub fn read_slice<T: Convert>(&self, addr_offset: u32, slice: &mut [T]) {
        utils::read_from_mem_slice(self.mem.as_slice(), addr_offset & (regions::WIFI_RAM_SIZE - 1), slice);
    }

    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        utils::write_to_mem(self.mem.as_mut_slice(), addr_offset & (regions::WIFI_RAM_SIZE - 1), value);
    }

    pub fn write_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        utils::write_to_mem_slice(self.mem.as_mut_slice(), (addr_offset & (regions::WIFI_RAM_SIZE - 1)) as usize, slice);
    }

    pub fn write_memset<T: Convert>(&mut self, addr_offset: u32, value: T, size: usize) {
        utils::write_memset(self.mem.as_mut_slice(), (addr_offset & (regions::WIFI_RAM_SIZE - 1)) as usize, value, size)
    }
}
