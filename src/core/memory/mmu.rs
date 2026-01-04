use crate::core::cp15::TcmState;
use crate::core::emu::Emu;
use crate::core::memory::regions;
use crate::core::memory::regions::{
    ARM7_BIOS_REGION, ARM9_BIOS_REGION, DTCM_REGION, GBA_ROM_REGION, ITCM_REGION, OAM_OFFSET, OAM_REGION, PALETTES_REGION, STANDARD_PALETTES_OFFSET, VRAM_OFFSET, V_MEM_ARM7_RANGE,
};
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::logging::debug_println;
use crate::mmap::{MemRegion, VirtualMem};
use crate::utils::HeapArrayUsize;
use regions::{ARM7_BIOS_OFFSET, GBA_RAM_OFFSET, GBA_ROM_OFFSET, GBA_ROM_OFFSET2, IO_PORTS_OFFSET, MAIN_OFFSET, MAIN_REGION, SHARED_WRAM_OFFSET, V_MEM_ARM9_RANGE};
use std::cmp::max;

pub const MMU_PAGE_SHIFT: usize = 12;
pub const MMU_PAGE_SIZE: usize = 1 << MMU_PAGE_SHIFT;

const FAST_MEM_PAGE_SHIFT: usize = 14;
pub const FAST_MEM_PAGE_SIZE: usize = 1 << FAST_MEM_PAGE_SHIFT;

fn remove_mmu_write_entry(addr: u32, region: &MemRegion, mmu: &mut [usize], vmem: Option<&mut VirtualMem>) {
    if mmu[(addr >> MMU_PAGE_SHIFT) as usize] == 0 {
        return;
    }
    let base_offset = addr - region.start as u32;
    let base_offset = base_offset & (region.size as u32 - 1);
    for addr_offset in (region.start as u32 + base_offset..region.end as u32).step_by(region.size) {
        mmu[(addr_offset >> MMU_PAGE_SHIFT) as usize] = 0;
    }
    if let Some(vmem) = vmem {
        vmem.set_region_protection(addr as usize & !(MMU_PAGE_SIZE - 1), MMU_PAGE_SIZE, region, true, false, false);
    }
}

impl MmuArm9 {
    pub fn new() -> Self {
        MmuArm9 {
            vmem_tcm: VirtualMem::new(V_MEM_ARM9_RANGE as _, ARM9.mmu_tcm_addr()).unwrap(),
            mmu_read: HeapArrayUsize::default(),
            mmu_write: HeapArrayUsize::default(),
            mmu_read_tcm: HeapArrayUsize::default(),
            mmu_write_tcm: HeapArrayUsize::default(),
            current_itcm_size: 0,
            current_dtcm_addr: 0,
            current_dtcm_size: 0,
        }
    }
}

pub struct MmuArm9 {
    vmem_tcm: VirtualMem,
    mmu_read: HeapArrayUsize<{ V_MEM_ARM9_RANGE as usize / MMU_PAGE_SIZE }>,
    mmu_write: HeapArrayUsize<{ V_MEM_ARM9_RANGE as usize / MMU_PAGE_SIZE }>,
    mmu_read_tcm: HeapArrayUsize<{ V_MEM_ARM9_RANGE as usize / MMU_PAGE_SIZE }>,
    mmu_write_tcm: HeapArrayUsize<{ V_MEM_ARM9_RANGE as usize / MMU_PAGE_SIZE }>,
    current_itcm_size: u32,
    current_dtcm_addr: u32,
    current_dtcm_size: u32,
}

