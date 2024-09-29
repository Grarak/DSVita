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
use std::alloc::{GlobalAlloc, Layout, System};
use std::intrinsics::unlikely;
use std::marker::ConstParamTy;
use std::{mem, ptr};
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

pub struct JitBlock {
    guest_pc: u32,
    jit_addr: *const extern "C" fn(),
    cycles: Vec<JitCycle>,
}

impl JitBlock {
    fn alloc() -> *mut Self {
        unsafe {
            let jit_block = System.alloc(Layout::new::<JitBlock>()) as *mut JitBlock;
            jit_block.write(JitBlock {
                guest_pc: 0,
                jit_addr: ptr::null(),
                cycles: Vec::new(),
            });
            jit_block
        }
    }

    fn dealloc(jit_block: *mut Self) {
        unsafe {
            ptr::drop_in_place(jit_block);
            System.dealloc(jit_block as _, Layout::new::<JitBlock>())
        }
    }
}

unsafe impl Sync for JitBlock {}

#[derive(Copy, Clone)]
pub struct JitBlockPtr(*const JitBlock);

impl Default for JitBlockPtr {
    fn default() -> Self {
        JitBlockPtr(ptr::null())
    }
}

unsafe impl Sync for JitBlockPtr {}

#[cfg(target_os = "linux")]
lazy_static! {
    static ref PAGE_SIZE: usize = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as _ };
}
#[cfg(target_os = "vita")]
lazy_static! {
    static ref PAGE_SIZE: usize = 16;
}

static DEFAULT_JIT_BLOCK_ARM9: JitBlock = JitBlock {
    guest_pc: 0,
    jit_addr: emit_code_block::<{ ARM9 }, false> as _,
    cycles: Vec::new(),
};

static DEFAULT_JIT_BLOCK_ARM9_THUMB: JitBlock = JitBlock {
    guest_pc: 0,
    jit_addr: emit_code_block::<{ ARM9 }, true> as _,
    cycles: Vec::new(),
};

static DEFAULT_JIT_BLOCK_ARM7: JitBlock = JitBlock {
    guest_pc: 0,
    jit_addr: emit_code_block::<{ ARM7 }, false> as _,
    cycles: Vec::new(),
};

static DEFAULT_JIT_BLOCK_ARM7_THUMB: JitBlock = JitBlock {
    guest_pc: 0,
    jit_addr: emit_code_block::<{ ARM7 }, true> as _,
    cycles: Vec::new(),
};

pub struct JitInsertArgs {
    pc: u32,
    cycle_counts: Vec<u16>,
}

impl JitInsertArgs {
    pub fn new(pc: u32, cycle_counts: Vec<u16>) -> Self {
        JitInsertArgs { pc, cycle_counts }
    }
}

macro_rules! create_jit_blocks {
    ($([$block_name:ident, $size:expr, $default_block:expr, $default_block_thumb:expr]),+) => {
        paste! {
            pub struct JitLookups {
                $(
                    pub $block_name: HeapMem<JitBlockPtr, { $size as usize / 4 }>,
                    pub [<$block_name _ thumb>]: HeapMem<JitBlockPtr, { $size as usize / 2 }>,
                )*
            }

            impl JitLookups {
                fn new() -> Self {
                    let mut instance = JitLookups {
                        $(
                            $block_name: HeapMem::new(),
                            [<$block_name _ thumb>]: HeapMem::new(),
                        )*
                    };
                    instance.reset();
                    instance
                }

                fn reset(&mut self) {
                    $(
                        self.$block_name.fill(JitBlockPtr(ptr::addr_of!($default_block)));
                        self.[<$block_name _ thumb>].fill(JitBlockPtr(ptr::addr_of!($default_block_thumb)));
                    )*
                }
            }
        }
    };
}

