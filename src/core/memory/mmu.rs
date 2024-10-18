use crate::core::cp15::TcmState;
use crate::core::emu::{get_cp15, get_mem, Emu};
use crate::core::memory::regions;
use crate::core::memory::regions::{ARM7_BIOS_REGION, ARM9_BIOS_REGION, DTCM_REGION, GBA_ROM_REGION, ITCM_REGION, V_MEM_ARM7_RANGE, WIFI_REGION};
use crate::core::CpuType::{ARM7, ARM9};
use crate::mmap::{VirtualMem, PAGE_SIZE};
use regions::{ARM7_BIOS_OFFSET, GBA_ROM_OFFSET, GBA_ROM_OFFSET2, IO_PORTS_OFFSET, MAIN_OFFSET, MAIN_REGION, SHARED_WRAM_OFFSET, SHARED_WRAM_REGION, V_MEM_ARM9_RANGE};
use std::cell::UnsafeCell;
use std::cmp::max;
use std::intrinsics::unlikely;

pub trait Mmu {
    fn update_all(&self, emu: &Emu);
    fn update_itcm(&self, emu: &Emu);
    fn update_dtcm(&self, emu: &Emu);
    fn update_wram(&self, emu: &Emu);
    fn get_base_ptr(&self) -> *mut u8;
    fn get_base_tcm_ptr(&self) -> *mut u8;
}

struct MmuArm9Inner {
    vmem: VirtualMem,
    vmem_tcm: VirtualMem,
    current_itcm_size: u32,
    current_dtcm_addr: u32,
    current_dtcm_size: u32,
}

impl MmuArm9Inner {
    fn new() -> Self {
        MmuArm9Inner {
            vmem: VirtualMem::new(V_MEM_ARM9_RANGE as _).unwrap(),
            vmem_tcm: VirtualMem::new(V_MEM_ARM9_RANGE as _).unwrap(),
            current_itcm_size: 0,
            current_dtcm_addr: 0,
            current_dtcm_size: 0,
        }
    }