impl Emu {
    fn update_all_no_tcm_arm9(&mut self) {
        for addr in (MAIN_OFFSET..V_MEM_ARM9_RANGE).step_by(MMU_PAGE_SIZE) {
            let mmu_read = &mut self.mem.mmu_arm9.mmu_read[(addr as usize) >> MMU_PAGE_SHIFT];
            let mmu_write = &mut self.mem.mmu_arm9.mmu_write[(addr as usize) >> MMU_PAGE_SHIFT];
            *mmu_read = 0;
            *mmu_write = 0;

            match addr & 0x0F000000 {
                MAIN_OFFSET => {
                    let addr_offset = (addr as usize) & (MAIN_REGION.size - 1);
                    *mmu_read = MAIN_REGION.shm_offset + addr_offset;
                    *mmu_write = MAIN_REGION.shm_offset + addr_offset;
                }
                GBA_ROM_OFFSET | GBA_ROM_OFFSET2 | GBA_RAM_OFFSET => *mmu_read = GBA_ROM_REGION.shm_offset,
                0x0F000000 => *mmu_read = ARM9_BIOS_REGION.shm_offset,
                _ => {}
            }
        }

        self.update_wram_no_tcm_arm9();

        self.remove_vram_tcm_arm9(0, OAM_OFFSET);

        let mmu_read = &mut self.mem.mmu_arm9.mmu_read[(STANDARD_PALETTES_OFFSET as usize) >> MMU_PAGE_SHIFT];
        let mmu_write = &mut self.mem.mmu_arm9.mmu_write[(STANDARD_PALETTES_OFFSET as usize) >> MMU_PAGE_SHIFT];
        *mmu_read = PALETTES_REGION.shm_offset;
        *mmu_write = PALETTES_REGION.shm_offset;

        let mmu_read = &mut self.mem.mmu_arm9.mmu_read[(OAM_OFFSET as usize) >> MMU_PAGE_SHIFT];
        let mmu_write = &mut self.mem.mmu_arm9.mmu_write[(OAM_OFFSET as usize) >> MMU_PAGE_SHIFT];
        *mmu_read = OAM_REGION.shm_offset;
        *mmu_write = OAM_REGION.shm_offset;
    }

    fn update_wram_no_tcm_arm9(&mut self) {
        let wram = &self.mem.wram;

        for addr in (SHARED_WRAM_OFFSET..IO_PORTS_OFFSET).step_by(MMU_PAGE_SIZE) {
            let shifted_addr = (addr as usize) >> MMU_PAGE_SHIFT;
            let mmu_read = unsafe { self.mem.mmu_arm9.mmu_read.get_unchecked_mut(shifted_addr) };
            let mmu_write = unsafe { self.mem.mmu_arm9.mmu_write.get_unchecked_mut(shifted_addr) };

            let shm_offset = wram.get_shm_offset::<{ ARM9 }>(addr);
            if shm_offset != usize::MAX {
                *mmu_read = shm_offset;
                *mmu_write = shm_offset;
            } else {
                *mmu_read = 0;
                *mmu_write = 0;
            }
        }
    }

    fn initialize_tcm_arm9(&mut self) {
        // mprotect is the only way in kubridge to merge pages
        // Because with jit we are setting protection in 4kb pages
        for addr in (0..SHARED_WRAM_OFFSET).step_by(FAST_MEM_PAGE_SIZE) {
            self.mem.mmu_arm9.vmem_tcm.set_protection(addr as usize, FAST_MEM_PAGE_SIZE, false, false, false);
        }
    }

