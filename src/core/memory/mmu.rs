use crate::core::cp15::TcmState;
use crate::core::emu::{get_cp15, get_mem, Emu};
use crate::core::memory::regions;
use crate::core::memory::regions::{
    ARM7_BIOS_OFFSET, ARM7_BIOS_REGION, ARM9_BIOS_REGION, DTCM_REGION, GBA_RAM_OFFSET, GBA_RAM_REGION, GBA_ROM_REGION, ITCM_REGION, STANDARD_PALETTES_OFFSET, V_MEM_ARM7_RANGE, WIFI_MIRROR_REGION,
    WIFI_REGION,
};
use crate::core::CpuType::{ARM7, ARM9};
use crate::mmap::VirtualMem;
use crate::utils::HeapMem;
use regions::{GBA_ROM_OFFSET, GBA_ROM_OFFSET2, IO_PORTS_OFFSET, MAIN_OFFSET, MAIN_REGION, SHARED_WRAM_OFFSET, SHARED_WRAM_REGION, V_MEM_ARM9_RANGE};
use std::cell::UnsafeCell;
use std::cmp::max;

pub const MMU_PAGE_SHIFT: usize = 14;
pub const MMU_PAGE_SIZE: usize = 16 * 1024;

pub trait Mmu {
    fn update_all(&self, emu: &Emu);
    fn update_itcm(&self, emu: &Emu);
    fn update_dtcm(&self, emu: &Emu);
    fn update_wram(&self, emu: &Emu);
    fn get_base_ptr(&self) -> *mut u8;
    fn get_base_tcm_ptr(&self) -> *mut u8;
    fn get_mmu_read(&self) -> &[bool];
    fn get_mmu_write(&self) -> &[bool];
    fn get_mmu_read_tcm(&self) -> &[bool];
    fn get_mmu_write_tcm(&self) -> &[bool];
}

struct MmuArm9Inner {
    vmem: VirtualMem,
    vmem_tcm: VirtualMem,
    mmu_read: HeapMem<bool, { V_MEM_ARM9_RANGE as usize / MMU_PAGE_SIZE }>,
    mmu_write: HeapMem<bool, { V_MEM_ARM9_RANGE as usize / MMU_PAGE_SIZE }>,
    mmu_read_tcm: HeapMem<bool, { V_MEM_ARM9_RANGE as usize / MMU_PAGE_SIZE }>,
    mmu_write_tcm: HeapMem<bool, { V_MEM_ARM9_RANGE as usize / MMU_PAGE_SIZE }>,
    current_itcm_size: u32,
    current_dtcm_addr: u32,
    current_dtcm_size: u32,
}

impl MmuArm9Inner {
    fn new() -> Self {
        MmuArm9Inner {
            vmem: VirtualMem::new(V_MEM_ARM9_RANGE as _).unwrap(),
            vmem_tcm: VirtualMem::new(V_MEM_ARM9_RANGE as _).unwrap(),
            mmu_read: HeapMem::new(),
            mmu_write: HeapMem::new(),
            mmu_read_tcm: HeapMem::new(),
            mmu_write_tcm: HeapMem::new(),
            current_itcm_size: 0,
            current_dtcm_addr: 0,
            current_dtcm_size: 0,
        }
    }

    fn update_all_no_tcm(&mut self, emu: &Emu) {
        let shm = &get_mem!(emu).shm;

        for addr in (0..V_MEM_ARM9_RANGE).step_by(MMU_PAGE_SIZE) {
            self.vmem.destroy_map(addr as usize, MMU_PAGE_SIZE);

            let mmu_read = &mut self.mmu_read[(addr as usize) >> MMU_PAGE_SHIFT];
            let mmu_write = &mut self.mmu_write[(addr as usize) >> MMU_PAGE_SHIFT];
            *mmu_read = false;
            *mmu_write = false;

            match addr & 0x0F000000 {
                MAIN_OFFSET => {
                    *mmu_read = true;
                    *mmu_write = true;
                }
                GBA_ROM_OFFSET | GBA_ROM_OFFSET2 | GBA_RAM_OFFSET | 0x0F000000 => *mmu_read = true,
                _ => {}
            }
        }

        self.vmem.destroy_region_map(&MAIN_REGION);
        self.vmem.create_region_map(shm, &MAIN_REGION).unwrap();

        self.vmem.destroy_region_map(&GBA_ROM_REGION);
        self.vmem.create_region_map(shm, &GBA_ROM_REGION).unwrap();

        self.vmem.destroy_region_map(&GBA_RAM_REGION);
        self.vmem.create_region_map(shm, &GBA_RAM_REGION).unwrap();

        self.vmem.destroy_region_map(&ARM9_BIOS_REGION);
        self.vmem.create_region_map(shm, &ARM9_BIOS_REGION).unwrap();

        self.update_wram_no_tcm(emu);
    }