    fn update(&mut self, start: u32, end: u32, emu: &Emu) {
        let shm = &get_mem!(emu).shm;

        for addr in (start..end).step_by(PAGE_SIZE) {
            let base_addr = addr & !0xFF000000;
            match addr & 0x0F000000 {
                MAIN_OFFSET => {
                    for vmem in [&mut self.vmem, &mut self.vmem_tcm] {
                        vmem.destroy_page_mapping(addr as usize);
                        vmem.create_page_mapping(shm, MAIN_REGION.shm_offset, base_addr as usize, MAIN_REGION.size, addr as usize, MAIN_REGION.allow_write)
                            .unwrap();
                    }
                }
                SHARED_WRAM_OFFSET => {
                    let shm_offset = get_mem!(emu).wram.get_shm_offset::<{ ARM9 }>(addr);
                    for vmem in [&mut self.vmem, &mut self.vmem_tcm] {
                        vmem.destroy_page_mapping(addr as usize);
                    }
                    if shm_offset != usize::MAX {
                        println!("arm9 map {addr:x} {shm_offset:x}");
                        for vmem in [&mut self.vmem, &mut self.vmem_tcm] {
                            vmem.create_page_mapping(shm, shm_offset, 0, PAGE_SIZE, addr as usize, SHARED_WRAM_REGION.allow_write).unwrap();
                        }
                        println!("arm9 map finish");
                    }
                }
                GBA_ROM_OFFSET | GBA_ROM_OFFSET2 => {
                    for vmem in [&mut self.vmem, &mut self.vmem_tcm] {
                        vmem.destroy_page_mapping(addr as usize);
                        vmem.create_page_mapping(shm, GBA_ROM_REGION.shm_offset, base_addr as usize, GBA_ROM_REGION.size, addr as usize, GBA_ROM_REGION.allow_write)
                            .unwrap();
                    }
                }
                0x0F000000 => {
                    for vmem in [&mut self.vmem, &mut self.vmem_tcm] {
                        vmem.destroy_page_mapping(addr as usize);
                        vmem.create_page_mapping(shm, ARM9_BIOS_REGION.shm_offset, base_addr as usize, ARM9_BIOS_REGION.size, addr as usize, ARM9_BIOS_REGION.allow_write)
                            .unwrap();
                    }
                }
                _ => {}
            }

            let cp15 = get_cp15!(emu, ARM9);
            if addr < cp15.itcm_size {
                if cp15.itcm_state == TcmState::RW {
                    self.vmem_tcm.destroy_page_mapping(addr as usize);
                    self.vmem_tcm
                        .create_page_mapping(shm, ITCM_REGION.shm_offset, base_addr as usize, ITCM_REGION.size, addr as usize, ITCM_REGION.allow_write)
                        .unwrap();
                }
            } else if addr >= cp15.dtcm_addr && addr < cp15.dtcm_addr + cp15.dtcm_size && cp15.dtcm_state == TcmState::RW {
                self.vmem_tcm.destroy_page_mapping(addr as usize);
                self.vmem_tcm
                    .create_page_mapping(shm, DTCM_REGION.shm_offset, base_addr as usize, DTCM_REGION.size, addr as usize, DTCM_REGION.allow_write)
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
        unsafe { (*self.inner.get()).update(0, V_MEM_ARM9_RANGE, emu) };
    }

    fn update_itcm(&self, emu: &Emu) {
        let inner = unsafe { self.inner.get().as_mut().unwrap_unchecked() };
        inner.update(regions::ITCM_OFFSET, max(inner.current_itcm_size, get_cp15!(emu, ARM9).itcm_size), emu);
    }

    fn update_dtcm(&self, emu: &Emu) {
        let inner = unsafe { self.inner.get().as_mut().unwrap_unchecked() };
        inner.update(inner.current_dtcm_addr, inner.current_dtcm_addr + inner.current_dtcm_size, emu);
        let cp15 = get_cp15!(emu, ARM9);
        inner.update(cp15.dtcm_addr, cp15.dtcm_addr + cp15.dtcm_size, emu);
    }

    fn update_wram(&self, emu: &Emu) {
        unsafe { (*self.inner.get()).update(SHARED_WRAM_OFFSET, IO_PORTS_OFFSET, emu) };
    }

    fn get_base_ptr(&self) -> *mut u8 {
        unsafe { (*self.inner.get()).vmem.as_mut_ptr() }
    }

    fn get_base_tcm_ptr(&self) -> *mut u8 {
        unsafe { (*self.inner.get()).vmem_tcm.as_mut_ptr() }
    }
}

struct MmuArm7Inner {
    vmem: VirtualMem,
}

impl MmuArm7Inner {
    fn new() -> Self {
        MmuArm7Inner {
            vmem: VirtualMem::new(V_MEM_ARM7_RANGE as _).unwrap(),
        }
    }

    fn update(&mut self, start: u32, end: u32, emu: &Emu) {
        let shm = &get_mem!(emu).shm;

        for addr in (start..end).step_by(PAGE_SIZE) {
            let base_addr = addr & !0xFF000000;
            match addr & 0x0F000000 {
                ARM7_BIOS_OFFSET => {
                    self.vmem.destroy_page_mapping(addr as usize);
                    self.vmem
                        .create_page_mapping(shm, ARM7_BIOS_REGION.shm_offset, base_addr as usize, ARM7_BIOS_REGION.size, addr as usize, ARM7_BIOS_REGION.allow_write)
                        .unwrap();
                }
                MAIN_OFFSET => {
                    self.vmem.destroy_page_mapping(addr as usize);
                    self.vmem
                        .create_page_mapping(shm, MAIN_REGION.shm_offset, base_addr as usize, MAIN_REGION.size, addr as usize, MAIN_REGION.allow_write)
                        .unwrap();
                }
                SHARED_WRAM_OFFSET => {
                    let shm_offset = get_mem!(emu).wram.get_shm_offset::<{ ARM7 }>(addr);
                    println!("arm7 map {addr:x} {shm_offset:x}");
                    self.vmem.destroy_page_mapping(addr as usize);
                    self.vmem.create_page_mapping(shm, shm_offset, 0, PAGE_SIZE, addr as usize, SHARED_WRAM_REGION.allow_write).unwrap();
                    println!("arm7 map finish");
                }
                IO_PORTS_OFFSET => {
                    if unlikely(addr >= regions::WIFI_IO_OFFSET) {
                        let addr = addr & !0x8000;
                        if addr >= 0x4804000 && addr < 0x4806000 {
                            self.vmem.destroy_page_mapping(addr as usize);
                            self.vmem
                                .create_page_mapping(shm, WIFI_REGION.shm_offset, base_addr as usize, WIFI_REGION.size, addr as usize, WIFI_REGION.allow_write)
                                .unwrap();
                        }
                    }
                }
                GBA_ROM_OFFSET | GBA_ROM_OFFSET2 => {
                    self.vmem.destroy_page_mapping(addr as usize);
                    self.vmem
                        .create_page_mapping(shm, GBA_ROM_REGION.shm_offset, base_addr as usize, GBA_ROM_REGION.size, addr as usize, GBA_ROM_REGION.allow_write)
                        .unwrap();
                }
                _ => {}
            }
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
        unsafe { (*self.inner.get()).update(0, V_MEM_ARM7_RANGE, emu) };
    }

    fn update_itcm(&self, _: &Emu) {
        unreachable!()
    }

    fn update_dtcm(&self, _: &Emu) {
        unreachable!()
    }

    fn update_wram(&self, emu: &Emu) {
        unsafe { (*self.inner.get()).update(SHARED_WRAM_OFFSET, IO_PORTS_OFFSET, emu) };
    }

    fn get_base_ptr(&self) -> *mut u8 {
        unsafe { (*self.inner.get()).vmem.as_mut_ptr() }
    }

    fn get_base_tcm_ptr(&self) -> *mut u8 {
        unreachable!()
    }
}
