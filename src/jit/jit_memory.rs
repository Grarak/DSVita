use crate::core::memory::{regions, vram};
use crate::core::CpuType;
use crate::jit::jit_asm::emit_code_block;
use crate::jit::jit_memory_map::JitMemoryMap;
use crate::logging::debug_println;
use crate::mmap::Mmap;
use crate::utils::{HeapMem, HeapMemU32};
use crate::{utils, DEBUG_LOG};
use lazy_static::lazy_static;
use paste::paste;
use std::intrinsics::unlikely;
use std::marker::ConstParamTy;
use std::{ptr, slice};
use CpuType::{ARM7, ARM9};

const JIT_MEMORY_SIZE: usize = 16 * 1024 * 1024;
pub const JIT_LIVE_RANGE_PAGE_SIZE_SHIFT: u32 = 8;
const JIT_LIVE_RANGE_PAGE_SIZE: u32 = 1 << JIT_LIVE_RANGE_PAGE_SIZE_SHIFT;

#[derive(ConstParamTy, Eq, PartialEq)]
pub enum JitRegion {
    Itcm,
    Main,
    Wram,
    VramArm7,
}

#[derive(Clone, Default)]
struct JitCycle {
    pre_cycle_sum: u16,
    inst_cycle_count: u8,
}

#[derive(Copy, Clone)]
pub struct JitEntry(pub *const extern "C" fn(bool));

impl Default for JitEntry {
    fn default() -> Self {
        JitEntry(ptr::null())
    }
}

#[cfg(target_os = "linux")]
lazy_static! {
    static ref PAGE_SIZE: usize = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as _ };
}
#[cfg(target_os = "vita")]
lazy_static! {
    static ref PAGE_SIZE: usize = 16;
}

const DEFAULT_JIT_ENTRY_ARM9: JitEntry = JitEntry(emit_code_block::<{ ARM9 }> as _);
const DEFAULT_JIT_ENTRY_ARM7: JitEntry = JitEntry(emit_code_block::<{ ARM7 }> as _);

macro_rules! create_jit_blocks {
    ($([$block_name:ident, $size:expr, $default_block:expr]),+) => {
        paste! {
            pub struct JitEntries {
                $(
                    pub $block_name: HeapMem<JitEntry, { $size as usize / 2 }>,
                )*
            }

            impl JitEntries {
                fn new() -> Self {
                    let mut instance = JitEntries {
                        $(
                            $block_name: HeapMem::new(),
                        )*
                    };
                    instance.reset();
                    instance
                }

                fn reset(&mut self) {
                    $(
                        self.$block_name.fill($default_block);
                    )*
                }
            }
        }
    };
}

create_jit_blocks!(
    [itcm, regions::INSTRUCTION_TCM_SIZE, DEFAULT_JIT_ENTRY_ARM9],
    [main_arm9, regions::MAIN_MEMORY_SIZE, DEFAULT_JIT_ENTRY_ARM9],
    [arm9_bios, regions::ARM9_BIOS_SIZE, DEFAULT_JIT_ENTRY_ARM9],
    [main_arm7, regions::MAIN_MEMORY_SIZE, DEFAULT_JIT_ENTRY_ARM7],
    [wram, regions::SHARED_WRAM_SIZE + regions::ARM7_WRAM_SIZE, DEFAULT_JIT_ENTRY_ARM7],
    [vram_arm7, vram::ARM7_SIZE, DEFAULT_JIT_ENTRY_ARM7],
    [arm7_bios, regions::ARM7_BIOS_SIZE, DEFAULT_JIT_ENTRY_ARM7]
);

#[derive(Default)]
pub struct JitLiveRanges {
    pub itcm: HeapMemU32<{ (regions::INSTRUCTION_TCM_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 32) as usize }>,
    pub main: HeapMemU32<{ (regions::MAIN_MEMORY_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 32) as usize }>,
    pub wram: HeapMemU32<{ ((regions::SHARED_WRAM_SIZE + regions::ARM7_WRAM_SIZE) / JIT_LIVE_RANGE_PAGE_SIZE / 32) as usize }>,
    pub vram_arm7: HeapMemU32<{ (vram::ARM7_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 32) as usize }>,
    pub arm9_bios: HeapMemU32<{ (regions::ARM9_BIOS_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 32) as usize }>,
    pub arm7_bios: HeapMemU32<{ (regions::ARM7_BIOS_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 32) as usize }>,
}

#[cfg(target_os = "linux")]
extern "C" {
    fn built_in_clear_cache(start: *const u8, end: *const u8);
}