    fn update_wram_no_tcm(&mut self, emu: &Emu) {
        let shm = &get_mem!(emu).shm;

        for addr in (SHARED_WRAM_OFFSET..IO_PORTS_OFFSET).step_by(MMU_PAGE_SIZE) {
            self.vmem.destroy_map(addr as usize, MMU_PAGE_SIZE);

            let mmu_read = &mut self.mmu_read[(addr as usize) >> MMU_PAGE_SHIFT];
            let mmu_write = &mut self.mmu_write[(addr as usize) >> MMU_PAGE_SHIFT];

            let shm_offset = get_mem!(emu).wram.get_shm_offset::<{ ARM9 }>(addr);
            if shm_offset != usize::MAX {
                self.vmem.create_map(shm, shm_offset, addr as usize, MMU_PAGE_SIZE, true, true, false).unwrap();
                *mmu_read = true;
                *mmu_write = true;
            } else {
                self.vmem.create_map(shm, 0, addr as usize, MMU_PAGE_SIZE, false, false, false).unwrap();
                *mmu_read = false;
                *mmu_write = false;
            }
        }
    }

    fn update_tcm(&mut self, start: u32, end: u32, emu: &Emu) {
        let shm = &get_mem!(emu).shm;

        for addr in (start..end).step_by(MMU_PAGE_SIZE) {
            self.vmem_tcm.destroy_map(addr as usize, MMU_PAGE_SIZE);

            let mmu_read = &mut self.mmu_read_tcm[(addr as usize) >> MMU_PAGE_SHIFT];
            let mmu_write = &mut self.mmu_write_tcm[(addr as usize) >> MMU_PAGE_SHIFT];
            *mmu_read = false;
            *mmu_write = false;

            let base_addr = addr & !0xFF000000;
            match addr & 0x0F000000 {
                MAIN_OFFSET => {
                    *mmu_read = true;
                    *mmu_write = true;
                    self.vmem_tcm
                        .create_page_map(shm, MAIN_REGION.shm_offset, base_addr as usize, MAIN_REGION.size, addr as usize, MMU_PAGE_SIZE, MAIN_REGION.allow_write)
                        .unwrap();
                }
                SHARED_WRAM_OFFSET => {
                    let shm_offset = get_mem!(emu).wram.get_shm_offset::<{ ARM9 }>(addr);
                    if shm_offset != usize::MAX {
                        self.vmem_tcm
                            .create_page_map(shm, shm_offset, 0, MMU_PAGE_SIZE, addr as usize, MMU_PAGE_SIZE, SHARED_WRAM_REGION.allow_write)
                            .unwrap();
                        *mmu_read = true;
                        *mmu_write = true;
                    } else {
                        self.vmem_tcm.create_map(shm, 0, addr as usize, MMU_PAGE_SIZE, false, false, false).unwrap();
                    }
                }
                GBA_ROM_OFFSET | GBA_ROM_OFFSET2 => {
                    *mmu_read = true;
                    self.vmem_tcm
                        .create_page_map(
                            shm,
                            GBA_ROM_REGION.shm_offset,
                            base_addr as usize,
                            GBA_ROM_REGION.size,
                            addr as usize,
                            MMU_PAGE_SIZE,
                            GBA_ROM_REGION.allow_write,
                        )
                        .unwrap();
                }
                GBA_RAM_OFFSET => {
                    *mmu_read = true;
                    self.vmem_tcm
                        .create_page_map(
                            shm,
                            GBA_RAM_REGION.shm_offset,
                            base_addr as usize,
                            GBA_RAM_REGION.size,
                            addr as usize,
                            MMU_PAGE_SIZE,
                            GBA_RAM_REGION.allow_write,
                        )
                        .unwrap();
                }
                0x0F000000 => {
                    *mmu_read = true;
                    self.vmem_tcm
                        .create_page_map(
                            shm,
                            ARM9_BIOS_REGION.shm_offset,
                            base_addr as usize,
                            ARM9_BIOS_REGION.size,
                            addr as usize,
                            MMU_PAGE_SIZE,
                            ARM9_BIOS_REGION.allow_write,
                        )
                        .unwrap();
                }
                _ => {}
            }

            let cp15 = get_cp15!(emu, ARM9);
            if addr < cp15.itcm_size {
                if cp15.itcm_state == TcmState::RW {
                    *mmu_read = true;
                    *mmu_write = true;
                    self.vmem_tcm
                        .create_page_map(shm, ITCM_REGION.shm_offset, base_addr as usize, ITCM_REGION.size, addr as usize, MMU_PAGE_SIZE, ITCM_REGION.allow_write)
                        .unwrap();
                }
            } else if addr >= cp15.dtcm_addr && addr < cp15.dtcm_addr + cp15.dtcm_size && cp15.dtcm_state == TcmState::RW {
                *mmu_read = true;
                *mmu_write = true;
                self.vmem_tcm
                    .create_page_map(shm, DTCM_REGION.shm_offset, base_addr as usize, DTCM_REGION.size, addr as usize, MMU_PAGE_SIZE, DTCM_REGION.allow_write)
                    .unwrap();
            }
        }

        let cp15 = get_cp15!(emu, ARM9);
        self.current_itcm_size = cp15.itcm_size;
        self.current_dtcm_addr = cp15.dtcm_addr;
        self.current_dtcm_size = cp15.dtcm_size;
    }
}