create_jit_blocks!(
    [itcm, regions::INSTRUCTION_TCM_SIZE, DEFAULT_JIT_BLOCK_ARM9, DEFAULT_JIT_BLOCK_ARM9_THUMB],
    [main_arm9, regions::MAIN_MEMORY_SIZE, DEFAULT_JIT_BLOCK_ARM9, DEFAULT_JIT_BLOCK_ARM9_THUMB],
    [arm9_bios, regions::ARM9_BIOS_SIZE, DEFAULT_JIT_BLOCK_ARM9, DEFAULT_JIT_BLOCK_ARM9_THUMB],
    [main_arm7, regions::MAIN_MEMORY_SIZE, DEFAULT_JIT_BLOCK_ARM7, DEFAULT_JIT_BLOCK_ARM7_THUMB],
    [wram, regions::SHARED_WRAM_SIZE + regions::ARM7_WRAM_SIZE, DEFAULT_JIT_BLOCK_ARM7, DEFAULT_JIT_BLOCK_ARM7_THUMB],
    [vram_arm7, vram::ARM7_SIZE, DEFAULT_JIT_BLOCK_ARM7, DEFAULT_JIT_BLOCK_ARM7_THUMB],
    [arm7_bios, regions::ARM7_BIOS_SIZE, DEFAULT_JIT_BLOCK_ARM7, DEFAULT_JIT_BLOCK_ARM7_THUMB]
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
    jit_lookups: JitLookups,
    jit_blocks_used: Vec<*mut JitBlock>,
    jit_blocks_free: Vec<*mut JitBlock>,
    jit_live_ranges: JitLiveRanges,
    jit_memory_map: JitMemoryMap,
}

impl JitMemory {
    pub fn new() -> Self {
        let jit_lookups = JitLookups::new();
        let jit_memory_map = JitMemoryMap::new(&jit_lookups);
        JitMemory {
            mem: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            mem_offset: 0,
            jit_lookups,
            jit_blocks_used: Vec::new(),
            jit_blocks_free: Vec::new(),
            jit_live_ranges: JitLiveRanges::default(),
            jit_memory_map,
        }
    }

