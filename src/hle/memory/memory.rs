use crate::hle::memory::indirect_memory::Convert;
use crate::hle::memory::regions;
use crate::host_memory::VmManager;
use crate::utils;
use std::ops::{Deref, DerefMut};

pub struct Memory {
    pub vmm: VmManager,
    pub wram_cnt: u8,
    wram_arm7: Box<[u8; regions::ARM7_WRAM_SIZE as usize]>,
}

impl Memory {
    pub fn new(vmm: VmManager) -> Self {
        Memory {
            vmm,
            wram_cnt: 0,
            wram_arm7: Box::new([0u8; regions::ARM7_WRAM_SIZE as usize]),
        }
    }

    pub fn set_wram_cnt(&mut self, value: u8) -> u8 {
        self.wram_cnt = value & 0x3;
        self.wram_cnt
    }

    pub fn read_wram_arm7<T: Convert>(&self, addr_offset: u32) -> T {
        let mut slice = [T::from(0)];
        self.read_wram_slice_arm7(addr_offset, &mut slice);
        slice[0]
    }

    pub fn read_wram_slice_arm7<T: Convert>(&self, addr_offset: u32, slice: &mut [T]) {
        match self.wram_cnt {
            1 | 2 | 3 => self.read_raw_slice(regions::SHARED_WRAM_OFFSET | addr_offset, slice),
            _ => utils::read_from_mem_slice(
                self.wram_arm7.deref(),
                addr_offset & (regions::ARM7_WRAM_SIZE - 1),
                slice,
            ),
        }
    }

    pub fn write_wram_arm7<T: Convert>(&mut self, addr_offset: u32, value: T) {
        match self.wram_cnt {
            1 | 2 | 3 => {
                self.write_raw(regions::SHARED_WRAM_OFFSET | addr_offset, value);
            }
            _ => {
                utils::write_to_mem(
                    self.wram_arm7.deref_mut(),
                    addr_offset & (regions::ARM7_WRAM_SIZE - 1),
                    value,
                );
            }
        }
    }

    pub fn write_wram_arm9<T: Convert>(&mut self, addr_offset: u32, value: T) {
        todo!()
    }

    pub fn read_raw<T: Convert>(&self, addr: u32) -> T {
        let vmmap = self.vmm.get_vm_mapping();
        utils::read_from_mem(&vmmap, addr)
    }

    pub fn read_raw_slice<T: Convert>(&self, addr: u32, slice: &mut [T]) {
        let vmmap = self.vmm.get_vm_mapping();
        utils::read_from_mem_slice(&vmmap, addr, slice);
    }

    pub fn write_raw<T: Convert>(&mut self, addr: u32, value: T) {
        let mut vmmap = self.vmm.get_vm_mapping_mut();
        utils::write_to_mem(&mut vmmap, addr, value)
    }

    pub fn write_raw_slice<T: Convert>(&mut self, addr: u32, slice: &[T]) {
        let mut vmmap = self.vmm.get_vm_mapping_mut();
        utils::write_to_mem_slice(&mut vmmap, addr, slice)
    }
}