pub struct MmuArm9 {
    inner: UnsafeCell<MmuArm9Inner>,
}

impl MmuArm9 {
    pub fn new() -> Self {
        MmuArm9 {
            inner: UnsafeCell::new(MmuArm9Inner::new()),
        }
    }
}

impl Mmu for MmuArm9 {
    #[inline(never)]
    fn update_all(&self, emu: &Emu) {
        unsafe {
            (*self.inner.get()).update_all_no_tcm(emu);
            (*self.inner.get()).update_tcm(0, V_MEM_ARM9_RANGE, emu)
        }
    }

    fn update_itcm(&self, emu: &Emu) {
        let inner = unsafe { self.inner.get().as_mut().unwrap_unchecked() };
        inner.update_tcm(regions::ITCM_OFFSET, max(inner.current_itcm_size, get_cp15!(emu, ARM9).itcm_size), emu);
    }

    fn update_dtcm(&self, emu: &Emu) {
        let inner = unsafe { self.inner.get().as_mut().unwrap_unchecked() };
        inner.update_tcm(inner.current_dtcm_addr, inner.current_dtcm_addr + inner.current_dtcm_size, emu);
        let cp15 = get_cp15!(emu, ARM9);
        inner.update_tcm(cp15.dtcm_addr, cp15.dtcm_addr + cp15.dtcm_size, emu);
    }

    fn update_wram(&self, emu: &Emu) {
        unsafe {
            (*self.inner.get()).update_wram_no_tcm(emu);
            (*self.inner.get()).update_tcm(SHARED_WRAM_OFFSET, IO_PORTS_OFFSET, emu);
        }
    }

    fn get_base_ptr(&self) -> *mut u8 {
        unsafe { (*self.inner.get()).vmem.as_mut_ptr() }
    }

    fn get_base_tcm_ptr(&self) -> *mut u8 {
        unsafe { (*self.inner.get()).vmem_tcm.as_mut_ptr() }
    }

    fn get_mmu_read(&self) -> &[bool] {
        unsafe { (*self.inner.get()).mmu_read.as_ref() }
    }

    fn get_mmu_read_tcm(&self) -> &[bool] {
        unsafe { (*self.inner.get()).mmu_read_tcm.as_ref() }
    }

    fn get_mmu_write(&self) -> &[bool] {
        unsafe { (*self.inner.get()).mmu_write.as_ref() }
    }

    fn get_mmu_write_tcm(&self) -> &[bool] {
        unsafe { (*self.inner.get()).mmu_write_tcm.as_ref() }
    }
}

struct MmuArm7Inner {
    vmem: VirtualMem,
    mmu_read: HeapMem<bool, { V_MEM_ARM7_RANGE as usize / MMU_PAGE_SIZE }>,
    mmu_write: HeapMem<bool, { V_MEM_ARM7_RANGE as usize / MMU_PAGE_SIZE }>,
}

impl MmuArm7Inner {
    fn new() -> Self {
        MmuArm7Inner {
            vmem: VirtualMem::new(V_MEM_ARM7_RANGE as usize).unwrap(),
            mmu_read: HeapMem::new(),
            mmu_write: HeapMem::new(),
        }
    }

