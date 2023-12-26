use crate::hle::memory::regions;
use crate::utils;
use crate::utils::{Convert, HeapMem};

pub struct TcmContext {
    itcm: HeapMem<{ regions::INSTRUCTION_TCM_SIZE as usize }>,
    dtcm: HeapMem<{ regions::DATA_TCM_SIZE as usize }>,
}

impl TcmContext {
    pub fn new() -> Self {
        TcmContext {
            itcm: HeapMem::new(),
            dtcm: HeapMem::new(),
        }
    }

    pub fn write_itcm_slice<T: Convert>(&mut self, addr: u32, slice: &[T]) {
        utils::write_to_mem_slice(
            self.itcm.as_mut_slice(),
            addr & (regions::INSTRUCTION_TCM_SIZE - 1),
            slice,
        );
    }

    pub fn write_dtcm_slice<T: Convert>(&mut self, addr: u32, slice: &[T]) {
        utils::write_to_mem_slice(
            self.dtcm.as_mut_slice(),
            addr & (regions::DATA_TCM_SIZE - 1),
            slice,
        );
    }
}