    fn update_tcm_arm9(&mut self, start: u32, end: u32) {
        debug_println!("update tcm arm9 {start:x} - {end:x}");

        let shm = &self.mem.shm;
        let jit_mmu = &self.jit.jit_memory_map;

        for addr in (start..end).step_by(MMU_PAGE_SIZE) {
            let shifted_addr = (addr as usize) >> MMU_PAGE_SHIFT;
            let mmu_read = unsafe { self.mem.mmu_arm9.mmu_read_tcm.get_unchecked_mut(shifted_addr) };
            let mmu_write = unsafe { self.mem.mmu_arm9.mmu_write_tcm.get_unchecked_mut(shifted_addr) };
            *mmu_read = 0;
            *mmu_write = 0;

            match addr & 0x0F000000 {
                MAIN_OFFSET => {
                    let addr_offset = (addr as usize) & (MAIN_REGION.size - 1);
                    *mmu_read = MAIN_REGION.shm_offset + addr_offset;
                    let write = !jit_mmu.has_jit_block(addr);
                    if write {
                        *mmu_write = MAIN_REGION.shm_offset + addr_offset;
                    }
                }
                SHARED_WRAM_OFFSET => {
                    let shm_offset = self.mem.wram.get_shm_offset::<{ ARM9 }>(addr);
                    if shm_offset != usize::MAX {
                        *mmu_read = shm_offset;
                        *mmu_write = shm_offset;
                    }
                }
                _ => {}
            }

            if addr < self.cp15.itcm_size {
                if self.cp15.itcm_state == TcmState::RW {
                    let addr_offset = (addr as usize) & (ITCM_REGION.size - 1);
                    let write = !jit_mmu.has_jit_block(addr);
                    *mmu_read = ITCM_REGION.shm_offset + addr_offset;
                    if write {
                        *mmu_write = ITCM_REGION.shm_offset + addr_offset;
                    }
                }
            } else if addr >= self.cp15.dtcm_addr && addr < self.cp15.dtcm_addr + self.cp15.dtcm_size {
                if self.cp15.dtcm_state == TcmState::RW {
                    let base_addr = addr - self.cp15.dtcm_addr;
                    let addr_offset = (base_addr as usize) & (DTCM_REGION.size - 1);
                    *mmu_read = DTCM_REGION.shm_offset + addr_offset;
                    *mmu_write = DTCM_REGION.shm_offset + addr_offset;
                }
            }
        }

        for addr in (start..end).step_by(FAST_MEM_PAGE_SIZE) {
            self.mem.mmu_arm9.vmem_tcm.destroy_map(addr as usize, FAST_MEM_PAGE_SIZE);
        }

        for addr in (start..end).step_by(FAST_MEM_PAGE_SIZE) {
            let base_addr = addr & !0xFF000000;
            match addr & 0x0F000000 {
                MAIN_OFFSET => {
                    let write = !jit_mmu.has_jit_block(addr);
                    self.mem
                        .mmu_arm9
                        .vmem_tcm
                        .create_page_map(shm, MAIN_REGION.shm_offset, base_addr as usize, MAIN_REGION.size, addr as usize, FAST_MEM_PAGE_SIZE, write)
                        .unwrap();
                }
                SHARED_WRAM_OFFSET => {
                    let shm_offset = self.mem.wram.get_shm_offset::<{ ARM9 }>(addr);
                    if shm_offset != usize::MAX {
                        self.mem
                            .mmu_arm9
                            .vmem_tcm
                            .create_page_map(shm, shm_offset, 0, FAST_MEM_PAGE_SIZE, addr as usize, FAST_MEM_PAGE_SIZE, true)
                            .unwrap();
                    } else {
                        self.mem.mmu_arm9.vmem_tcm.create_map(shm, 0, addr as usize, FAST_MEM_PAGE_SIZE, false, false, false).unwrap();
                    }
                }
                _ => {}
            }

            if addr < self.cp15.itcm_size {
                if self.cp15.itcm_state == TcmState::RW {
                    let write = !jit_mmu.has_jit_block(addr);
                    self.mem
                        .mmu_arm9
                        .vmem_tcm
                        .create_page_map(shm, ITCM_REGION.shm_offset, base_addr as usize, ITCM_REGION.size, addr as usize, FAST_MEM_PAGE_SIZE, write)
                        .unwrap();
                }
            } else if addr >= self.cp15.dtcm_addr && addr < self.cp15.dtcm_addr + self.cp15.dtcm_size {
                if self.cp15.dtcm_state == TcmState::RW {
                    let base_addr = addr - self.cp15.dtcm_addr;
                    let addr_offset = (base_addr as usize) & (DTCM_REGION.size - 1);
                    self.mem.mmu_arm9.vmem_tcm.destroy_map(addr as usize, FAST_MEM_PAGE_SIZE);
                    self.mem
                        .mmu_arm9
                        .vmem_tcm
                        .create_page_map(shm, DTCM_REGION.shm_offset, addr_offset, DTCM_REGION.size, addr as usize, FAST_MEM_PAGE_SIZE, true)
                        .unwrap();
                }
            }
        }

        self.mem.mmu_arm9.current_itcm_size = self.cp15.itcm_size;
        self.mem.mmu_arm9.current_dtcm_addr = self.cp15.dtcm_addr;
        self.mem.mmu_arm9.current_dtcm_size = self.cp15.dtcm_size;
    }