    fn update_all(&mut self, emu: &Emu) {
        let shm = &get_mem!(emu).shm;

        for addr in (0..V_MEM_ARM7_RANGE).step_by(MMU_PAGE_SIZE) {
            self.vmem.destroy_map(addr as usize, MMU_PAGE_SIZE);

            let mmu_read = &mut self.mmu_read[(addr as usize) >> MMU_PAGE_SHIFT];
            let mmu_write = &mut self.mmu_write[(addr as usize) >> MMU_PAGE_SHIFT];
            *mmu_read = false;
            *mmu_write = false;

            match addr & 0x0F000000 {
                ARM7_BIOS_OFFSET | 0x01000000 | GBA_ROM_OFFSET | GBA_ROM_OFFSET2 | GBA_RAM_OFFSET => *mmu_read = true,
                MAIN_OFFSET => {
                    *mmu_read = true;
                    *mmu_write = true;
                }
                _ => {}
            }
        }

        self.vmem.destroy_region_map(&ARM7_BIOS_REGION);
        self.vmem.create_region_map(shm, &ARM7_BIOS_REGION).unwrap();

        self.vmem.destroy_region_map(&MAIN_REGION);
        self.vmem.create_region_map(shm, &MAIN_REGION).unwrap();

        // This aligns the underlying pages of the vita to 8kb for the wifi regions
        // Otherwise it will crash when cleaning them up
        for addr in (IO_PORTS_OFFSET..STANDARD_PALETTES_OFFSET).step_by(8 * 1024) {
            self.vmem.destroy_map(addr as usize, 8 * 1024);
            self.vmem.create_map(shm, 0, addr as usize, 8 * 1024, false, false, false).unwrap();
        }

        self.vmem.destroy_region_map(&WIFI_REGION);
        self.vmem.create_region_map(shm, &WIFI_REGION).unwrap();

        self.vmem.destroy_region_map(&WIFI_MIRROR_REGION);
        self.vmem.create_region_map(shm, &WIFI_MIRROR_REGION).unwrap();

        self.vmem.destroy_region_map(&GBA_ROM_REGION);
        self.vmem.create_region_map(shm, &GBA_ROM_REGION).unwrap();

        self.vmem.destroy_region_map(&GBA_RAM_REGION);
        self.vmem.create_region_map(shm, &GBA_RAM_REGION).unwrap();

        self.update_wram(emu);
    }

    fn update_wram(&mut self, emu: &Emu) {
        let shm = &get_mem!(emu).shm;

        for addr in (SHARED_WRAM_OFFSET..IO_PORTS_OFFSET).step_by(MMU_PAGE_SIZE) {
            self.vmem.destroy_map(addr as usize, MMU_PAGE_SIZE);

            let mmu_read = &mut self.mmu_read[(addr as usize) >> MMU_PAGE_SHIFT];
            let mmu_write = &mut self.mmu_write[(addr as usize) >> MMU_PAGE_SHIFT];
            *mmu_read = true;
            *mmu_write = true;

            let shm_offset = get_mem!(emu).wram.get_shm_offset::<{ ARM7 }>(addr);
            self.vmem.create_map(shm, shm_offset, addr as usize, MMU_PAGE_SIZE, true, true, false).unwrap();
        }
    }
}

pub struct MmuArm7 {
    inner: UnsafeCell<MmuArm7Inner>,
}

impl MmuArm7 {
    pub fn new() -> Self {
        MmuArm7 {
            inner: UnsafeCell::new(MmuArm7Inner::new()),
        }
    }
}

impl Mmu for MmuArm7 {
    #[inline(never)]
    fn update_all(&self, emu: &Emu) {
        unsafe { (*self.inner.get()).update_all(emu) };
    }

    fn update_itcm(&self, _: &Emu) {
        unreachable!()
    }

    fn update_dtcm(&self, _: &Emu) {
        unreachable!()
    }

    fn update_wram(&self, emu: &Emu) {
        unsafe { (*self.inner.get()).update_wram(emu) };
    }

    fn get_base_ptr(&self) -> *mut u8 {
        unsafe { (*self.inner.get()).vmem.as_mut_ptr() }
    }

    fn get_base_tcm_ptr(&self) -> *mut u8 {
        unreachable!()
    }

    fn get_mmu_read(&self) -> &[bool] {
        unsafe { (*self.inner.get()).mmu_read.as_ref() }
    }

    fn get_mmu_read_tcm(&self) -> &[bool] {
        unreachable!()
    }

    fn get_mmu_write(&self) -> &[bool] {
        unsafe { (*self.inner.get()).mmu_write.as_ref() }
    }

    fn get_mmu_write_tcm(&self) -> &[bool] {
        unreachable!()
    }
}
