use crate::core::memory::regions;
use crate::utils;
use crate::utils::{Convert, HeapMemU8};

pub struct Wifi {
    pub mem: HeapMemU8<{ regions::WIFI_RAM_SIZE as usize }>,
}

impl Wifi {
    pub fn new() -> Self {
        Wifi { mem: HeapMemU8::new() }
    }

    pub fn get_ptr(&self, addr: u32) -> *const u8 {
        unsafe { self.mem.as_ptr().add((addr & (regions::WIFI_RAM_SIZE - 1)) as usize) }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem(self.mem.as_slice(), addr_offset & (regions::WIFI_RAM_SIZE - 1))
    }

    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        utils::write_to_mem(self.mem.as_mut_slice(), addr_offset & (regions::WIFI_RAM_SIZE - 1), value);
    }
}
