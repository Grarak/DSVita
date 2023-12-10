use crate::hle::memory::regions;
use crate::host_memory::VmManager;
use crate::logging::debug_println;
use crate::utils;

pub struct Memory {
    pub vmm: VmManager,
    pub wram_cnt: u8,
    wram_arm7: [u8; regions::ARM7_WRAM_SIZE as usize],
}

impl Memory {
    pub fn new(vmm: VmManager) -> Self {
        Memory {
            vmm,
            wram_cnt: 0,
            wram_arm7: [0u8; regions::ARM7_WRAM_SIZE as usize],
        }
    }

    pub fn set_wram_cnt(&mut self, value: u8) -> u8 {
        self.wram_cnt = value & 0x3;
        self.wram_cnt
    }

    pub fn read_wram_arm7<T: Copy + Into<u32>>(&self, addr_offset: u32) -> T {
        match self.wram_cnt {
            1 | 2 | 3 => {
                debug_println!(
                    "ARM7 wram read at {:x}",
                    regions::SHARED_WRAM_OFFSET | addr_offset
                );
                self.read_raw(regions::SHARED_WRAM_OFFSET | addr_offset)
            }
            _ => utils::read_from_mem(&self.wram_arm7, addr_offset & (regions::ARM7_WRAM_SIZE - 1)),
        }
    }

    pub fn write_wram_arm7<T: Copy + Into<u32>>(&mut self, addr_offset: u32, value: T) {
        match self.wram_cnt {
            1 | 2 | 3 => {
                debug_println!(
                    "ARM7 wram write at {:x} with value {:x}",
                    regions::SHARED_WRAM_OFFSET | addr_offset,
                    value.into()
                );
                self.write_raw(regions::SHARED_WRAM_OFFSET | addr_offset, value);
            }
            _ => {
                utils::write_to_mem(
                    &mut self.wram_arm7,
                    addr_offset & (regions::ARM7_WRAM_SIZE - 1),
                    value,
                );
            }
        }
    }

    pub fn write_wram_arm9<T: Into<u32>>(&self, addr_offset: u32, value: T) {
        todo!()
    }

    fn read_raw<T: Copy + Into<u32>>(&self, addr: u32) -> T {
        let vmmap = self.vmm.get_vm_mapping();
        utils::read_from_mem(&vmmap, addr)
    }

    fn write_raw<T: Into<u32>>(&self, addr: u32, value: T) {
        let mut vmmap = self.vmm.get_vm_mapping();
        utils::write_to_mem(&mut vmmap, addr, value)
    }
}
