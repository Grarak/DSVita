use crate::core::memory::regions;
use crate::core::CpuType;
use crate::jit::jit_memory::{JitEntries, JitEntry, JitLiveRanges, BIOS_UNINTERRUPT_ENTRY_ARM7, BIOS_UNINTERRUPT_ENTRY_ARM9, JIT_LIVE_RANGE_PAGE_SIZE_SHIFT};
use crate::utils::HeapMemU32;
use CpuType::{ARM7, ARM9};

// ARM9 Bios starts at 0xFFFF0000, but just treat everything above OAM region as bios
// Also omit 0xF msb to save more memory
const MEMORY_RANGE_ARM9: u32 = 0x10000000;
const MEMORY_RANGE_ARM7: u32 = regions::OAM_OFFSET;

pub const BLOCK_SHIFT: usize = 13;
pub const BLOCK_SIZE: usize = 1 << BLOCK_SHIFT;
const SIZE_ARM9: usize = (MEMORY_RANGE_ARM9 >> 1) as usize / BLOCK_SIZE;
const SIZE_ARM7: usize = (MEMORY_RANGE_ARM7 >> 1) as usize / BLOCK_SIZE;
const LIVE_RANGES_SIZE_ARM9: usize = (MEMORY_RANGE_ARM9 >> (JIT_LIVE_RANGE_PAGE_SIZE_SHIFT + 3)) as usize;
const LIVE_RANGES_SIZE_ARM7: usize = (MEMORY_RANGE_ARM7 >> (JIT_LIVE_RANGE_PAGE_SIZE_SHIFT + 3)) as usize;

const BIOS_UNINTERRUPT_ENTRIES_ARM9: [JitEntry; BLOCK_SIZE] = [BIOS_UNINTERRUPT_ENTRY_ARM9; BLOCK_SIZE];
const BIOS_UNINTERRUPT_ENTRIES_ARM7: [JitEntry; BLOCK_SIZE] = [BIOS_UNINTERRUPT_ENTRY_ARM7; BLOCK_SIZE];

pub struct JitMemoryMap {
    map_arm9: HeapMemU32<SIZE_ARM9>,
    map_arm7: HeapMemU32<SIZE_ARM7>,
    live_ranges_map_arm9: HeapMemU32<LIVE_RANGES_SIZE_ARM9>,
    live_ranges_map_arm7: HeapMemU32<LIVE_RANGES_SIZE_ARM7>,
}

