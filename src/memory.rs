use crate::mmap::Mmap;
use std::io;

pub struct MemoryRegion<'a> {
    pub name: &'a str,
    pub offset: u32,
    pub size: u32,
}

impl<'a> MemoryRegion<'a> {
    const fn new(name: &'a str, offset: u32, size: u32) -> Self {
        MemoryRegion { name, offset, size }
    }
}

const INSTRUCTION_TCM_REGION: MemoryRegion =
    MemoryRegion::new("Instruction TCM", 0x00000000, 32 * 1024);
static DATA_TCM_REGION: MemoryRegion = MemoryRegion::new("Data TCM", 0, 16 * 1024);
pub const MAIN_MEMORY_REGION: MemoryRegion =
    MemoryRegion::new("Main Memory", 0x02000000, 4 * 1024 * 1024);
const SHARED_WRAM_REGION: MemoryRegion = MemoryRegion::new("Shared WRAM", 0x03000000, 32 * 1024);
const ARM9_IO_PORTS_REGION: MemoryRegion = MemoryRegion::new("ARM9-I/O Ports", 0x04000000, 0);
const STANDARD_PALETTES_REGION: MemoryRegion =
    MemoryRegion::new("Standard Palettes", 0x05000000, 2 * 1024);
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

const GUEST_MEMORY_SIZE: u32 = OAM_REGION.offset + OAM_REGION.size;

#[inline]
pub fn allocate_memory_layout() -> io::Result<Mmap> {
    Mmap::new(
        "memory_layout",
        false,
        OAM_REGION.offset - MAIN_MEMORY_REGION.offset,
    )
}
