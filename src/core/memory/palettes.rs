use crate::core::memory::regions;
use crate::utils;
use crate::utils::{Convert, HeapMemU8};

pub struct Palettes {
    pub mem: HeapMemU8<{ regions::STANDARD_PALETTES_SIZE as usize }>,
    pub dirty: bool,
}

impl Palettes {
    pub fn new() -> Self {
        Palettes { mem: HeapMemU8::new(), dirty: false }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem(self.mem.as_slice(), addr_offset & (regions::STANDARD_PALETTES_SIZE - 1))
    }

    pub fn read_slice<T: Convert>(&self, addr_offset: u32, slice: &mut [T]) {
        utils::read_from_mem_slice(self.mem.as_slice(), addr_offset & (regions::STANDARD_PALETTES_SIZE - 1), slice);
    }

    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        self.dirty = true;
        utils::write_to_mem(self.mem.as_mut_slice(), addr_offset & (regions::STANDARD_PALETTES_SIZE - 1), value);
    }

    pub fn write_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        self.dirty = true;
        utils::write_to_mem_slice(self.mem.as_mut_slice(), (addr_offset & (regions::STANDARD_PALETTES_SIZE - 1)) as usize, slice);
    }

    pub fn write_memset<T: Convert>(&mut self, addr_offset: u32, value: T, size: usize) {
        self.dirty = true;
        utils::write_memset(self.mem.as_mut_slice(), (addr_offset & (regions::STANDARD_PALETTES_SIZE - 1)) as usize, value, size)
    }
}
