pub struct MemoryRegion {
    pub name: &'static str,
    pub offset: u32,
    pub size: u32,
}

impl MemoryRegion {
    pub const fn new(name: &'static str, offset: u32, size: u32) -> Self {
        MemoryRegion { name, offset, size }
    }
}

const INSTRUCTION_TCM_REGION: MemoryRegion =
    MemoryRegion::new("Instruction TCM", 0x00000000, 32 * 1024);
const DATA_TCM_REGION: MemoryRegion = MemoryRegion::new("Data TCM", 0, 16 * 1024);

pub const MAIN_MEMORY_REGION: MemoryRegion =
    MemoryRegion::new("Main Memory", 0x02000000, 4 * 1024 * 1024);

pub const SHARED_WRAM_OFFSET: u32 = 0x03000000;
const SHARED_WRAM_REGION: MemoryRegion =
    MemoryRegion::new("Shared WRAM", SHARED_WRAM_OFFSET, 32 * 1024);
const ARM7_WRAM_REGION: MemoryRegion = MemoryRegion::new("ARM7-WRAM", 0x03800000, 64 * 1024);

pub const ARM9_IO_PORTS_OFFSET: u32 = 0x04000000;
const ARM9_IO_PORTS_REGION: MemoryRegion =
    MemoryRegion::new("ARM9-I/O Ports", ARM9_IO_PORTS_OFFSET, 0);
const ARM7_IO_PORTS_REGION: MemoryRegion = MemoryRegion::new("ARM7-I/O Ports", 0x04000000, 0);

const WIRELESS_COM_WAIT_STATE_0: MemoryRegion =
    MemoryRegion::new("Wireless Communications Wait State 0", 0x04800000, 8 * 1024);
const WIRELESS_COM_WAIT_STATE_1: MemoryRegion =
    MemoryRegion::new("Wireless Communications Wait State 1", 0x04808000, 0);

pub const STANDARD_PALETTES_OFFSET: u32 = 0x05000000;
const STANDARD_PALETTES_REGION: MemoryRegion =
    MemoryRegion::new("Standard Palettes", STANDARD_PALETTES_OFFSET, 2 * 1024);

const ARM7_VRAM_WORK_RAM: MemoryRegion =
    MemoryRegion::new("VRAM allocated as Work RAM to ARM7", 0x06000000, 256 * 1024);

pub const VRAM_ENGINE_A_BG_OFFSET: u32 = 0x06000000;
pub const VRAM_ENGINE_B_BG_OFFSET: u32 = 0x06200000;
pub const VRAM_ENGINE_A_OBJ_OFFSET: u32 = 0x06400000;
pub const VRAM_ENGINE_B_OBJ_OFFSET: u32 = 0x06600000;
pub const VRAM_LCDC_ALLOCATED_OFFSET: u32 = 0x06800000;
const VRAM_ENGINE_A_BG_REGION: MemoryRegion = MemoryRegion::new(
    "VRAM - Engine A, BG VRAM",
    VRAM_ENGINE_A_BG_OFFSET,
    512 * 1024,
);
const VRAM_ENGINE_B_BG_REGION: MemoryRegion = MemoryRegion::new(
    "VRAM - Engine B, BG VRAM",
    VRAM_ENGINE_B_BG_OFFSET,
    128 * 1024,
);
const VRAM_ENGINE_A_OBJ_REGION: MemoryRegion = MemoryRegion::new(
    "VRAM - Engine A, OBJ VRAM",
    VRAM_ENGINE_A_OBJ_OFFSET,
    256 * 1024,
);
const VRAM_ENGINE_B_OBJ_REGION: MemoryRegion = MemoryRegion::new(
    "VRAM - Engine B, OBJ VRAM",
    VRAM_ENGINE_B_OBJ_OFFSET,
    128 * 1024,
);
const VRAM_LCDC_ALLOCATED_REGION: MemoryRegion = MemoryRegion::new(
    "VRAM - \"LCDC\"-allocated",
    VRAM_LCDC_ALLOCATED_OFFSET,
    656 * 1024,
);

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
