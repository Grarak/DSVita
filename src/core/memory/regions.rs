use crate::core::memory::vram;

pub const ITCM_OFFSET: u32 = 0x00000000;
pub const ITCM_OFFSET2: u32 = 0x01000000;
pub const ITCM_SIZE: u32 = 32 * 1024;
pub const DTCM_SIZE: u32 = 16 * 1024;

pub const MAIN_OFFSET: u32 = 0x02000000;
pub const MAIN_SIZE: u32 = 4 * 1024 * 1024;

pub const SHARED_WRAM_OFFSET: u32 = 0x03000000;
pub const SHARED_WRAM_SIZE: u32 = 32 * 1024;

pub const ARM7_WRAM_OFFSET: u32 = 0x03800000;
pub const ARM7_WRAM_SIZE: u32 = 64 * 1024;

pub const IO_PORTS_OFFSET: u32 = 0x04000000;
pub const WIFI_IO_OFFSET: u32 = 0x04800000;
pub const WIFI_RAM_OFFSET: u32 = 0x04804000;
pub const WIFI_RAM_OFFSET2: u32 = 0x04808000;
pub const WIFI_RAM_SIZE: u32 = 8 * 1024;

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

pub struct MemRegion {
    pub start: u32,
    pub end: u32,
    pub size: u32,
    pub p_offset: usize,
    pub allow_write: bool,
}

impl MemRegion {
    pub const fn new(start: u32, end: u32, size: u32, p_offset: usize, allow_write: bool) -> Self {
        MemRegion {
            start,
            end,
            size,
            p_offset,
            allow_write,
        }
    }
}

pub const TOTAL_MEM_SIZE: u32 =
    ITCM_SIZE + DTCM_SIZE + MAIN_SIZE + SHARED_WRAM_SIZE + ARM7_WRAM_SIZE + WIFI_RAM_SIZE + vram::TOTAL_SIZE as u32 + GBA_ROM_SIZE + GBA_RAM_SIZE + ARM9_BIOS_SIZE + ARM7_BIOS_SIZE;

const P_ITCM_OFFSET: usize = 0;
const P_DTCM_OFFSET: usize = P_ITCM_OFFSET + ITCM_SIZE as usize;
const P_MAIN_OFFSET: usize = P_DTCM_OFFSET + DTCM_SIZE as usize;
const P_SHARED_WRAM_OFFSET: usize = P_MAIN_OFFSET + MAIN_SIZE as usize;
const P_ARM7_WRAM_OFFSET: usize = P_SHARED_WRAM_OFFSET + SHARED_WRAM_SIZE as usize;
const P_WIFI_RAM_OFFSET: usize = P_ARM7_WRAM_OFFSET + ARM7_WRAM_SIZE as usize;
const P_VRAM_OFFSET: usize = P_WIFI_RAM_OFFSET + WIFI_RAM_SIZE as usize;
const P_GBA_ROM_OFFSET: usize = P_VRAM_OFFSET + vram::TOTAL_SIZE;
const P_GBA_RAM_OFFSET: usize = P_GBA_ROM_OFFSET + GBA_ROM_SIZE as usize;
const P_ARM9_BIOS_OFFSET: usize = P_GBA_RAM_OFFSET + GBA_RAM_SIZE as usize;
const P_ARM7_BIOS_OFFSET: usize = P_ARM9_BIOS_OFFSET + ARM9_BIOS_SIZE as usize;

pub const ITCM_REGION: MemRegion = MemRegion::new(ITCM_OFFSET, MAIN_OFFSET, ITCM_SIZE, P_ITCM_OFFSET, true);
pub const MAIN_REGION: MemRegion = MemRegion::new(MAIN_OFFSET, SHARED_WRAM_OFFSET, MAIN_SIZE, P_MAIN_OFFSET, true);
pub const ARM7_WRAM_REGION: MemRegion = MemRegion::new(ARM7_WRAM_OFFSET, IO_PORTS_OFFSET, ARM7_WRAM_SIZE, P_ARM7_WRAM_OFFSET, true);
pub const GBA_ROM_REGION: MemRegion = MemRegion::new(GBA_ROM_OFFSET, GBA_RAM_OFFSET, GBA_ROM_SIZE, P_GBA_ROM_OFFSET, false);
pub const ARM9_BIOS_REGION: MemRegion = MemRegion::new(0xF0000000, 0x10000000, ARM9_BIOS_SIZE, P_ARM9_BIOS_OFFSET, false);
pub const ARM7_BIOS_REGION: MemRegion = MemRegion::new(ARM7_BIOS_OFFSET, MAIN_OFFSET, ARM7_BIOS_SIZE, P_ARM7_BIOS_OFFSET, true);
pub const WIFI_REGION: MemRegion = MemRegion::new(WIFI_RAM_OFFSET, WIFI_RAM_OFFSET + WIFI_RAM_SIZE, WIFI_RAM_SIZE, P_WIFI_RAM_OFFSET, true);
pub const WIFI_MIRROR_REGION: MemRegion = MemRegion::new(WIFI_RAM_OFFSET2, WIFI_RAM_OFFSET2 + WIFI_RAM_SIZE, WIFI_RAM_SIZE, P_WIFI_RAM_OFFSET, true);

pub const V_MEM_REGIONS_ARM9: [MemRegion; 11] = [
    ITCM_REGION,
    MemRegion::new(0, 0, DTCM_SIZE, P_DTCM_OFFSET, true),
    MAIN_REGION,
    MemRegion::new(0, 0, SHARED_WRAM_SIZE, P_SHARED_WRAM_OFFSET, true),
    MemRegion::new(IO_PORTS_OFFSET, STANDARD_PALETTES_OFFSET, 0, 0, true),
    MemRegion::new(STANDARD_PALETTES_OFFSET, VRAM_OFFSET, 0, 0, true),
    MemRegion::new(0, 0, vram::TOTAL_SIZE as u32, P_VRAM_OFFSET, true),
    MemRegion::new(OAM_OFFSET, GBA_ROM_OFFSET, 0, 0, true),
    GBA_ROM_REGION,
    MemRegion::new(0, 0, GBA_RAM_SIZE, P_GBA_RAM_OFFSET, true),
    ARM9_BIOS_REGION,
];

pub const V_MEM_REGIONS_ARM7: [MemRegion; 10] = [
    ARM7_BIOS_REGION,
    MAIN_REGION,
    MemRegion::new(0, 0, SHARED_WRAM_SIZE, P_SHARED_WRAM_OFFSET, true),
    MemRegion::new(ARM7_WRAM_OFFSET, IO_PORTS_OFFSET, ARM7_WRAM_SIZE, P_ARM7_WRAM_OFFSET, true),
    MemRegion::new(IO_PORTS_OFFSET, WIFI_IO_OFFSET, 0, 0, true),
    WIFI_REGION,
    WIFI_MIRROR_REGION,
    MemRegion::new(0, 0, vram::TOTAL_SIZE as u32, P_VRAM_OFFSET, true),
    GBA_ROM_REGION,
    MemRegion::new(0, 0, GBA_RAM_SIZE, P_GBA_RAM_OFFSET, true),
];

pub const V_MEM_ARM9_RANGE: u32 = 0x10000000;
pub const V_MEM_ARM7_RANGE: u32 = 0x0B000000;
