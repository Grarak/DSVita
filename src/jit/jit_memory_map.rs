use crate::core::memory::regions;
use crate::core::CpuType;
use crate::jit::jit_memory::{JitEntries, JitEntry, JitLiveRanges, BIOS_UNINTERRUPT_ENTRY_ARM7, BIOS_UNINTERRUPT_ENTRY_ARM9, JIT_LIVE_RANGE_PAGE_SIZE_SHIFT};
use crate::utils::HeapMemU32;
use CpuType::{ARM7, ARM9};

pub const BLOCK_SHIFT: usize = 13;
pub const BLOCK_SIZE: usize = 1 << BLOCK_SHIFT;
const SIZE: usize = (1 << 31) / BLOCK_SIZE;
const LIVE_RANGES_SIZE: usize = 1 << (32 - JIT_LIVE_RANGE_PAGE_SIZE_SHIFT - 5);

const BIOS_UNINTERRUPT_ENTRIES_ARM9: [JitEntry; BLOCK_SIZE] = [BIOS_UNINTERRUPT_ENTRY_ARM9; BLOCK_SIZE];
const BIOS_UNINTERRUPT_ENTRIES_ARM7: [JitEntry; BLOCK_SIZE] = [BIOS_UNINTERRUPT_ENTRY_ARM7; BLOCK_SIZE];

pub struct JitMemoryMap {
    map_arm9: HeapMemU32<SIZE>,
    map_arm7: HeapMemU32<SIZE>,
    live_ranges_map_arm9: HeapMemU32<LIVE_RANGES_SIZE>,
    live_ranges_map_arm7: HeapMemU32<LIVE_RANGES_SIZE>,
}

impl JitMemoryMap {
    pub fn new(entries: &JitEntries, live_ranges: &JitLiveRanges) -> Self {
        let mut instance = JitMemoryMap {
            map_arm9: HeapMemU32::new(),
            map_arm7: HeapMemU32::new(),
            live_ranges_map_arm9: HeapMemU32::new(),
            live_ranges_map_arm7: HeapMemU32::new(),
        };

        for i in 0..SIZE {
            let addr = (i << BLOCK_SHIFT) << 1;
            let arm9_ptr = &mut instance.map_arm9[i];
            let arm7_ptr = &mut instance.map_arm7[i];

            macro_rules! get_ptr {
                ($entries:expr) => {{
                    (unsafe { $entries.as_ptr().add((addr >> 1) % $entries.len()) } as u32)
                }};
            }

            match (addr as u32) & 0xFF000000 {
                0 => {
                    *arm9_ptr = get_ptr!(entries.itcm);
                    *arm7_ptr = BIOS_UNINTERRUPT_ENTRIES_ARM7.as_ptr() as u32;
                }
                regions::INSTRUCTION_TCM_MIRROR_OFFSET => *arm9_ptr = get_ptr!(entries.itcm),
                regions::MAIN_MEMORY_OFFSET => {
                    *arm9_ptr = get_ptr!(entries.main_arm9);
                    *arm7_ptr = get_ptr!(entries.main_arm7);
                }
                regions::SHARED_WRAM_OFFSET => *arm7_ptr = get_ptr!(entries.wram),
                regions::VRAM_OFFSET => *arm7_ptr = get_ptr!(entries.vram_arm7),
                0xFF000000 => *arm9_ptr = BIOS_UNINTERRUPT_ENTRIES_ARM9.as_ptr() as u32,
                _ => {}
            }
        }

        for i in 0..LIVE_RANGES_SIZE {
            let addr = i << (JIT_LIVE_RANGE_PAGE_SIZE_SHIFT + 5);
            let arm9_ptr = &mut instance.live_ranges_map_arm9[i];
            let arm7_ptr = &mut instance.live_ranges_map_arm7[i];

            macro_rules! get_ptr {
                ($live_ranges:expr) => {{
                    (unsafe { $live_ranges.as_ptr().add(i % $live_ranges.len()) } as u32)
                }};
            }

            match (addr as u32) & 0xFF000000 {
                0 => {
                    *arm9_ptr = get_ptr!(live_ranges.itcm);
                }
                regions::INSTRUCTION_TCM_MIRROR_OFFSET => *arm9_ptr = get_ptr!(live_ranges.itcm),
                regions::MAIN_MEMORY_OFFSET => {
                    *arm9_ptr = get_ptr!(live_ranges.main);
                    *arm7_ptr = get_ptr!(live_ranges.main);
                }
                regions::SHARED_WRAM_OFFSET => *arm7_ptr = get_ptr!(live_ranges.wram),
                regions::VRAM_OFFSET => *arm7_ptr = get_ptr!(live_ranges.vram_arm7),
                _ => {}
            }
        }

        instance
    }

    pub fn get_jit_entry<const CPU: CpuType>(&self, addr: u32) -> *mut JitEntry {
        let addr = addr >> 1;
        macro_rules! get_jit_entry {
            ($map:expr) => {{
                unsafe { ($map[(addr >> BLOCK_SHIFT) as usize] as *mut JitEntry).add((addr as usize) & (BLOCK_SIZE - 1)) }
            }};
        }
        match CPU {
            ARM9 => get_jit_entry!(self.map_arm9),
            ARM7 => get_jit_entry!(self.map_arm7),
        }
    }

    pub fn get_live_range<const CPU: CpuType>(&self, addr: u32) -> *mut u32 {
        match CPU {
            ARM9 => self.live_ranges_map_arm9[(addr >> (JIT_LIVE_RANGE_PAGE_SIZE_SHIFT + 5)) as usize] as _,
            ARM7 => self.live_ranges_map_arm7[(addr >> (JIT_LIVE_RANGE_PAGE_SIZE_SHIFT + 5)) as usize] as _,
        }
    }

    pub fn get_map_ptr<const CPU: CpuType>(&self) -> *const JitEntry {
        match CPU {
            ARM9 => self.map_arm9.as_ptr() as _,
            ARM7 => self.map_arm7.as_ptr() as _,
        }
    }
}