    fn initialize_tcm_misc_arm9(&mut self) {
        let mmu_read_tcm = &mut self.mem.mmu_arm9.mmu_read_tcm[(STANDARD_PALETTES_OFFSET as usize) >> MMU_PAGE_SHIFT];
        let mmu_write_tcm = &mut self.mem.mmu_arm9.mmu_write_tcm[(STANDARD_PALETTES_OFFSET as usize) >> MMU_PAGE_SHIFT];
        *mmu_read_tcm = PALETTES_REGION.shm_offset;
        *mmu_write_tcm = PALETTES_REGION.shm_offset;

        self.mem.mmu_arm9.vmem_tcm.destroy_region_map(&PALETTES_REGION);
        self.mem.mmu_arm9.vmem_tcm.create_region_map(&self.mem.shm, &PALETTES_REGION).unwrap();

        let mmu_read_tcm = &mut self.mem.mmu_arm9.mmu_read_tcm[(OAM_OFFSET as usize) >> MMU_PAGE_SHIFT];
        let mmu_write_tcm = &mut self.mem.mmu_arm9.mmu_write_tcm[(OAM_OFFSET as usize) >> MMU_PAGE_SHIFT];
        *mmu_read_tcm = OAM_REGION.shm_offset;
        *mmu_write_tcm = OAM_REGION.shm_offset;

        self.mem.mmu_arm9.vmem_tcm.destroy_region_map(&OAM_REGION);
        self.mem.mmu_arm9.vmem_tcm.create_region_map(&self.mem.shm, &OAM_REGION).unwrap();

        for addr in (GBA_ROM_OFFSET..V_MEM_ARM9_RANGE).step_by(MMU_PAGE_SIZE) {
            let mmu_read = &mut self.mem.mmu_arm9.mmu_read_tcm[(addr as usize) >> MMU_PAGE_SHIFT];
            *mmu_read = 0;

            match addr & 0x0F000000 {
                GBA_ROM_OFFSET | GBA_ROM_OFFSET2 | GBA_RAM_OFFSET => *mmu_read = GBA_ROM_REGION.shm_offset,
                0x0F000000 => *mmu_read = ARM9_BIOS_REGION.shm_offset,
                _ => {}
            }
        }
    }

    fn remove_vram_tcm_arm9(&mut self, start: u32, end: u32) {
        let start = start | VRAM_OFFSET;
        let end = end | VRAM_OFFSET;
        for addr in (start..end).step_by(MMU_PAGE_SIZE) {
            let shifted_addr = (addr as usize) >> MMU_PAGE_SHIFT;
            let mmu_read = unsafe { self.mem.mmu_arm9.mmu_read.get_unchecked_mut(shifted_addr) };
            let mmu_write = unsafe { self.mem.mmu_arm9.mmu_write.get_unchecked_mut(shifted_addr) };
            let mmu_read_tcm = unsafe { self.mem.mmu_arm9.mmu_read_tcm.get_unchecked_mut(shifted_addr) };
            let mmu_write_tcm = unsafe { self.mem.mmu_arm9.mmu_write_tcm.get_unchecked_mut(shifted_addr) };
            *mmu_read = 0;
            *mmu_write = 0;
            *mmu_read_tcm = 0;
            *mmu_write_tcm = 0;
        }
    }

