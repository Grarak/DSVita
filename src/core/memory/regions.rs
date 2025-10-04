use crate::core::memory::mmu::FAST_MEM_PAGE_SIZE;
use crate::core::memory::vram;
use crate::mmap::MemRegion;

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

pub const TOTAL_MEM_SIZE: u32 = 16 * 1024 /* Some padding for mmu */
        + ITCM_SIZE + DTCM_SIZE + MAIN_SIZE + SHARED_WRAM_SIZE + ARM7_WRAM_SIZE + WIFI_RAM_SIZE
        + FAST_MEM_PAGE_SIZE as u32 /* Palettes */
        + FAST_MEM_PAGE_SIZE as u32 /* OAM */
        + vram::TOTAL_SIZE as u32
        + FAST_MEM_PAGE_SIZE as u32 /* GBA ROM/RAM, filled with 0xFF */
        + FAST_MEM_PAGE_SIZE as u32 /* Both BIOSES, filled with 0x0 */
;

const P_ITCM_OFFSET: usize = 16 * 1024;
const P_DTCM_OFFSET: usize = P_ITCM_OFFSET + ITCM_SIZE as usize;
const P_MAIN_OFFSET: usize = P_DTCM_OFFSET + DTCM_SIZE as usize;
const P_SHARED_WRAM_OFFSET: usize = P_MAIN_OFFSET + MAIN_SIZE as usize;
const P_ARM7_WRAM_OFFSET: usize = P_SHARED_WRAM_OFFSET + SHARED_WRAM_SIZE as usize;
const P_WIFI_RAM_OFFSET: usize = P_ARM7_WRAM_OFFSET + ARM7_WRAM_SIZE as usize;
const P_PALETTES_OFFSET: usize = P_WIFI_RAM_OFFSET + WIFI_RAM_SIZE as usize;
const P_VRAM_OFFSET: usize = P_PALETTES_OFFSET + FAST_MEM_PAGE_SIZE;
const P_OAM_OFFSET: usize = P_VRAM_OFFSET + vram::TOTAL_SIZE;
const P_GBA_ROM_OFFSET: usize = P_OAM_OFFSET + FAST_MEM_PAGE_SIZE;
const P_ARM9_BIOS_OFFSET: usize = P_GBA_ROM_OFFSET + FAST_MEM_PAGE_SIZE;
const P_ARM7_BIOS_OFFSET: usize = P_ARM9_BIOS_OFFSET;

pub const ITCM_REGION: MemRegion = MemRegion::new(ITCM_OFFSET as usize, MAIN_OFFSET as usize, ITCM_SIZE as usize, P_ITCM_OFFSET, true);
pub const DTCM_REGION: MemRegion = MemRegion::new(0, 0, DTCM_SIZE as usize, P_DTCM_OFFSET, true);
pub const MAIN_REGION: MemRegion = MemRegion::new(MAIN_OFFSET as usize, SHARED_WRAM_OFFSET as usize, MAIN_SIZE as usize, P_MAIN_OFFSET, true);
pub const SHARED_WRAM_REGION: MemRegion = MemRegion::new(0, 0, SHARED_WRAM_SIZE as usize, P_SHARED_WRAM_OFFSET, true);
pub const SHARED_WRAM_ARM7_REGION: MemRegion = MemRegion::new(SHARED_WRAM_OFFSET as usize, ARM7_WRAM_OFFSET as usize, SHARED_WRAM_SIZE as usize, P_SHARED_WRAM_OFFSET, true);
pub const ARM7_WRAM_REGION: MemRegion = MemRegion::new(ARM7_WRAM_OFFSET as usize, IO_PORTS_OFFSET as usize, ARM7_WRAM_SIZE as usize, P_ARM7_WRAM_OFFSET, true);
pub const PALETTES_REGION: MemRegion = MemRegion::new(STANDARD_PALETTES_OFFSET as usize, VRAM_OFFSET as usize, FAST_MEM_PAGE_SIZE, P_PALETTES_OFFSET, true);
pub const VRAM_REGION: MemRegion = MemRegion::new(VRAM_OFFSET as usize, OAM_OFFSET as usize, vram::TOTAL_SIZE, P_VRAM_OFFSET, true);
pub const OAM_REGION: MemRegion = MemRegion::new(OAM_OFFSET as usize, GBA_ROM_OFFSET as usize, FAST_MEM_PAGE_SIZE, P_OAM_OFFSET, true);
pub const GBA_ROM_REGION: MemRegion = MemRegion::new(GBA_ROM_OFFSET as usize, GBA_RAM_OFFSET as usize, FAST_MEM_PAGE_SIZE, P_GBA_ROM_OFFSET, false);
pub const GBA_RAM_REGION: MemRegion = MemRegion::new(GBA_RAM_OFFSET as usize, 0x0B000000, FAST_MEM_PAGE_SIZE, P_GBA_ROM_OFFSET, false);
pub const ARM9_BIOS_REGION: MemRegion = MemRegion::new(0x0F000000, 0x10000000, FAST_MEM_PAGE_SIZE, P_ARM9_BIOS_OFFSET, false);
pub const ARM7_BIOS_REGION: MemRegion = MemRegion::new(ARM7_BIOS_OFFSET as usize, MAIN_OFFSET as usize, FAST_MEM_PAGE_SIZE, P_ARM7_BIOS_OFFSET, false);
pub const WIFI_REGION: MemRegion = MemRegion::new(WIFI_RAM_OFFSET as usize, (WIFI_RAM_OFFSET + WIFI_RAM_SIZE) as usize, WIFI_RAM_SIZE as usize, P_WIFI_RAM_OFFSET, true);
pub const WIFI_MIRROR_REGION: MemRegion = MemRegion::new(WIFI_RAM_OFFSET2 as usize, (WIFI_RAM_OFFSET2 + WIFI_RAM_SIZE) as usize, WIFI_RAM_SIZE as usize, P_WIFI_RAM_OFFSET, true);

pub const V_MEM_ARM9_RANGE: u32 = 0x10000000;
pub const V_MEM_ARM7_RANGE: u32 = 0x0B000000;
