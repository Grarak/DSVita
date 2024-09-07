use crate::core::memory::{regions, vram};
use crate::core::CpuType;
use crate::logging::debug_println;
use crate::mmap::Mmap;
use crate::simple_tree_map::SimpleTreeMap;
use crate::utils::HeapMem;
use crate::{utils, DEBUG_LOG};
use paste::paste;
use std::cmp::Ordering;
use std::intrinsics::{likely, unlikely};
use std::ops::{Deref, DerefMut};
use std::ptr;
use CpuType::{ARM7, ARM9};

const JIT_MEMORY_SIZE: u32 = 16 * 1024 * 1024;

#[derive(Clone, Default)]
struct JitCycle {
    pre_cycle_sum: u16,
    inst_cycle_count: u8,
}

#[derive(Default)]
struct JitBlock {
    guest_pc: u32,
    jit_addr: u32,
    cycles: Vec<JitCycle>,
}

impl Eq for JitBlock {}

impl PartialEq<Self> for JitBlock {
    fn eq(&self, other: &Self) -> bool {
        self.guest_pc == other.guest_pc
    }
}

impl PartialOrd<Self> for JitBlock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.guest_pc.partial_cmp(&other.guest_pc)
    }
}

impl Ord for JitBlock {
    fn cmp(&self, other: &Self) -> Ordering {
        self.guest_pc.cmp(&other.guest_pc)
    }
}

#[derive(Copy, Clone)]
struct JitLookup {
    jit_block: *const JitBlock,
    is_entry: bool,
}

impl JitLookup {
    fn new(jit_block: *const JitBlock, is_entry: bool) -> Self {
        JitLookup { jit_block, is_entry }
    }
}

impl Default for JitLookup {
    fn default() -> Self {
        JitLookup {
            jit_block: ptr::null(),
            is_entry: false,
        }
    }
}

pub struct JitInsertArgs {
    guest_pc: u32,
    guest_insts_cycle_counts: Vec<u16>,
}

impl JitInsertArgs {
    pub fn new(guest_pc: u32, guest_insts_cycle_counts: Vec<u16>) -> Self {
        JitInsertArgs { guest_pc, guest_insts_cycle_counts }
    }
}

macro_rules! create_jit_blocks {
    ($([$block_name:ident, $size:expr]),+) => {
        paste! {
            struct JitLookups {
                $(
                    $block_name: HeapMem<JitLookup, { $size as usize / 4 }>,
                    [<$block_name _ thumb>]: HeapMem<JitLookup, { $size as usize / 2 }>,
                )*
            }

            impl JitLookups {
                fn new() -> Self {
                    JitLookups {
                        $(
                            $block_name: HeapMem::new(),
                            [<$block_name _ thumb>]: HeapMem::new(),
                        )*
                    }
                }

                fn reset(&mut self) {
                    $(
                        self.$block_name.fill(JitLookup::default());
                        self.[<$block_name _ thumb>].fill(JitLookup::default());
                    )*
                }
            }
        }
    };
}

create_jit_blocks!(
    [itcm, regions::INSTRUCTION_TCM_SIZE],
    [main_arm9, regions::MAIN_MEMORY_SIZE],
    [arm9_bios, regions::ARM9_BIOS_SIZE],
    [main_arm7, regions::MAIN_MEMORY_SIZE],
    [wram, regions::SHARED_WRAM_SIZE + regions::ARM7_WRAM_SIZE],
    [vram_arm7, vram::ARM7_SIZE],
    [arm7_bios, regions::ARM7_BIOS_SIZE]
);

#[cfg(target_os = "linux")]
extern "C" {
    fn built_in_clear_cache(start: *const u8, end: *const u8);
}

pub struct JitMemory {
    mem: Mmap,
    mem_common_offset: u32,
    mem_offset: u32,
    jit_blocks: SimpleTreeMap<u32, Box<JitBlock>>,
    jit_blocks_thumb: SimpleTreeMap<u32, Box<JitBlock>>,
    jit_lookups: JitLookups,
    page_size: u32,
    // temporary invalidated jit block, only some when a running jit block becomes invalidated
    // we still need to access cycle data after imm breakout
    invalidated_block: Box<JitBlock>,
}