pub struct JitMemory {
    mem: Mmap,
    mem_offset: usize,
    jit_entries: JitEntries,
    jit_live_ranges: JitLiveRanges,
    pub jit_memory_map: JitMemoryMap,
}

impl JitMemory {
    pub fn new() -> Self {
        let jit_entries = JitEntries::new();
        let jit_live_ranges = JitLiveRanges::default();
        let jit_memory_map = JitMemoryMap::new(&jit_entries, &jit_live_ranges);
        JitMemory {
            mem: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            mem_offset: 0,
            jit_entries,
            jit_live_ranges,
            jit_memory_map,
        }
    }

    fn allocate_block(&mut self, required_size: usize) -> usize {
        if self.mem_offset + required_size >= JIT_MEMORY_SIZE {
            debug_println!("Jit memory full, reset");

            self.mem_offset = 0;

            self.jit_entries.reset();
            self.jit_live_ranges.itcm.fill(0);
            self.jit_live_ranges.main.fill(0);
            self.jit_live_ranges.wram.fill(0);
            self.jit_live_ranges.vram_arm7.fill(0);
            self.jit_live_ranges.arm7_bios.fill(0);
            self.jit_live_ranges.arm9_bios.fill(0);

            let addr = self.mem_offset;
            self.mem_offset += required_size;
            addr
        } else {
            let addr = self.mem_offset;
            self.mem_offset += required_size;
            addr
        }
    }

    fn insert(&mut self, opcodes: &[u32]) -> (usize, usize) {
        let aligned_size = utils::align_up(size_of_val(opcodes), *PAGE_SIZE);
        let allocated_offset_addr = self.allocate_block(aligned_size);

        utils::write_to_mem_slice(&mut self.mem, allocated_offset_addr, opcodes);
        self.flush_cache(allocated_offset_addr, aligned_size);

        (allocated_offset_addr, aligned_size)
    }

