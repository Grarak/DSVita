use crate::core::memory::contiguous_mem::ContiguousMem;
use crate::core::memory::regions;
use crate::utils;
use crate::utils::{Convert, HeapMemU8};

pub struct Tcm {
    itcm: *const u8,
    dtcm: HeapMemU8<{ regions::DATA_TCM_SIZE as usize }>,
}

impl Tcm {
    pub fn new(contiguous_mem: &mut ContiguousMem) -> Self {
        Tcm {
            itcm: contiguous_mem.get_itcm_ptr(),
            dtcm: HeapMemU8::new(),
        }
    }

    pub fn get_itcm_ptr(&self, addr: u32) -> *const u8 {
        unsafe { self.itcm.add((addr & (regions::INSTRUCTION_TCM_SIZE - 1)) as usize) }
    }

    pub fn read_itcm<T: Convert>(&self, addr: u32) -> T {
        utils::read_from_mem_ptr(self.itcm, addr & (regions::INSTRUCTION_TCM_SIZE - 1))
    }

    pub fn write_itcm<T: Convert>(&mut self, addr: u32, value: T) {
        utils::write_to_mem_ptr(self.itcm, addr & (regions::INSTRUCTION_TCM_SIZE - 1), value);
    }

    pub fn get_dtcm_ptr(&self, addr: u32) -> *const u8 {
        unsafe { self.dtcm.as_ptr().add((addr & (regions::DATA_TCM_SIZE - 1)) as usize) }
    }

    pub fn read_dtcm<T: Convert>(&self, addr: u32) -> T {
        utils::read_from_mem(self.dtcm.as_slice(), addr & (regions::DATA_TCM_SIZE - 1))
    }

    pub fn write_dtcm<T: Convert>(&mut self, addr: u32, value: T) {
        utils::write_to_mem(self.dtcm.as_mut_slice(), addr & (regions::DATA_TCM_SIZE - 1), value);
    }
}
