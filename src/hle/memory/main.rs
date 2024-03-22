use crate::hle::memory::regions;
use crate::mmap::Mmap;
use crate::utils;
use crate::utils::Convert;

pub struct Main {
    main: Mmap,
}

impl Main {
    pub fn new() -> Self {
        Main {
            main: Mmap::rw("main_memory", regions::MAIN_MEMORY_SIZE).unwrap(),
        }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem(&self.main, addr_offset & (regions::MAIN_MEMORY_SIZE - 1))
    }

    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        utils::write_to_mem(
            &mut self.main,
            addr_offset & (regions::MAIN_MEMORY_SIZE - 1),
            value,
        );
    }

    pub fn write_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        utils::write_to_mem_slice(
            &mut self.main,
            addr_offset & (regions::MAIN_MEMORY_SIZE - 1),
            slice,
        );
    }
}
