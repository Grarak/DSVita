use crate::hle::memory::regions;
use crate::utils;
use crate::utils::{Convert, HeapMem};

pub struct PalettesContext {
    mem: HeapMem<{ regions::STANDARD_PALETTES_SIZE as usize }>,
}

impl PalettesContext {
    pub fn new() -> Self {
        PalettesContext {
            mem: HeapMem::new(),
        }
    }

    pub fn read_slice<T: Convert>(&self, addr_offset: u32, slice: &mut [T]) -> usize {
        utils::read_from_mem_slice(
            self.mem.as_slice(),
            addr_offset & (regions::STANDARD_PALETTES_SIZE - 1),
            slice,
        )
    }

    pub fn write_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) -> usize {
        utils::write_to_mem_slice(
            self.mem.as_mut_slice(),
            addr_offset & (regions::STANDARD_PALETTES_SIZE - 1),
            slice,
        )
    }
}