use crate::emu::memory::regions;
use crate::utils;
use crate::utils::{Convert, HeapMemU8};

pub struct Oam {
    mem: HeapMemU8<{ regions::OAM_SIZE as usize }>,
}

impl Oam {
    pub fn new() -> Self {
        Oam {
            mem: HeapMemU8::new(),
        }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem(self.mem.as_slice(), addr_offset & (regions::OAM_SIZE - 1))
    }

    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        utils::write_to_mem(
            self.mem.as_mut_slice(),
            addr_offset & (regions::OAM_SIZE - 1),
            value,
        )
    }
}
