use crate::core::memory::contiguous_mem::ContiguousMem;
use crate::core::memory::regions;
use crate::utils;
use crate::utils::Convert;

pub struct Main(*const u8);

impl Main {
    pub fn new(contiguous_mem: &mut ContiguousMem) -> Self {
        Main(contiguous_mem.get_main_ptr())
    }

    pub fn get_ptr(&self, addr: u32) -> *const u8 {
        unsafe { self.0.add((addr & (regions::MAIN_MEMORY_SIZE - 1)) as usize) }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem_ptr(self.0, addr_offset & (regions::MAIN_MEMORY_SIZE - 1))
    }

    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        utils::write_to_mem_ptr(self.0, addr_offset & (regions::MAIN_MEMORY_SIZE - 1), value);
    }

    pub fn write_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        utils::write_to_mem_slice_ptr(self.0, regions::MAIN_MEMORY_SIZE as usize, addr_offset & (regions::MAIN_MEMORY_SIZE - 1), slice);
    }
}
