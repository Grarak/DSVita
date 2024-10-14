use crate::core::memory::regions;
use crate::mmap::{ShmMem, VirtualMem, VirtualMemMap};
use std::io;

fn create_mapping(shm_mem: &ShmMem, vmem: &mut VirtualMem, region: &regions::MemRegion) -> io::Result<VirtualMemMap> {
    vmem.create_mapping(shm_mem, region.p_offset, region.size as usize, region.start as usize, region.end as usize, region.allow_write)
}

trait Vmem {
    fn main(&mut self) -> &mut VirtualMemMap;
}

pub struct VmemArm9 {
    vmem: VirtualMem,
    pub itcm: VirtualMemMap,
    pub main: VirtualMemMap,
    pub gba_rom: VirtualMemMap,
    pub bios: VirtualMemMap,
}

impl VmemArm9 {
    pub fn new(shm_mem: &ShmMem) -> Self {
        let mut vmem = VirtualMem::new(regions::V_MEM_ARM9_RANGE as usize).unwrap();
        let itcm = create_mapping(shm_mem, &mut vmem, &regions::ITCM_REGION).unwrap();
        let main = create_mapping(shm_mem, &mut vmem, &regions::MAIN_REGION).unwrap();
        let gba_rom = create_mapping(shm_mem, &mut vmem, &regions::GBA_ROM_REGION).unwrap();
        let bios = create_mapping(shm_mem, &mut vmem, &regions::ARM9_BIOS_REGION).unwrap();
        VmemArm9 { vmem, itcm, main, gba_rom, bios }
    }
}

impl Vmem for VmemArm9 {
    fn main(&mut self) -> &mut VirtualMemMap {
        &mut self.main
    }
}

pub struct VmemArm7 {
    vmem: VirtualMem,
    pub bios: VirtualMemMap,
    pub main: VirtualMemMap,
    pub wram: VirtualMemMap,
    pub wifi: VirtualMemMap,
    pub gba_rom: VirtualMemMap,
}

impl VmemArm7 {
    pub fn new(shm_mem: &ShmMem) -> Self {
        let mut vmem = VirtualMem::new(regions::V_MEM_ARM7_RANGE as usize).unwrap();
        let bios = create_mapping(shm_mem, &mut vmem, &regions::ARM7_BIOS_REGION).unwrap();
        let main = create_mapping(shm_mem, &mut vmem, &regions::MAIN_REGION).unwrap();
        let wram = create_mapping(shm_mem, &mut vmem, &regions::ARM7_WRAM_REGION).unwrap();
        let wifi = create_mapping(shm_mem, &mut vmem, &regions::WIFI_REGION).unwrap();
        create_mapping(shm_mem, &mut vmem, &regions::WIFI_MIRROR_REGION).unwrap();
        let gba_rom = create_mapping(shm_mem, &mut vmem, &regions::GBA_ROM_REGION).unwrap();
        VmemArm7 {
            vmem,
            bios,
            main,
            wram,
            wifi,
            gba_rom,
        }
    }
}

impl Vmem for VmemArm7 {
    fn main(&mut self) -> &mut VirtualMemMap {
        &mut self.main
    }
}
