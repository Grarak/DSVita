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
use std::ptr;
use CpuType::{ARM7, ARM9};

const JIT_MEMORY_SIZE: usize = 16 * 1024 * 1024;
const JIT_LIVE_RANGE_PAGE_SIZE_SHIFT: u32 = 8;
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
pub struct JitEntry(*const extern "C" fn());

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
struct JitLiveRanges {
    itcm: HeapMemU32<{ (regions::INSTRUCTION_TCM_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 32) as usize }>,
    main: HeapMemU32<{ (regions::MAIN_MEMORY_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 32) as usize }>,
    wram: HeapMemU32<{ ((regions::SHARED_WRAM_SIZE + regions::ARM7_WRAM_SIZE) / JIT_LIVE_RANGE_PAGE_SIZE / 32) as usize }>,
    vram_arm7: HeapMemU32<{ (vram::ARM7_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 32) as usize }>,
    arm9_bios: HeapMemU32<{ (regions::ARM9_BIOS_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 32) as usize }>,
    arm7_bios: HeapMemU32<{ (regions::ARM7_BIOS_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 32) as usize }>,
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
    jit_memory_map: JitMemoryMap,
}

impl JitMemory {
    pub fn new() -> Self {
        let jit_entries = JitEntries::new();
        let jit_memory_map = JitMemoryMap::new(&jit_entries);
        JitMemory {
            mem: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            mem_offset: 0,
            jit_entries,
            jit_live_ranges: JitLiveRanges::default(),
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
        let aligned_size = utils::align_up(opcodes.len() * size_of::<u32>(), *PAGE_SIZE);
        let allocated_offset_addr = self.allocate_block(aligned_size);

        utils::write_to_mem_slice(&mut self.mem, allocated_offset_addr, opcodes);
        self.flush_cache(allocated_offset_addr, aligned_size);

        (allocated_offset_addr, aligned_size)
    }

    pub fn insert_block<const CPU: CpuType>(&mut self, opcodes: &[u32], guest_pc: u32) -> *const extern "C" fn() {
        let (allocated_offset_addr, aligned_size) = self.insert(opcodes);

        macro_rules! insert {
            ($entries:expr, $live_ranges:expr) => {{
                let jit_entry_addr = (allocated_offset_addr + self.mem.as_ptr() as usize) as *const extern "C" fn();

                let entries_index = (guest_pc >> 1) as usize;
                let entries_index = entries_index % $entries.len();
                $entries[entries_index] = JitEntry(jit_entry_addr);

                // >> 5 for u32 (each bit represents a page)
                let live_ranges_index = ((guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) >> 5) as usize;
                let live_ranges_index = live_ranges_index % $live_ranges.len();
                let live_ranges_bit = (guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 31;
                $live_ranges[live_ranges_index] |= 1 << live_ranges_bit;

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

    pub fn get_jit_start_addr<const CPU: CpuType>(&self, guest_pc: u32) -> *const extern "C" fn() {
        unsafe { (*self.jit_memory_map.get_jit_entry::<CPU>(guest_pc)).0 }
    }

    pub fn invalidate_block<const REGION: JitRegion>(&mut self, guest_addr: u32, size: usize, guest_pc: u32) -> bool {
        let mut should_breakout = false;

        macro_rules! invalidate {
            ($guest_addr:expr, $live_range:ident, [$(($entries:ident, $default_entry:expr)),+]) => {{
                let live_ranges_index = (($guest_addr >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) >> 5) as usize;
                let live_ranges_index = live_ranges_index % self.jit_live_ranges.$live_range.len();
                let live_ranges_bit = ($guest_addr >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 31;

                if unlikely(self.jit_live_ranges.$live_range[live_ranges_index] & (1 << live_ranges_bit) != 0) {
                    self.jit_live_ranges.$live_range[live_ranges_index] &= !(1 << live_ranges_bit);

                    let guest_pc_index = ((guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) >> 5) as usize;
                    let guest_pc_index = guest_pc_index % self.jit_live_ranges.$live_range.len();
                    let guest_pc_bit = (guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 31;

                    should_breakout |= live_ranges_index == guest_pc_index && live_ranges_bit == guest_pc_bit;

                    let guest_addr_start = $guest_addr & !(JIT_LIVE_RANGE_PAGE_SIZE - 1);
                    let guest_addr_end = guest_addr_start + JIT_LIVE_RANGE_PAGE_SIZE;

                    $(
                        {
                            let entries_index_start = (guest_addr_start >> 1) as usize;
                            let entries_index_start = entries_index_start % self.jit_entries.$entries.len();
                            let entries_index_end = (guest_addr_end >> 1) as usize;
                            let entries_index_end = entries_index_end % self.jit_entries.$entries.len();
                            self.jit_entries.$entries[entries_index_start..entries_index_end].fill($default_entry);
                        }
                    )*
                }
            }};
        }

        match REGION {
            JitRegion::Itcm => {
                invalidate!(guest_addr, itcm, [(itcm, DEFAULT_JIT_ENTRY_ARM9)]);
                invalidate!(guest_addr + size as u32 - 1, itcm, [(itcm, DEFAULT_JIT_ENTRY_ARM9)]);
            }
            JitRegion::Main => {
                invalidate!(guest_addr, main, [(main_arm9, DEFAULT_JIT_ENTRY_ARM9), (main_arm7, DEFAULT_JIT_ENTRY_ARM7)]);
                invalidate!(guest_addr + size as u32 - 1, main, [(main_arm9, DEFAULT_JIT_ENTRY_ARM9), (main_arm7, DEFAULT_JIT_ENTRY_ARM7)]);
            }
            JitRegion::Wram => {
                invalidate!(guest_addr, wram, [(wram, DEFAULT_JIT_ENTRY_ARM7)]);
                invalidate!(guest_addr + size as u32 - 1, wram, [(wram, DEFAULT_JIT_ENTRY_ARM7)]);
            }
            JitRegion::VramArm7 => {
                invalidate!(guest_addr, vram_arm7, [(vram_arm7, DEFAULT_JIT_ENTRY_ARM7)]);
                invalidate!(guest_addr + size as u32 - 1, vram_arm7, [(vram_arm7, DEFAULT_JIT_ENTRY_ARM7)]);
            }
        }

        should_breakout
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