    fn update_vram_tcm_arm9(&mut self, start: u32, end: u32) {
        let start = start | VRAM_OFFSET;
        let end = end | VRAM_OFFSET;
        for addr in (start..end).step_by(MMU_PAGE_SIZE) {
            let shifted_addr = (addr as usize) >> MMU_PAGE_SHIFT;
            let mmu_read = unsafe { self.mem.mmu_arm9.mmu_read.get_unchecked_mut(shifted_addr) };
            let mmu_write = unsafe { self.mem.mmu_arm9.mmu_write.get_unchecked_mut(shifted_addr) };
            let mmu_read_tcm = unsafe { self.mem.mmu_arm9.mmu_read_tcm.get_unchecked_mut(shifted_addr) };
            let mmu_write_tcm = unsafe { self.mem.mmu_arm9.mmu_write_tcm.get_unchecked_mut(shifted_addr) };

            let shm_offset = self.mem.vram.get_shm_offset::<{ ARM9 }>(addr);

            *mmu_read = shm_offset;
            *mmu_write = shm_offset;
            *mmu_read_tcm = shm_offset;
            *mmu_write_tcm = shm_offset;
        }
    }
}

pub struct MmuArm7 {
    vmem: VirtualMem,
    mmu_read: HeapArrayUsize<{ V_MEM_ARM7_RANGE as usize / MMU_PAGE_SIZE }>,
    mmu_write: HeapArrayUsize<{ V_MEM_ARM7_RANGE as usize / MMU_PAGE_SIZE }>,
}

impl MmuArm7 {
    pub fn new() -> Self {
        MmuArm7 {
            vmem: VirtualMem::new(V_MEM_ARM7_RANGE as usize, ARM7.mmu_tcm_addr()).unwrap(),
            mmu_read: HeapArrayUsize::default(),
            mmu_write: HeapArrayUsize::default(),
        }
    }
}

impl Emu {
    fn update_all_arm7(&mut self) {
        for addr in (0..V_MEM_ARM7_RANGE).step_by(MMU_PAGE_SIZE) {
            let mmu_read = &mut self.mem.mmu_arm7.mmu_read[(addr as usize) >> MMU_PAGE_SHIFT];
            let mmu_write = &mut self.mem.mmu_arm7.mmu_write[(addr as usize) >> MMU_PAGE_SHIFT];
            *mmu_read = 0;
            *mmu_write = 0;

            match addr & 0x0F000000 {
                ARM7_BIOS_OFFSET | 0x01000000 => *mmu_read = ARM7_BIOS_REGION.shm_offset,
                MAIN_OFFSET => {
                    let addr_offset = (addr as usize) & (MAIN_REGION.size - 1);
                    *mmu_read = MAIN_REGION.shm_offset + addr_offset;
                    *mmu_write = MAIN_REGION.shm_offset + addr_offset;
                }
                GBA_ROM_OFFSET | GBA_ROM_OFFSET2 | GBA_RAM_OFFSET => *mmu_read = GBA_ROM_REGION.shm_offset,
                _ => {}
            }
        }

        // self.mem.mmu_arm7.vmem.destroy_region_map(&ARM7_BIOS_REGION);
        // self.mem.mmu_arm7.vmem.create_region_map(&self.mem.shm, &ARM7_BIOS_REGION).unwrap();

        for addr in (MAIN_OFFSET..SHARED_WRAM_OFFSET).step_by(FAST_MEM_PAGE_SIZE) {
            self.mem.mmu_arm7.vmem.set_protection(addr as usize, FAST_MEM_PAGE_SIZE, false, false, false);
        }

        for addr in (MAIN_OFFSET..SHARED_WRAM_OFFSET).step_by(FAST_MEM_PAGE_SIZE) {
            let base_addr = addr & !0xFF000000;
            self.mem
                .mmu_arm7
                .vmem
                .create_page_map(&self.mem.shm, MAIN_REGION.shm_offset, base_addr as usize, MAIN_REGION.size, addr as usize, FAST_MEM_PAGE_SIZE, true)
                .unwrap();
        }

        // This aligns the underlying pages of the vita to 8kb for the wifi regions
        // Otherwise it will crash when cleaning them up
        // for addr in (IO_PORTS_OFFSET..STANDARD_PALETTES_OFFSET).step_by(8 * 1024) {
        //     self.vmem.destroy_map(addr as usize, 8 * 1024);
        //     self.vmem.create_map(shm, 0, addr as usize, 8 * 1024, false, false, false).unwrap();
        // }

        // self.vmem.destroy_region_map(&WIFI_REGION);
        // self.vmem.create_region_map(shm, &WIFI_REGION).unwrap();
        //
        // self.vmem.destroy_region_map(&WIFI_MIRROR_REGION);
        // self.vmem.create_region_map(shm, &WIFI_MIRROR_REGION).unwrap();

        // self.mem.mmu_arm7.vmem.destroy_region_map(&GBA_ROM_REGION);
        // self.mem.mmu_arm7.vmem.create_region_map(&self.mem.shm, &GBA_ROM_REGION).unwrap();
        //
        // self.mem.mmu_arm7.vmem.destroy_region_map(&GBA_RAM_REGION);
        // self.mem.mmu_arm7.vmem.create_region_map(&self.mem.shm, &GBA_RAM_REGION).unwrap();

        self.update_wram_arm7();

        self.remove_vram_arm7(0, OAM_OFFSET);
    }

