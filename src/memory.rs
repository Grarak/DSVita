use crate::mmap::Mmap;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::{io, slice};

pub struct MemoryRegion {
    pub name: &'static str,
    pub offset: u32,
    pub size: u32,
}

impl MemoryRegion {
    const fn new(name: &'static str, offset: u32, size: u32) -> Self {
        MemoryRegion { name, offset, size }
    }
}

const INSTRUCTION_TCM_REGION: MemoryRegion =
    MemoryRegion::new("Instruction TCM", 0x00000000, 32 * 1024);
const DATA_TCM_REGION: MemoryRegion = MemoryRegion::new("Data TCM", 0, 16 * 1024);

pub const MAIN_MEMORY_REGION: MemoryRegion =
    MemoryRegion::new("Main Memory", 0x02000000, 4 * 1024 * 1024);

const SHARED_WRAM_REGION: MemoryRegion = MemoryRegion::new("Shared WRAM", 0x03000000, 32 * 1024);
const ARM7_WRAM_REGION: MemoryRegion = MemoryRegion::new("ARM7-WRAM", 0x03800000, 64 * 1024);

const ARM9_IO_PORTS_REGION: MemoryRegion = MemoryRegion::new("ARM9-I/O Ports", 0x04000000, 0);
const ARM7_IO_PORTS_REGION: MemoryRegion = MemoryRegion::new("ARM7-I/O Ports", 0x04000000, 0);

const WIRELESS_COM_WAIT_STATE_0: MemoryRegion =
    MemoryRegion::new("Wireless Communications Wait State 0", 0x04800000, 8 * 1024);
const WIRELESS_COM_WAIT_STATE_1: MemoryRegion =
    MemoryRegion::new("Wireless Communications Wait State 1", 0x04808000, 0);

const STANDARD_PALETTES_REGION: MemoryRegion =
    MemoryRegion::new("Standard Palettes", 0x05000000, 2 * 1024);

const ARM7_VRAM_WORK_RAM: MemoryRegion =
    MemoryRegion::new("VRAM allocated as Work RAM to ARM7", 0x06000000, 256 * 1024);

const VRAM_ENGINE_A_BG_REGION: MemoryRegion =
    MemoryRegion::new("VRAM - Engine A, BG VRAM", 0x06000000, 512 * 1024);
const VRAM_ENGINE_B_BG_REGION: MemoryRegion =
    MemoryRegion::new("VRAM - Engine B, BG VRAM", 0x06200000, 128 * 1024);
const VRAM_ENGINE_A_OBJ_REGION: MemoryRegion =
    MemoryRegion::new("VRAM - Engine A, OBJ VRAM", 0x06400000, 256 * 1024);
const VRAM_ENGINE_B_OBJ_REGION: MemoryRegion =
    MemoryRegion::new("VRAM - Engine B, OBJ VRAM", 0x06600000, 128 * 1024);
const VRAM_LCDC_ALLOCATED_REGION: MemoryRegion =
    MemoryRegion::new("VRAM - \"LCDC\"-allocated", 0x06800000, 656 * 1024);

const OAM_REGION: MemoryRegion = MemoryRegion::new("OAM", 0x07000000, 2 * 1024);

const GBA_SLOT_ROM_REGION: MemoryRegion =
    MemoryRegion::new("GBA Slot ROM", 0x08000000, 32 * 1024 * 1024);
const GBA_SLOT_RAM_REGION: MemoryRegion = MemoryRegion::new("GBA Slot RAM", 0x0A000000, 64 * 1024);
const ARM9_BIOS_REGION: MemoryRegion = MemoryRegion::new("ARM9-BIOS", 0xFFFF0000, 32 * 1024);

pub const ARM9_REGIONS: [&'static MemoryRegion; 10] = [
    &MAIN_MEMORY_REGION,
    &SHARED_WRAM_REGION,
    &ARM9_IO_PORTS_REGION,
    &STANDARD_PALETTES_REGION,
    &VRAM_ENGINE_A_BG_REGION,
    &VRAM_ENGINE_B_BG_REGION,
    &VRAM_ENGINE_A_OBJ_REGION,
    &VRAM_ENGINE_B_OBJ_REGION,
    &VRAM_LCDC_ALLOCATED_REGION,
    &OAM_REGION,
];

pub const ARM7_REGIONS: [&'static MemoryRegion; 7] = [
    &MAIN_MEMORY_REGION,
    &SHARED_WRAM_REGION,
    &ARM7_WRAM_REGION,
    &ARM7_IO_PORTS_REGION,
    &WIRELESS_COM_WAIT_STATE_0,
    &WIRELESS_COM_WAIT_STATE_1,
    &ARM7_VRAM_WORK_RAM,
];

pub struct VMmap<'a> {
    vm_begin_addr: *const u8,
    offset: u32,
    size: usize,
    phantom_data: PhantomData<&'a u8>,
}

impl<'a> VMmap<'a> {
    fn new(vm_begin_addr: *const u8, offset: u32, size: usize) -> Self {
        VMmap {
            vm_begin_addr,
            offset,
            size,
            phantom_data: PhantomData,
        }
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        (self.vm_begin_addr as u32 - self.offset) as _
    }

    pub fn as_ptr(&self) -> *const u8 {
        (self.vm_begin_addr as u32 - self.offset) as _
    }

    pub fn len(&self) -> usize {
        self.size + self.offset as usize
    }
}

impl<'a> Deref for VMmap<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
}

impl<'a> DerefMut for VMmap<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) }
    }
}

impl<'a> AsRef<[u8]> for VMmap<'a> {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl<'a> AsMut<[u8]> for VMmap<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.deref_mut()
    }
}

pub struct VmManager {
    regions: &'static [&'static MemoryRegion],
    pub vm: Mmap,
}

impl VmManager {
    pub fn new(name: &str, regions: &'static [&'static MemoryRegion]) -> io::Result<Self> {
        let vm = Mmap::new(
            name,
            false,
            regions[regions.len() - 1].offset + regions[regions.len() - 1].size - regions[0].offset,
        )?;

        Ok(VmManager { regions, vm })
    }

    pub fn offset(&self) -> u32 {
        self.regions[0].offset
    }

    pub fn vm_begin_addr(&self) -> *const u8 {
        self.vm.as_ptr()
    }

    pub fn get_vm_mapping(&self) -> VMmap<'_> {
        VMmap::new(self.vm_begin_addr(), self.offset(), self.vm.len())
    }
}