impl JitMemory {
    pub fn new() -> Self {
        JitMemory {
            mem: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            mem_common_offset: 0,
            mem_offset: 0,
            jit_blocks: SimpleTreeMap::new(),
            jit_blocks_thumb: SimpleTreeMap::new(),
            jit_lookups: JitLookups::new(),
            page_size: {
                #[cfg(target_os = "linux")]
                {
                    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as _ }
                }
                #[cfg(target_os = "vita")]
                {
                    16
                }
            },
            invalidated_block: Box::new(JitBlock::default()),
        }
    }

    fn allocate_block(&mut self, required_size: u32) -> u32 {
        if self.mem_offset + required_size >= JIT_MEMORY_SIZE {
            self.mem_offset = self.mem_common_offset;
            self.jit_blocks.clear();
            self.jit_blocks_thumb.clear();
            self.jit_lookups.reset();
            let addr = self.mem_offset;
            self.mem_offset += required_size;
            addr
        } else {
            let addr = self.mem_offset;
            self.mem_offset += required_size;
            addr
        }
    }

    fn insert(&mut self, opcodes: &[u32]) -> (u32, u32) {
        let aligned_size = utils::align_up((opcodes.len() << 2) as u32, self.page_size);
        let allocated_offset_addr = self.allocate_block(aligned_size);

        utils::write_to_mem_slice(&mut self.mem, allocated_offset_addr, opcodes);
        self.flush_cache(allocated_offset_addr, aligned_size);

        (allocated_offset_addr, aligned_size)
    }

    pub fn insert_common(&mut self, opcodes: &[u32]) -> u32 {
        let (allocated_offset_addr, aligned_size) = self.insert(opcodes);

        self.mem_common_offset = allocated_offset_addr + aligned_size;
        debug_println!("Insert new jit ({:x}) block with size {} at {:x}", self.mem.as_ptr() as u32, aligned_size, allocated_offset_addr,);

        allocated_offset_addr + self.mem.as_ptr() as u32
    }

    pub fn insert_block<const CPU: CpuType, const THUMB: bool>(&mut self, opcodes: &[u32], insert_args: JitInsertArgs) {
        let (allocated_offset_addr, aligned_size) = self.insert(opcodes);

        macro_rules! insert_to_block {
            ($lookup:expr) => {{
                let mut jit_block = JitBlock::default();
                jit_block.guest_pc = insert_args.guest_pc;
                jit_block.jit_addr = allocated_offset_addr + self.mem.as_ptr() as u32;
                jit_block.cycles.resize(insert_args.guest_insts_cycle_counts.len(), JitCycle::default());
                let inst_size = jit_block.cycles.len();
                jit_block.cycles[0].pre_cycle_sum = 0;
                jit_block.cycles[0].inst_cycle_count = insert_args.guest_insts_cycle_counts[0] as u8;
                for i in 1..insert_args.guest_insts_cycle_counts.len() {
                    let cycles = &mut jit_block.cycles[i];
                    cycles.inst_cycle_count = (insert_args.guest_insts_cycle_counts[i] - insert_args.guest_insts_cycle_counts[i - 1]) as u8;
                    cycles.pre_cycle_sum = insert_args.guest_insts_cycle_counts[i] - cycles.inst_cycle_count as u16;
                }

                let jit_blocks = if THUMB { &mut self.jit_blocks_thumb } else { &mut self.jit_blocks };
                let index = jit_blocks.insert(jit_block.guest_pc, Box::new(jit_block));
                let jit_block_ptr = jit_blocks[index].1.deref() as *const _;

                let lookup_entry = (insert_args.guest_pc >> if THUMB { 1 } else { 2 }) as usize;
                let lookup_entry = lookup_entry % $lookup.len();
                $lookup[lookup_entry] = JitLookup::new(jit_block_ptr, true);
                $lookup[lookup_entry + 1..lookup_entry + inst_size].fill(JitLookup::new(jit_block_ptr, false));
            }};
        }
        macro_rules! insert {
            ($block_name:ident) => {
                paste! {
                    if THUMB {
                        insert_to_block!(self.jit_lookups.[<$block_name _ thumb>])
                    } else {
                        insert_to_block!(self.jit_lookups.$block_name)
                    }
                }
            };
        }

        match CPU {
            ARM9 => match insert_args.guest_pc & 0xFF000000 {
                regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => {
                    insert!(itcm)
                }
                regions::MAIN_MEMORY_OFFSET => insert!(main_arm9),
                0xFF000000 => insert!(arm9_bios),
                _ => todo!("{:x}", insert_args.guest_pc),
            },
            ARM7 => match insert_args.guest_pc & 0xFF000000 {
                regions::ARM7_BIOS_OFFSET => insert!(arm7_bios),
                regions::MAIN_MEMORY_OFFSET => insert!(main_arm7),
                regions::SHARED_WRAM_OFFSET => insert!(wram),
                regions::VRAM_OFFSET => insert!(vram_arm7),
                _ => todo!("{:x}", insert_args.guest_pc),
            },
        }

        if DEBUG_LOG {
            let per = (self.mem_offset * 100) as f32 / JIT_MEMORY_SIZE as f32;
            debug_println!(
                "Insert new jit ({:x}) block with size {} at {:x}, {}% allocated with guest pc {:x}",
                self.mem.as_ptr() as u32,
                aligned_size,
                allocated_offset_addr,
                per,
                insert_args.guest_pc
            );
        }
    }

    #[inline(always)]
    pub fn get_jit_start_addr<const CPU: CpuType, const THUMB: bool>(&self, guest_pc: u32) -> Option<(u32, u32)> {
        macro_rules! get_addr_block {
            ($lookup:expr) => {{
                let lookup_entry = (guest_pc >> if THUMB { 1 } else { 2 }) as usize;
                let lookup_entry = lookup_entry % $lookup.len();
                let jit_lookup = &$lookup[lookup_entry];
                if likely(jit_lookup.is_entry && !jit_lookup.jit_block.is_null()) {
                    Some((unsafe { (*jit_lookup.jit_block).jit_addr }, jit_lookup.jit_block as u32))
                } else {
                    None
                }
            }};
        }
        macro_rules! get_addr {
            ($block_name:ident) => {
                paste! {
                    if THUMB {
                        get_addr_block!(self.jit_lookups.[<$block_name _ thumb>])
                    } else {
                        get_addr_block!(self.jit_lookups.$block_name)
                    }
                }
            };
        }
        match CPU {
            ARM9 => match guest_pc & 0xFF000000 {
                regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => {
                    get_addr!(itcm)
                }
                regions::MAIN_MEMORY_OFFSET => {
                    get_addr!(main_arm9)
                }
                0xFF000000 => get_addr!(arm9_bios),
                _ => todo!("{:x} {:x}", guest_pc, guest_pc & 0xFF000000),
            },
            ARM7 => match guest_pc & 0xFF000000 {
                regions::ARM7_BIOS_OFFSET => get_addr!(arm7_bios),
                regions::MAIN_MEMORY_OFFSET => get_addr!(main_arm7),
                regions::SHARED_WRAM_OFFSET => get_addr!(wram),
                regions::VRAM_OFFSET => get_addr!(vram_arm7),
                _ => todo!("{:x} {:x}", guest_pc, guest_pc & 0xFF000000),
            },
        }
    }

    pub fn get_cycle_counts_unchecked<const THUMB: bool>(&self, guest_pc: u32, jit_block_addr: u32) -> (u16, u8) {
        let block = unsafe { (jit_block_addr as *const JitBlock).as_ref().unwrap_unchecked() };
        let cycles_offset = ((guest_pc - block.guest_pc) >> if THUMB { 1 } else { 2 }) as usize;
        let cycles = &block.cycles[cycles_offset];
        (cycles.pre_cycle_sum, cycles.inst_cycle_count)
    }

    pub fn invalidate_block<const CPU: CpuType>(&mut self, guest_addr: u32, size: usize, current_jit_block: u32) -> bool {
        let mut should_breakout = false;

        let mut invalidate = |jit_lookup: &mut [JitLookup], jit_blocks: &mut SimpleTreeMap<u32, Box<JitBlock>>, addr_shift: u8| {
            let mut invalidated_size = 0;
            while invalidated_size <= size {
                let lookup_entry = (guest_addr >> addr_shift) as usize + invalidated_size;
                let lookup_entry = lookup_entry % jit_lookup.len();
                let jit_block_ptr = jit_lookup[lookup_entry].jit_block;
                if unlikely(!jit_block_ptr.is_null()) {
                    let jit_block = unsafe { jit_block_ptr.as_ref().unwrap_unchecked() };

                    let start_lookup_entry = (jit_block.guest_pc >> addr_shift) as usize;
                    let start_lookup_entry = start_lookup_entry % jit_lookup.len();
                    jit_lookup[start_lookup_entry..start_lookup_entry + jit_block.cycles.len()].fill(JitLookup::default());

                    invalidated_size += jit_block.cycles.len() << addr_shift;

                    let removed_block = jit_blocks.remove(&jit_block.guest_pc).unwrap();
                    if unlikely(jit_block_ptr as u32 == current_jit_block) {
                        should_breakout = true;
                        self.invalidated_block = removed_block.1;
                    }
                } else {
                    invalidated_size += 1;
                }
            }
        };

        match CPU {
            ARM9 => match guest_addr & 0xFF000000 {
                regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => {
                    invalidate(self.jit_lookups.itcm.deref_mut(), &mut self.jit_blocks, 2);
                    invalidate(self.jit_lookups.itcm_thumb.deref_mut(), &mut self.jit_blocks_thumb, 1);
                }
                regions::MAIN_MEMORY_OFFSET => {
                    invalidate(self.jit_lookups.main_arm7.deref_mut(), &mut self.jit_blocks, 2);
                    invalidate(self.jit_lookups.main_arm7_thumb.deref_mut(), &mut self.jit_blocks_thumb, 1);
                    invalidate(self.jit_lookups.main_arm9.deref_mut(), &mut self.jit_blocks, 2);
                    invalidate(self.jit_lookups.main_arm9_thumb.deref_mut(), &mut self.jit_blocks_thumb, 1);
                }
                _ => {}
            },
            ARM7 => match guest_addr & 0xFF000000 {
                regions::MAIN_MEMORY_OFFSET => {
                    invalidate(self.jit_lookups.main_arm7.deref_mut(), &mut self.jit_blocks, 2);
                    invalidate(self.jit_lookups.main_arm7_thumb.deref_mut(), &mut self.jit_blocks_thumb, 1);
                    invalidate(self.jit_lookups.main_arm9.deref_mut(), &mut self.jit_blocks, 2);
                    invalidate(self.jit_lookups.main_arm9_thumb.deref_mut(), &mut self.jit_blocks_thumb, 1);
                }
                regions::SHARED_WRAM_OFFSET => {
                    invalidate(self.jit_lookups.wram.deref_mut(), &mut self.jit_blocks, 2);
                    invalidate(self.jit_lookups.wram_thumb.deref_mut(), &mut self.jit_blocks_thumb, 1);
                }
                regions::VRAM_OFFSET => {
                    invalidate(self.jit_lookups.vram_arm7.deref_mut(), &mut self.jit_blocks, 2);
                    invalidate(self.jit_lookups.vram_arm7_thumb.deref_mut(), &mut self.jit_blocks_thumb, 1);
                }
                _ => {}
            },
        }

        should_breakout
    }

    pub fn invalidate_wram(&mut self) {
        loop {
            match self.jit_blocks.get_next(&regions::SHARED_WRAM_OFFSET) {
                None => break,
                Some((index, (_, jit_block))) => {
                    if jit_block.guest_pc & 0xFF000000 != regions::SHARED_WRAM_OFFSET {
                        break;
                    }
                    let lookup_entry = (jit_block.guest_pc >> 2) as usize;
                    let lookup_entry = lookup_entry % self.jit_lookups.wram.len();
                    self.jit_lookups.wram[lookup_entry..lookup_entry + jit_block.cycles.len()].fill(JitLookup::default());
                    self.jit_blocks.remove_at(index);
                }
            }
        }
        loop {
            match self.jit_blocks_thumb.get_next(&regions::SHARED_WRAM_OFFSET) {
                None => break,
                Some((index, (_, jit_block))) => {
                    if jit_block.guest_pc & 0xFF000000 != regions::SHARED_WRAM_OFFSET {
                        break;
                    }
                    let lookup_entry = (jit_block.guest_pc >> 1) as usize;
                    let lookup_entry = lookup_entry % self.jit_lookups.wram_thumb.len();
                    self.jit_lookups.wram_thumb[lookup_entry..lookup_entry + jit_block.cycles.len()].fill(JitLookup::default());
                    self.jit_blocks_thumb.remove_at(index);
                }
            }
        }
    }

    pub fn invalidate_vram(&mut self) {
        loop {
            match self.jit_blocks.get_next(&regions::VRAM_OFFSET) {
                None => break,
                Some((index, (_, jit_block))) => {
                    if jit_block.guest_pc & 0xFF000000 != regions::VRAM_OFFSET {
                        break;
                    }
                    let lookup_entry = (jit_block.guest_pc >> 2) as usize;
                    let lookup_entry = lookup_entry % self.jit_lookups.vram_arm7.len();
                    self.jit_lookups.vram_arm7[lookup_entry..lookup_entry + jit_block.cycles.len()].fill(JitLookup::default());
                    self.jit_blocks.remove_at(index);
                }
            }
        }
        loop {
            match self.jit_blocks_thumb.get_next(&regions::VRAM_OFFSET) {
                None => break,
                Some((index, (_, jit_block))) => {
                    if jit_block.guest_pc & 0xFF000000 != regions::VRAM_OFFSET {
                        break;
                    }
                    let lookup_entry = (jit_block.guest_pc >> 1) as usize;
                    let lookup_entry = lookup_entry % self.jit_lookups.vram_arm7_thumb.len();
                    self.jit_lookups.vram_arm7_thumb[lookup_entry..lookup_entry + jit_block.cycles.len()].fill(JitLookup::default());
                    self.jit_blocks_thumb.remove_at(index);
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    pub fn open(&mut self) {}

    #[cfg(target_os = "linux")]
    pub fn close(&mut self) {}

    #[cfg(target_os = "linux")]
    fn flush_cache(&mut self, start_addr: u32, size: u32) {
        unsafe {
            built_in_clear_cache((self.mem.as_ptr() as u32 + start_addr) as _, (self.mem.as_ptr() as u32 + start_addr + size) as _);
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
    fn flush_cache(&mut self, start_addr: u32, size: u32) {
        unsafe { vitasdk_sys::sceKernelSyncVMDomain(self.mem.block_uid, (self.mem.as_ptr() as u32 + start_addr) as _, size) };
    }
}