    fn update_wram_arm7(&mut self) {
        for addr in (SHARED_WRAM_OFFSET..IO_PORTS_OFFSET).step_by(MMU_PAGE_SIZE) {
            let shifted_addr = (addr as usize) >> MMU_PAGE_SHIFT;
            let mmu_read = unsafe { self.mem.mmu_arm7.mmu_read.get_unchecked_mut(shifted_addr) };
            let mmu_write = unsafe { self.mem.mmu_arm7.mmu_write.get_unchecked_mut(shifted_addr) };

            let shm_offset = self.mem.wram.get_shm_offset::<{ ARM7 }>(addr);
            *mmu_read = shm_offset;
            *mmu_write = shm_offset;
        }

        for addr in (SHARED_WRAM_OFFSET..IO_PORTS_OFFSET).step_by(FAST_MEM_PAGE_SIZE) {
            self.mem.mmu_arm7.vmem.destroy_map(addr as usize, FAST_MEM_PAGE_SIZE);
            let shm_offset = self.mem.wram.get_shm_offset::<{ ARM7 }>(addr);
            self.mem
                .mmu_arm7
                .vmem
                .create_map(&self.mem.shm, shm_offset, addr as usize, FAST_MEM_PAGE_SIZE, true, true, false)
                .unwrap();
        }
    }

    fn remove_vram_arm7(&mut self, start: u32, end: u32) {
        let start = start | VRAM_OFFSET;
        let end = end | VRAM_OFFSET;
        for addr in (start..end).step_by(MMU_PAGE_SIZE) {
            let shifted_addr = (addr as usize) >> MMU_PAGE_SHIFT;
            let mmu_read = unsafe { self.mem.mmu_arm7.mmu_read.get_unchecked_mut(shifted_addr) };
            let mmu_write = unsafe { self.mem.mmu_arm7.mmu_write.get_unchecked_mut(shifted_addr) };
            *mmu_read = 0;
            *mmu_write = 0;
        }
    }

    fn update_vram_arm7(&mut self, start: u32, end: u32) {
        let start = start | VRAM_OFFSET;
        let end = end | VRAM_OFFSET;
        for addr in (start..end).step_by(MMU_PAGE_SIZE) {
            let shifted_addr = (addr as usize) >> MMU_PAGE_SHIFT;
            let mmu_read = unsafe { self.mem.mmu_arm7.mmu_read.get_unchecked_mut(shifted_addr) };
            let mmu_write = unsafe { self.mem.mmu_arm7.mmu_write.get_unchecked_mut(shifted_addr) };

            let shm_offset = self.mem.vram.get_shm_offset::<{ ARM7 }>(addr);

            *mmu_read = shm_offset;
            *mmu_write = shm_offset;
        }
    }
}