    pub fn insert_block<const CPU: CpuType>(&mut self, opcodes: &[u32], guest_pc: u32) -> *const extern "C" fn(bool) {
        let (allocated_offset_addr, aligned_size) = self.insert(opcodes);

        macro_rules! insert {
            ($entries:expr, $live_ranges:expr) => {{
                let jit_entry_addr = (allocated_offset_addr + self.mem.as_ptr() as usize) as *const extern "C" fn(bool);

                let entries_index = (guest_pc >> 1) as usize;
                let entries_index = entries_index % $entries.len();
                $entries[entries_index] = JitEntry(jit_entry_addr);
                assert_eq!(ptr::addr_of!($entries[entries_index]), self.jit_memory_map.get_jit_entry::<CPU>(guest_pc));

                // >> 5 for u32 (each bit represents a page)
                let live_ranges_index = ((guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) >> 5) as usize;
                let live_ranges_index = live_ranges_index % $live_ranges.len();
                let live_ranges_bit = (guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 31;
                $live_ranges[live_ranges_index] |= 1 << live_ranges_bit;
                assert_eq!(ptr::addr_of!($live_ranges[live_ranges_index]), self.jit_memory_map.get_live_range::<CPU>(guest_pc));

                jit_entry_addr
            }};
        }

        let jit_addr = match CPU {
            ARM9 => match guest_pc & 0xFF000000 {
                regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => {
                    insert!(self.jit_entries.itcm, self.jit_live_ranges.itcm)
                }
                regions::MAIN_MEMORY_OFFSET => insert!(self.jit_entries.main_arm9, self.jit_live_ranges.main),
                0xFF000000 => insert!(self.jit_entries.arm9_bios, self.jit_live_ranges.arm9_bios),
                _ => todo!("{:x}", guest_pc),
            },
            ARM7 => match guest_pc & 0xFF000000 {
                regions::ARM7_BIOS_OFFSET => insert!(self.jit_entries.arm7_bios, self.jit_live_ranges.arm7_bios),
                regions::MAIN_MEMORY_OFFSET => insert!(self.jit_entries.main_arm7, self.jit_live_ranges.main),
                regions::SHARED_WRAM_OFFSET => insert!(self.jit_entries.wram, self.jit_live_ranges.wram),
                regions::VRAM_OFFSET => insert!(self.jit_entries.vram_arm7, self.jit_live_ranges.vram_arm7),
                _ => todo!("{:x}", guest_pc),
            },
        };

        if DEBUG_LOG {
            let per = (self.mem_offset * 100) as f32 / JIT_MEMORY_SIZE as f32;
            debug_println!(
                "Insert new jit ({:x}) block with size {} at {:x}, {}% allocated with guest pc {:x}",
                self.mem.as_ptr() as u32,
                aligned_size,
                allocated_offset_addr,
                per,
                guest_pc
            );
        }

        jit_addr
    }

    pub fn get_jit_start_addr<const CPU: CpuType>(&self, guest_pc: u32) -> *const extern "C" fn(bool) {
        unsafe { (*self.jit_memory_map.get_jit_entry::<CPU>(guest_pc)).0 }
    }

    pub fn invalidate_block<const REGION: JitRegion>(&mut self, guest_addr: u32, size: usize) {
        macro_rules! invalidate {
            ($guest_addr:expr, $live_range:ident, $cpu:expr, [$(($cpu_entry:expr, $entries:ident)),+]) => {{
                let live_range = unsafe { self.jit_memory_map.get_live_range::<{ $cpu }>($guest_addr).as_mut_unchecked() };
                let live_ranges_bit = ($guest_addr >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 31;
                if unlikely(*live_range & (1 << live_ranges_bit) != 0) {
                    *live_range &= !(1 << live_ranges_bit);

                    let guest_addr_start = $guest_addr & !(JIT_LIVE_RANGE_PAGE_SIZE - 1);
                    $(
                        let jit_entry_start = self.jit_memory_map.get_jit_entry::<{ $cpu_entry }>(guest_addr_start);
                        unsafe { slice::from_raw_parts_mut(jit_entry_start, JIT_LIVE_RANGE_PAGE_SIZE as usize).fill(
                            match $cpu_entry {
                                ARM9 => DEFAULT_JIT_ENTRY_ARM9,
                                ARM7 => DEFAULT_JIT_ENTRY_ARM7,
                            }
                        ) }
                    )*
                }
            }};
        }

        match REGION {
            JitRegion::Itcm => {
                invalidate!(guest_addr, itcm, ARM9, [(ARM9, itcm)]);
                invalidate!(guest_addr + size as u32 - 1, itcm, ARM9, [(ARM9, itcm)]);
            }
            JitRegion::Main => {
                invalidate!(guest_addr, main, ARM9, [(ARM9, main_arm9), (ARM7, main_arm7)]);
                invalidate!(guest_addr + size as u32 - 1, main, ARM9, [(ARM9, main_arm9), (ARM7, main_arm7)]);
            }
            JitRegion::Wram => {
                invalidate!(guest_addr, wram, ARM7, [(ARM7, wram)]);
                invalidate!(guest_addr + size as u32 - 1, wram, ARM7, [(ARM7, wram)]);
            }
            JitRegion::VramArm7 => {
                invalidate!(guest_addr, vram_arm7, ARM7, [(ARM7, vram_arm7)]);
                invalidate!(guest_addr + size as u32 - 1, vram_arm7, ARM7, [(ARM7, vram_arm7)]);
            }
        }
    }

    pub fn invalidate_wram(&mut self) {
        self.jit_entries.wram.fill(DEFAULT_JIT_ENTRY_ARM7);
        self.jit_live_ranges.wram.fill(0);
    }

    pub fn invalidate_vram(&mut self) {
        self.jit_entries.vram_arm7.fill(DEFAULT_JIT_ENTRY_ARM7);
        self.jit_live_ranges.vram_arm7.fill(0);
    }

    #[cfg(target_os = "linux")]
    pub fn open(&mut self) {}

    #[cfg(target_os = "linux")]
    pub fn close(&mut self) {}

    #[cfg(target_os = "linux")]
    fn flush_cache(&mut self, start_addr: usize, size: usize) {
        unsafe {
            built_in_clear_cache((self.mem.as_ptr() as usize + start_addr) as _, (self.mem.as_ptr() as usize + start_addr + size) as _);
        }
    }

    #[cfg(target_os = "vita")]
    pub fn open(&mut self) {
        unsafe { vitasdk_sys::sceKernelOpenVMDomain() };
    }

    #[cfg(target_os = "vita")]
    pub fn close(&mut self) {
        unsafe { vitasdk_sys::sceKernelCloseVMDomain() };
    }

    #[cfg(target_os = "vita")]
    fn flush_cache(&mut self, start_addr: usize, size: usize) {
        unsafe { vitasdk_sys::sceKernelSyncVMDomain(self.mem.block_uid, (self.mem.as_ptr() as usize + start_addr) as _, size as u32) };
    }
}