    fn allocate_block(&mut self, required_size: usize) -> usize {
        if self.mem_offset + required_size >= JIT_MEMORY_SIZE {
            debug_println!("Jit memory full, reset");

            self.mem_offset = 0;

            self.jit_lookups.reset();
            self.jit_blocks_free.extend(&self.jit_blocks_used);
            self.jit_blocks_used.clear();
            self.jit_live_ranges = JitLiveRanges::default();

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

    fn pop_jit_block(&mut self) -> *mut JitBlock {
        let jit_block = self.jit_blocks_free.pop().unwrap_or_else(|| JitBlock::alloc());
        self.jit_blocks_used.push(jit_block);
        jit_block
    }

    pub fn insert_block<const CPU: CpuType, const THUMB: bool>(&mut self, opcodes: &[u32], insert_args: JitInsertArgs) -> *const extern "C" fn() {
        let (allocated_offset_addr, aligned_size) = self.insert(opcodes);

        macro_rules! insert_to_block {
            ($lookup:expr, $live_ranges:expr) => {{
                let inst_size = insert_args.cycle_counts.len();

                let jit_block = unsafe { self.pop_jit_block().as_mut().unwrap() };
                jit_block.guest_pc = insert_args.pc;
                jit_block.jit_addr = (allocated_offset_addr + self.mem.as_ptr() as usize) as _;
                jit_block.cycles.resize(inst_size, JitCycle::default());

                jit_block.cycles[0].pre_cycle_sum = 0;
                jit_block.cycles[0].inst_cycle_count = insert_args.cycle_counts[0] as u8;

                for i in 1..inst_size {
                    let cycles = &mut jit_block.cycles[i];
                    cycles.inst_cycle_count = (insert_args.cycle_counts[i] - insert_args.cycle_counts[i - 1]) as u8;
                    cycles.pre_cycle_sum = insert_args.cycle_counts[i] - cycles.inst_cycle_count as u16;
                }

                let lookup_entry = (insert_args.pc >> if THUMB { 1 } else { 2 }) as usize;
                let lookup_entry = lookup_entry % $lookup.len();
                $lookup[lookup_entry] = JitBlockPtr(jit_block);

                // >> 5 for u32 (each bit represents a page)
                let live_ranges_entry = ((insert_args.pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) >> 5) as usize;
                let live_ranges_entry = live_ranges_entry % $live_ranges.len();
                let live_ranges_bit = (insert_args.pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 31;
                $live_ranges[live_ranges_entry] |= 1 << live_ranges_bit;

                jit_block.jit_addr
            }};
        }
        macro_rules! insert {
            ($block_name:ident, $live_ranges:ident) => {
                paste! {
                    if THUMB {
                        insert_to_block!(self.jit_lookups.[<$block_name _ thumb>], self.jit_live_ranges.$live_ranges)
                    } else {
                        insert_to_block!(self.jit_lookups.$block_name, self.jit_live_ranges.$live_ranges)
                    }
                }
            };
        }

        let jit_addr = match CPU {
            ARM9 => match insert_args.pc & 0xFF000000 {
                regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => {
                    insert!(itcm, itcm)
                }
                regions::MAIN_MEMORY_OFFSET => insert!(main_arm9, main),
                0xFF000000 => insert!(arm9_bios, arm9_bios),
                _ => todo!("{:x}", insert_args.pc),
            },
            ARM7 => match insert_args.pc & 0xFF000000 {
                regions::ARM7_BIOS_OFFSET => insert!(arm7_bios, arm7_bios),
                regions::MAIN_MEMORY_OFFSET => insert!(main_arm7, main),
                regions::SHARED_WRAM_OFFSET => insert!(wram, wram),
                regions::VRAM_OFFSET => insert!(vram_arm7, vram_arm7),
                _ => todo!("{:x}", insert_args.pc),
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
                insert_args.pc
            );
        }

        jit_addr
    }

    pub fn get_jit_start_addr<const CPU: CpuType, const THUMB: bool>(&self, guest_pc: u32) -> *const extern "C" fn() {
        unsafe { (*self.jit_memory_map.get_jit_block::<CPU, THUMB>(guest_pc)).jit_addr }
    }

    pub fn invalidate_block<const REGION: JitRegion>(&mut self, guest_addr: u32, size: usize, guest_pc: u32) -> bool {
        let mut should_breakout = false;

        macro_rules! invalidate {
            ($guest_addr:expr, $live_range:ident, [$(($lookup:ident, $lookup_default:expr)),+], [$(($lookup_thumb:ident, $lookup_thumb_default:expr)),+]) => {{
                let live_ranges_entry = (($guest_addr >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) >> 5) as usize;
                let live_ranges_entry = live_ranges_entry % self.jit_live_ranges.$live_range.len();
                let live_ranges_bit = ($guest_addr >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 31;

                if unlikely(self.jit_live_ranges.$live_range[live_ranges_entry] & (1 << live_ranges_bit) != 0) {
                    self.jit_live_ranges.$live_range[live_ranges_entry] &= !(1 << live_ranges_bit);

                    let guest_pc_entry = ((guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) >> 5) as usize;
                    let guest_pc_entry = guest_pc_entry % self.jit_live_ranges.$live_range.len();
                    let guest_pc_bit = ($guest_addr >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 31;

                    should_breakout |= live_ranges_entry == guest_pc_entry && live_ranges_bit == guest_pc_bit;

                    let guest_addr_start = $guest_addr & !(JIT_LIVE_RANGE_PAGE_SIZE - 1);
                    let guest_addr_end = guest_addr_start + JIT_LIVE_RANGE_PAGE_SIZE;

                    $(
                        {
                            let lookup_entry_start = (guest_addr_start >> 2) as usize;
                            let lookup_entry_start = lookup_entry_start % self.jit_lookups.$lookup.len();
                            let lookup_entry_end = (guest_addr_end >> 2) as usize;
                            let lookup_entry_end = lookup_entry_end % self.jit_lookups.$lookup.len();
                            self.jit_lookups.$lookup[lookup_entry_start..lookup_entry_end].fill(JitBlockPtr(ptr::addr_of!($lookup_default)));
                        }
                    )*

                    $(
                        {
                            let lookup_entry_start = (guest_addr_start >> 1) as usize;
                            let lookup_entry_start = lookup_entry_start % self.jit_lookups.$lookup_thumb.len();
                            let lookup_entry_end = (guest_addr_end >> 1) as usize;
                            let lookup_entry_end = lookup_entry_end % self.jit_lookups.$lookup_thumb.len();
                            self.jit_lookups.$lookup_thumb[lookup_entry_start..lookup_entry_end].fill(JitBlockPtr(ptr::addr_of!($lookup_thumb_default)));
                        }
                    )*
                }
            }};
        }

        match REGION {
            JitRegion::Itcm => {
                invalidate!(guest_addr, itcm, [(itcm, DEFAULT_JIT_BLOCK_ARM9)], [(itcm_thumb, DEFAULT_JIT_BLOCK_ARM9_THUMB)]);
                invalidate!(guest_addr + size as u32 - 1, itcm, [(itcm, DEFAULT_JIT_BLOCK_ARM9)], [(itcm_thumb, DEFAULT_JIT_BLOCK_ARM9_THUMB)]);
            }
            JitRegion::Main => {
                invalidate!(
                    guest_addr,
                    main,
                    [(main_arm9, DEFAULT_JIT_BLOCK_ARM9), (main_arm7, DEFAULT_JIT_BLOCK_ARM7)],
                    [(main_arm9_thumb, DEFAULT_JIT_BLOCK_ARM9_THUMB), (main_arm7_thumb, DEFAULT_JIT_BLOCK_ARM7_THUMB)]
                );
                invalidate!(
                    guest_addr + size as u32 - 1,
                    main,
                    [(main_arm9, DEFAULT_JIT_BLOCK_ARM9), (main_arm7, DEFAULT_JIT_BLOCK_ARM7)],
                    [(main_arm9_thumb, DEFAULT_JIT_BLOCK_ARM9_THUMB), (main_arm7_thumb, DEFAULT_JIT_BLOCK_ARM7_THUMB)]
                );
            }
            JitRegion::Wram => {
                invalidate!(guest_addr, wram, [(wram, DEFAULT_JIT_BLOCK_ARM7)], [(wram_thumb, DEFAULT_JIT_BLOCK_ARM7_THUMB)]);
                invalidate!(guest_addr + size as u32 - 1, wram, [(wram, DEFAULT_JIT_BLOCK_ARM7)], [(wram_thumb, DEFAULT_JIT_BLOCK_ARM7_THUMB)]);
            }
            JitRegion::VramArm7 => {
                invalidate!(guest_addr, vram_arm7, [(vram_arm7, DEFAULT_JIT_BLOCK_ARM7)], [(vram_arm7_thumb, DEFAULT_JIT_BLOCK_ARM7_THUMB)]);
                invalidate!(
                    guest_addr + size as u32 - 1,
                    vram_arm7,
                    [(vram_arm7, DEFAULT_JIT_BLOCK_ARM7)],
                    [(vram_arm7_thumb, DEFAULT_JIT_BLOCK_ARM7_THUMB)]
                );
            }
        }

        should_breakout
    }

    pub fn invalidate_wram(&mut self) {
        self.jit_lookups.wram.fill(JitBlockPtr(ptr::addr_of!(DEFAULT_JIT_BLOCK_ARM7)));
        self.jit_lookups.wram_thumb.fill(JitBlockPtr(ptr::addr_of!(DEFAULT_JIT_BLOCK_ARM7_THUMB)));
        self.jit_live_ranges.wram.fill(0);
    }

    pub fn invalidate_vram(&mut self) {
        self.jit_lookups.vram_arm7.fill(JitBlockPtr(ptr::addr_of!(DEFAULT_JIT_BLOCK_ARM7)));
        self.jit_lookups.vram_arm7_thumb.fill(JitBlockPtr(ptr::addr_of!(DEFAULT_JIT_BLOCK_ARM7_THUMB)));
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
