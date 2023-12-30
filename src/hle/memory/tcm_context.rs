use crate::hle::memory::regions;
use crate::utils;
use crate::utils::{Convert, HeapMemU8};

pub struct TcmContext {
    itcm: HeapMemU8<{ regions::INSTRUCTION_TCM_SIZE as usize }>,
    dtcm: HeapMemU8<{ regions::DATA_TCM_SIZE as usize }>,
}

impl TcmContext {
    pub fn new() -> Self {
        TcmContext {
            itcm: HeapMemU8::new(),
            dtcm: HeapMemU8::new(),
        }
    }

    pub fn read_itcm_slice<T: Convert>(&mut self, addr: u32, slice: &mut [T]) -> usize {
        utils::read_from_mem_slice(
            self.itcm.as_slice(),
            addr & (regions::INSTRUCTION_TCM_SIZE - 1),
            slice,
        )
    }

    pub fn write_itcm_slice<T: Convert>(&mut self, addr: u32, slice: &[T]) -> usize {
        utils::write_to_mem_slice(
            self.itcm.as_mut_slice(),
            addr & (regions::INSTRUCTION_TCM_SIZE - 1),
            slice,
        )
    }

    pub fn read_dtcm_slice<T: Convert>(&mut self, addr: u32, slice: &mut [T]) -> usize {
        utils::read_from_mem_slice(
            self.dtcm.as_slice(),
            addr & (regions::DATA_TCM_SIZE - 1),
            slice,
        )
    }

    pub fn write_dtcm_slice<T: Convert>(&mut self, addr: u32, slice: &[T]) -> usize {
        utils::write_to_mem_slice(
            self.dtcm.as_mut_slice(),
            addr & (regions::DATA_TCM_SIZE - 1),
            slice,
        )
    }
}
