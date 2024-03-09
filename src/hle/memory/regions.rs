pub const INSTRUCTION_TCM_OFFSET: u32 = 0x00000000;
pub const INSTRUCTION_TCM_MIRROR_OFFSET: u32 = 0x01000000;
pub const INSTRUCTION_TCM_SIZE: u32 = 32 * 1024;
pub const DATA_TCM_SIZE: u32 = 16 * 1024;

pub const MAIN_MEMORY_OFFSET: u32 = 0x02000000;
pub const MAIN_MEMORY_SIZE: u32 = 4 * 1024 * 1024;

pub const SHARED_WRAM_OFFSET: u32 = 0x03000000;
pub const SHARED_WRAM_SIZE: u32 = 32 * 1024;

pub const ARM7_WRAM_OFFSET: u32 = 0x03800000;
pub const ARM7_WRAM_SIZE: u32 = 64 * 1024;

pub const IO_PORTS_OFFSET: u32 = 0x04000000;

pub const STANDARD_PALETTES_OFFSET: u32 = 0x05000000;
pub const STANDARD_PALETTES_SIZE: u32 = 2 * 1024;

pub const VRAM_OFFSET: u32 = 0x06000000;

pub const OAM_OFFSET: u32 = 0x07000000;
pub const OAM_SIZE: u32 = 2 * 1024;

pub const GBA_ROM_OFFSET: u32 = 0x08000000;
pub const GBA_ROM_SIZE: u32 = 32 * 1024 * 1024;
pub const GBA_ROM_OFFSET2: u32 = 0x09000000;
pub const GBA_RAM_OFFSET: u32 = 0x0A000000;
pub const GBA_RAM_SIZE: u32 = 64 * 1024;

pub const ARM9_BIOS_OFFSET: u32 = 0xFFFF0000;
pub const ARM9_BIOS_SIZE: u32 = 32 * 1024;
pub const ARM7_BIOS_OFFSET: u32 = 0x00000000;
pub const ARM7_BIOS_SIZE: u32 = 16 * 1024;
