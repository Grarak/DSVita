use crate::hle::memory::regions;
use crate::mmap::Mmap;
use crate::utils;
use crate::utils::Convert;

pub struct MainMemory {
    main: Mmap,
}

impl MainMemory {
    pub fn new() -> Self {
        MainMemory {
            main: Mmap::rw("main", regions::MAIN_MEMORY_ADDRESS_SPACE).unwrap(),
        }
    }

    pub fn read_main<T: Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem(&self.main, addr_offset & (regions::MAIN_MEMORY_OFFSET - 1))
    }

    pub fn read_main_slice<T: Convert>(&self, addr_offset: u32, slice: &mut [T]) {
        utils::read_from_mem_slice(
            &self.main,
            addr_offset & (regions::MAIN_MEMORY_OFFSET - 1),
            slice,
        )
    }

    pub fn write_main<T: Convert>(&mut self, addr_offset: u32, value: T) {
        utils::write_to_mem(
            &mut self.main,
            addr_offset & (regions::MAIN_MEMORY_OFFSET - 1),
            value,
        )
    }

    pub fn write_main_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        utils::write_to_mem_slice(
            &mut self.main,
            addr_offset & (regions::MAIN_MEMORY_OFFSET - 1),
            slice,
        )
    }
}