impl Emu {
    #[inline(never)]
    pub fn mmu_update_all<const CPU: CpuType>(&mut self) {
        match CPU {
            ARM9 => {
                self.update_all_no_tcm_arm9();
                self.initialize_tcm_arm9();
                self.update_tcm_arm9(0, IO_PORTS_OFFSET);
                self.initialize_tcm_misc_arm9();
            }
            ARM7 => self.update_all_arm7(),
        }
    }

    pub fn mmu_update_itcm<const CPU: CpuType>(&mut self) {
        match CPU {
            ARM9 => self.update_tcm_arm9(regions::ITCM_OFFSET, max(self.mem.mmu_arm9.current_itcm_size, self.cp15.itcm_size)),
            ARM7 => unreachable!(),
        }
    }

    pub fn mmu_update_dtcm<const CPU: CpuType>(&mut self) {
        match CPU {
            ARM9 => {
                self.update_tcm_arm9(self.mem.mmu_arm9.current_dtcm_addr, self.mem.mmu_arm9.current_dtcm_addr + self.mem.mmu_arm9.current_dtcm_size);
                self.update_tcm_arm9(self.cp15.dtcm_addr, self.cp15.dtcm_addr + self.cp15.dtcm_size);
            }
            ARM7 => unreachable!(),
        }
    }

    pub fn mmu_update_wram<const CPU: CpuType>(&mut self) {
        match CPU {
            ARM9 => {
                self.update_wram_no_tcm_arm9();
                self.update_tcm_arm9(SHARED_WRAM_OFFSET, IO_PORTS_OFFSET);
            }
            ARM7 => self.update_wram_arm7(),
        }
    }

    pub fn mmu_get_read<const CPU: CpuType>(&self) -> &[usize] {
        match CPU {
            ARM9 => self.mem.mmu_arm9.mmu_read.as_ref(),
            ARM7 => self.mem.mmu_arm7.mmu_read.as_ref(),
        }
    }

    pub fn mmu_get_read_tcm<const CPU: CpuType>(&self) -> &[usize] {
        match CPU {
            ARM9 => self.mem.mmu_arm9.mmu_read_tcm.as_ref(),
            ARM7 => unreachable!(),
        }
    }

    pub fn mmu_get_write<const CPU: CpuType>(&self) -> &[usize] {
        match CPU {
            ARM9 => self.mem.mmu_arm9.mmu_write.as_ref(),
            ARM7 => self.mem.mmu_arm7.mmu_write.as_ref(),
        }
    }

    pub fn mmu_get_write_tcm<const CPU: CpuType>(&self) -> &[usize] {
        match CPU {
            ARM9 => self.mem.mmu_arm9.mmu_write_tcm.as_ref(),
            ARM7 => self.mem.mmu_arm7.mmu_write.as_ref(),
        }
    }

    pub fn mmu_remove_write<const CPU: CpuType>(&mut self, addr: u32, region: &MemRegion) {
        match CPU {
            ARM9 => {
                remove_mmu_write_entry(addr, region, self.mem.mmu_arm9.mmu_write.as_mut(), None);
                remove_mmu_write_entry(addr, region, self.mem.mmu_arm9.mmu_write_tcm.as_mut(), Some(&mut self.mem.mmu_arm9.vmem_tcm));
            }
            ARM7 => remove_mmu_write_entry(addr, region, self.mem.mmu_arm7.mmu_write.as_mut(), Some(&mut self.mem.mmu_arm7.vmem)),
        }
    }

    pub fn mmu_remove_vram<const CPU: CpuType>(&mut self, start_addr: u32, end_addr: u32) {
        match CPU {
            ARM9 => self.remove_vram_tcm_arm9(start_addr, end_addr),
            ARM7 => self.remove_vram_arm7(start_addr, end_addr),
        }
    }

    pub fn mmu_update_vram<const CPU: CpuType>(&mut self, start_addr: u32, end_addr: u32) {
        match CPU {
            ARM9 => self.update_vram_tcm_arm9(start_addr, end_addr),
            ARM7 => self.update_vram_arm7(start_addr, end_addr),
        }
    }
}
