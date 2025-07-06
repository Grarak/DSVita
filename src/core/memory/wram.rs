use crate::core::emu::Emu;
use crate::core::memory::regions;
use crate::core::CpuType;
use crate::core::CpuType::ARM7;
use std::hint::unreachable_unchecked;
use CpuType::ARM9;

const ARM7_WRAM_SHM_OFFSET: usize = regions::ARM7_WRAM_REGION.shm_offset;
const ARM7_WRAM_LEN: usize = regions::ARM7_WRAM_REGION.size;

struct SharedWramMap {
    shm_offset: usize,
    size: usize,
}

impl SharedWramMap {
    fn new(shm_offset: usize, size: usize) -> SharedWramMap {
        SharedWramMap { shm_offset, size }
    }
}

impl Default for SharedWramMap {
    fn default() -> Self {
        SharedWramMap { shm_offset: usize::MAX, size: 1 }
    }
}

pub struct Wram {
    pub cnt: u8,
    arm9_map: SharedWramMap,
    arm7_map: SharedWramMap,
}

impl Wram {
    pub fn new() -> Self {
        let mut instance = Wram {
            cnt: 0,
            arm9_map: SharedWramMap::default(),
            arm7_map: SharedWramMap::default(),
        };
        instance.init_maps();
        instance
    }

    fn init_maps(&mut self) {
        const SHARED_OFFSET: usize = regions::SHARED_WRAM_REGION.shm_offset;
        const SHARED_LEN: usize = regions::SHARED_WRAM_SIZE as usize;

        match self.cnt {
            0 => {
                self.arm9_map = SharedWramMap::new(SHARED_OFFSET, SHARED_LEN);
                self.arm7_map = SharedWramMap::new(ARM7_WRAM_SHM_OFFSET, ARM7_WRAM_LEN);
            }
            1 => {
                self.arm9_map = SharedWramMap::new(SHARED_OFFSET + SHARED_LEN / 2, SHARED_LEN / 2);
                self.arm7_map = SharedWramMap::new(SHARED_OFFSET, SHARED_LEN / 2);
            }
            2 => {
                self.arm9_map = SharedWramMap::new(SHARED_OFFSET, SHARED_LEN / 2);
                self.arm7_map = SharedWramMap::new(SHARED_OFFSET + SHARED_LEN / 2, SHARED_LEN / 2);
            }
            3 => {
                self.arm9_map = SharedWramMap::default();
                self.arm7_map = SharedWramMap::new(SHARED_OFFSET, SHARED_LEN);
            }
            _ => unsafe { unreachable_unchecked() },
        }
    }

    pub fn get_shm_offset<const CPU: CpuType>(&self, addr: u32) -> usize {
        match CPU {
            ARM9 => self.arm9_map.shm_offset + (addr as usize & (self.arm9_map.size - 1)),
            ARM7 => {
                if addr & regions::ARM7_WRAM_OFFSET == regions::ARM7_WRAM_OFFSET {
                    ARM7_WRAM_SHM_OFFSET + (addr as usize & (ARM7_WRAM_LEN - 1))
                } else {
                    self.arm7_map.shm_offset + (addr as usize & (self.arm7_map.size - 1))
                }
            }
        }
    }
}

impl Emu {
    pub fn wram_set_cnt(&mut self, value: u8) {
        let value = value & 0x3;
        if value == self.mem.wram.cnt {
            return;
        }

        self.mem.wram.cnt = value;
        self.mem.wram.init_maps();

        self.mmu_update_wram::<{ ARM9 }>();
        self.mmu_update_wram::<{ ARM7 }>();
    }
}
