use crate::hle::memory::regions;
use crate::utils;
use crate::utils::{Convert, HeapMemU8};

pub struct MainMemory {
    main: HeapMemU8<{ regions::MAIN_MEMORY_SIZE as usize }>,
}

impl MainMemory {
    pub fn new() -> Self {
        MainMemory {
            main: HeapMemU8::new(),
        }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem(
            self.main.as_slice(),
            addr_offset & (regions::MAIN_MEMORY_SIZE - 1),
        )
    }

    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        utils::write_to_mem(
            self.main.as_mut_slice(),
            addr_offset & (regions::MAIN_MEMORY_SIZE - 1),
            value,
        )
    }

    pub fn write_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        utils::write_to_mem_slice(
            self.main.as_mut_slice(),
            addr_offset & (regions::MAIN_MEMORY_SIZE - 1),
            slice,
        );
    }
}
