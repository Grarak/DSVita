use crate::core::memory::regions;
use crate::utils;
use crate::utils::{Convert, HeapMemU8};

pub struct Bios<const SIZE: usize> {
    mem: HeapMemU8<SIZE>,
}

impl<const SIZE: usize> Bios<SIZE> {
    pub fn new() -> Self {
        let mut instance = Bios { mem: HeapMemU8::new() };
        instance.mem[3] = 0xEC; // Indicator for emit unknown to finish interrupt cpu
        instance
    }

    pub fn get_ptr(&self, addr: u32) -> *const u8 {
        unsafe { self.mem.as_ptr().add((addr as usize) & (SIZE - 1)) }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem(self.mem.as_slice(), addr_offset & (SIZE as u32 - 1))
    }
}

pub type BiosArm9 = Bios<{ regions::ARM9_BIOS_SIZE as usize }>;
pub type BiosArm7 = Bios<{ regions::ARM7_BIOS_SIZE as usize }>;
