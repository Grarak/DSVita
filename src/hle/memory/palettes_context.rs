use crate::hle::memory::regions;
use crate::utils;
use crate::utils::{Convert, HeapMemU8};

pub struct PalettesContext {
    mem: HeapMemU8<{ regions::STANDARD_PALETTES_SIZE as usize }>,
}

impl PalettesContext {
    pub fn new() -> Self {
        PalettesContext {
            mem: HeapMemU8::new(),
        }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem(
            self.mem.as_slice(),
            addr_offset & (regions::STANDARD_PALETTES_SIZE - 1),
        )
    }

    pub fn write<T: Convert>(&mut self, addr_offset: u32, value: T) {
        utils::write_to_mem(
            self.mem.as_mut_slice(),
            addr_offset & (regions::STANDARD_PALETTES_SIZE - 1),
            value,
        );
    }
}
