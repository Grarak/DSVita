use crate::host_memory::VmManager;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Memory {
    pub vmm: Rc<RefCell<VmManager>>,
    pub wram_cnt: u8,
}

impl Memory {
    pub fn new(vmm: Rc<RefCell<VmManager>>) -> Self {
        Memory { vmm, wram_cnt: 0 }
    }

    pub fn set_wram_cnt(&mut self, value: u8) -> u8 {
        self.wram_cnt = value & 0x3;
        self.wram_cnt
    }
}
