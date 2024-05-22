use crate::emu::memory::regions;
use crate::utils;
use crate::utils::{Convert, HeapMemU8};
use std::ops::{Deref, DerefMut};

pub struct Main {
    main: HeapMemU8<{ regions::MAIN_MEMORY_SIZE as usize }>,
}

impl Main {
    pub fn new() -> Self {
        Main { main: HeapMemU8::new() }
    }

    pub fn get_ptr(&self, addr: u32) -> *const u8 {
        unsafe { self.main.as_ptr().add((addr & (regions::MAIN_MEMORY_SIZE - 1)) as usize) }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem(self.main.deref(), addr_offset & (regions::MAIN_MEMORY_SIZE - 1))
    }

    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        utils::write_to_mem(self.main.deref_mut(), addr_offset & (regions::MAIN_MEMORY_SIZE - 1), value);
    }

    pub fn write_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        utils::write_to_mem_slice(self.main.deref_mut(), addr_offset & (regions::MAIN_MEMORY_SIZE - 1), slice);
    }
}
