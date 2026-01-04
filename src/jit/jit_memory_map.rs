use crate::core::memory::regions;
use crate::jit::jit_memory::{JitEntries, JitEntry, JitLiveRanges, BIOS_UNINTERRUPT_ENTRY_ARM7, BIOS_UNINTERRUPT_ENTRY_ARM9, JIT_LIVE_RANGE_PAGE_SIZE_SHIFT};
use crate::utils;
use crate::utils::HeapArrayU32;
use std::cmp::min;
use std::{ptr, slice};

// ARM9 Bios starts at 0xFFFF0000, but just treat everything above OAM region as bios
// Also omit 0xF msb to save more memory
// Also move ARM7 bios to 0xFFF00000, so we don't need separate mappings
const MEMORY_RANGE: u32 = 0x10000000;

pub const BLOCK_SHIFT: usize = 13;
pub const BLOCK_SIZE: usize = 1 << BLOCK_SHIFT;
const SIZE: usize = (MEMORY_RANGE >> 1) as usize / BLOCK_SIZE;
const LIVE_RANGES_SIZE: usize = (MEMORY_RANGE >> (JIT_LIVE_RANGE_PAGE_SIZE_SHIFT + 3)) as usize;

pub struct JitMemoryMap {
    map: HeapArrayU32<SIZE>,
    live_ranges_map: HeapArrayU32<LIVE_RANGES_SIZE>,
}

impl JitMemoryMap {
    pub fn new(entries: &JitEntries, live_ranges: &JitLiveRanges) -> Self {
        let mut instance = JitMemoryMap {
            map: HeapArrayU32::default(),
            live_ranges_map: HeapArrayU32::default(),
        };

        macro_rules! get_ptr {
            ($addr:expr, $entries:expr) => {{
                (unsafe { $entries.as_ptr().add(($addr >> 1) % $entries.len()) } as u32)
            }};
        }

        instance.map[(0xFFF0000) >> BLOCK_SHIFT >> 1] = ptr::addr_of!(BIOS_UNINTERRUPT_ENTRY_ARM9) as u32;
        instance.map[(0xFF00000) >> BLOCK_SHIFT >> 1] = ptr::addr_of!(BIOS_UNINTERRUPT_ENTRY_ARM7) as u32;

        for i in 0..SIZE {
            let addr = (i << BLOCK_SHIFT) << 1;
            let map_ptr = &mut instance.map[i];

            match (addr as u32) & 0x0F000000 {
                regions::ITCM_OFFSET | regions::ITCM_OFFSET2 => *map_ptr = get_ptr!(addr, entries.itcm),
                regions::MAIN_OFFSET => *map_ptr = get_ptr!(addr, entries.main),
                regions::SHARED_WRAM_OFFSET => {
                    if (addr as u32) & regions::ARM7_WRAM_OFFSET == regions::ARM7_WRAM_OFFSET {
                        *map_ptr = get_ptr!(addr, entries.wram_arm7)
                    } else {
                        *map_ptr = get_ptr!(addr, entries.shared_wram_arm7)
                    }
                }
                regions::VRAM_OFFSET => *map_ptr = get_ptr!(addr, entries.vram),
                _ => {}
            }
        }

        macro_rules! get_ptr {
            ($index:expr, $live_ranges:expr) => {{
                (unsafe { $live_ranges.as_ptr().add($index % $live_ranges.len()) } as u32)
            }};
        }

        for i in 0..LIVE_RANGES_SIZE {
            let addr = i << (JIT_LIVE_RANGE_PAGE_SIZE_SHIFT + 3);
            let map_ptr = &mut instance.live_ranges_map[i];

            match (addr as u32) & 0xFF000000 {
                0 | regions::ITCM_OFFSET2 => *map_ptr = get_ptr!(i, live_ranges.itcm),
                regions::MAIN_OFFSET => *map_ptr = get_ptr!(i, live_ranges.main),
                regions::VRAM_OFFSET => *map_ptr = get_ptr!(i, live_ranges.vram),
                _ => {}
            }
        }

        instance
    }

    pub fn get_jit_entry(&self, addr: u32) -> *mut JitEntry {
        let addr = (addr & 0x0FFFFFFF) >> 1;
        unsafe { ((*self.map.get_unchecked((addr >> BLOCK_SHIFT) as usize)) as *mut JitEntry).add((addr as usize) & (BLOCK_SIZE - 1)) }
    }

    pub fn write_jit_entries(&mut self, addr: u32, size: usize, value: JitEntry) {
        let mut addr = (addr & 0x0FFFFFFF) >> 1;
        let mut size = size >> 1;
        while size > 0 {
            let block = self.map[(addr >> BLOCK_SHIFT) as usize] as *mut JitEntry;
            let block_offset = (addr as usize) & (BLOCK_SIZE - 1);
            let block_remaining = BLOCK_SIZE - block_offset;
            let write_size = min(block_remaining, size);
            unsafe { slice::from_raw_parts_mut(block.add(block_offset), write_size).fill(value) };
            addr = utils::align_up(addr as usize, BLOCK_SIZE) as u32;
            size -= write_size;
        }
    }

    pub fn get_live_range(&self, addr: u32) -> *mut u8 {
        unsafe { (*self.live_ranges_map.get_unchecked((addr >> (JIT_LIVE_RANGE_PAGE_SIZE_SHIFT + 3)) as usize)) as _ }
    }

    pub fn has_jit_block(&self, addr: u32) -> bool {
        let live_range = self.get_live_range(addr);
        if live_range.is_null() {
            return false;
        }
        let bit = (addr >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 0x7;
        unsafe { *live_range & (1 << bit) != 0 }
    }

    pub fn get_map_ptr(&self) -> *const JitEntry {
        self.map.as_ptr() as _
    }
}
