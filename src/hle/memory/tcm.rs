use crate::hle::memory::{regions, Convert};
use crate::utils;

pub struct Tcm {
    itcm: Box<[u8; regions::INSTRUCTION_TCM_SIZE as usize]>,
    dtcm: Box<[u8; regions::DATA_TCM_SIZE as usize]>,
}

impl Tcm {
    pub fn new() -> Self {
        Tcm {
            itcm: Box::new([0u8; regions::INSTRUCTION_TCM_SIZE as usize]),
            dtcm: Box::new([0u8; regions::DATA_TCM_SIZE as usize]),
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