impl JitMemoryMap {
    pub fn new(entries: &JitEntries, live_ranges: &JitLiveRanges) -> Self {
        let mut instance = JitMemoryMap {
            map_arm9: HeapMemU32::new(),
            map_arm7: HeapMemU32::new(),
            live_ranges_map_arm9: HeapMemU32::new(),
            live_ranges_map_arm7: HeapMemU32::new(),
        };

        macro_rules! get_ptr {
            ($addr:expr, $entries:expr) => {{
                (unsafe { $entries.as_ptr().add(($addr >> 1) % $entries.len()) } as u32)
            }};
        }

        for i in 0..SIZE_ARM9 {
            let addr = (i << BLOCK_SHIFT) << 1;
            let arm9_ptr = &mut instance.map_arm9[i];

            match (addr as u32) & 0x0F000000 {
                regions::ITCM_OFFSET | regions::ITCM_OFFSET2 => *arm9_ptr = get_ptr!(addr, entries.itcm),
                regions::MAIN_OFFSET => *arm9_ptr = get_ptr!(addr, entries.main_arm9),
                0x0F000000 => *arm9_ptr = BIOS_UNINTERRUPT_ENTRIES_ARM9.as_ptr() as u32,
                _ => {}
            }
        }

        for i in 0..SIZE_ARM7 {
            let addr = (i << BLOCK_SHIFT) << 1;
            let arm7_ptr = &mut instance.map_arm7[i];

            match (addr as u32) & 0x0F000000 {
                0 => *arm7_ptr = BIOS_UNINTERRUPT_ENTRIES_ARM7.as_ptr() as u32,
                regions::MAIN_OFFSET => *arm7_ptr = get_ptr!(addr, entries.main_arm7),
                regions::SHARED_WRAM_OFFSET => {
                    if (addr as u32) & regions::ARM7_WRAM_OFFSET == regions::ARM7_WRAM_OFFSET {
                        *arm7_ptr = get_ptr!(addr, entries.wram_arm7)
                    } else {
                        *arm7_ptr = get_ptr!(addr, entries.shared_wram_arm7)
                    }
                }
                regions::VRAM_OFFSET => *arm7_ptr = get_ptr!(addr, entries.vram_arm7),
                _ => {}
            }
        }

        macro_rules! get_ptr {
            ($index:expr, $live_ranges:expr) => {{
                (unsafe { $live_ranges.as_ptr().add($index % $live_ranges.len()) } as u32)
            }};
        }

        for i in 0..LIVE_RANGES_SIZE_ARM9 {
            let addr = i << (JIT_LIVE_RANGE_PAGE_SIZE_SHIFT + 3);
            let arm9_ptr = &mut instance.live_ranges_map_arm9[i];

            match (addr as u32) & 0xFF000000 {
                0 | regions::ITCM_OFFSET2 => *arm9_ptr = get_ptr!(i, live_ranges.itcm),
                regions::MAIN_OFFSET => *arm9_ptr = get_ptr!(i, live_ranges.main),
                _ => {}
            }
        }

        for i in 0..LIVE_RANGES_SIZE_ARM7 {
            let addr = i << (JIT_LIVE_RANGE_PAGE_SIZE_SHIFT + 3);
            let arm7_ptr = &mut instance.live_ranges_map_arm7[i];

            match (addr as u32) & 0xFF000000 {
                regions::MAIN_OFFSET => *arm7_ptr = get_ptr!(i, live_ranges.main),
                regions::SHARED_WRAM_OFFSET => {
                    if (addr as u32) & regions::ARM7_WRAM_OFFSET == regions::ARM7_WRAM_OFFSET {
                        *arm7_ptr = get_ptr!(i, live_ranges.wram_arm7)
                    } else {
                        *arm7_ptr = get_ptr!(i, live_ranges.shared_wram_arm7)
                    }
                }
                regions::VRAM_OFFSET => *arm7_ptr = get_ptr!(i, live_ranges.vram_arm7),
                _ => {}
            }
        }

        instance
    }

    pub fn get_jit_entry<const CPU: CpuType>(&self, addr: u32) -> *mut JitEntry {
        let addr = (addr & 0x0FFFFFFF) >> 1;
        macro_rules! get_jit_entry {
            ($map:expr) => {{
                unsafe { ((*$map.get_unchecked((addr >> BLOCK_SHIFT) as usize)) as *mut JitEntry).add((addr as usize) & (BLOCK_SIZE - 1)) }
            }};
        }
        match CPU {
            ARM9 => get_jit_entry!(self.map_arm9),
            ARM7 => get_jit_entry!(self.map_arm7),
        }
    }

    pub fn get_live_range<const CPU: CpuType>(&self, addr: u32) -> *mut u8 {
        macro_rules! get_live_range {
            ($map:expr) => {{
                unsafe { (*$map.get_unchecked((addr >> (JIT_LIVE_RANGE_PAGE_SIZE_SHIFT + 3)) as usize)) as _ }
            }};
        }
        match CPU {
            ARM9 => get_live_range!(self.live_ranges_map_arm9),
            ARM7 => get_live_range!(self.live_ranges_map_arm7),
        }
    }

    pub fn get_map_ptr<const CPU: CpuType>(&self) -> *const JitEntry {
        match CPU {
            ARM9 => self.map_arm9.as_ptr() as _,
            ARM7 => self.map_arm7.as_ptr() as _,
        }
    }
}
