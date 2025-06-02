use crate::core::emu::Emu;
use crate::core::memory::mmu::MMU_PAGE_SHIFT;
use crate::core::memory::{regions, vram};
use crate::core::CpuType;
use crate::jit::assembler::block_asm::{BlockAsm, DirectBranch, GuestInstMetadata, GuestInstOffset};
use crate::jit::assembler::{arm, thumb};
use crate::jit::inst_mem_handler::{
    inst_read64_mem_handler, inst_read64_mem_handler_with_cpsr, inst_read_mem_handler, inst_read_mem_handler_multiple, inst_read_mem_handler_multiple_with_cpsr, inst_read_mem_handler_with_cpsr,
    inst_write_mem_handler, inst_write_mem_handler_gxfifo, inst_write_mem_handler_gxfifo_with_cpsr, inst_write_mem_handler_multiple, inst_write_mem_handler_multiple_gxfifo,
    inst_write_mem_handler_multiple_gxfifo_with_cpsr, inst_write_mem_handler_multiple_with_cpsr, inst_write_mem_handler_with_cpsr, InstMemMultipleParams,
};
use crate::jit::jit_asm::{emit_code_block, hle_bios_uninterrupt};
use crate::jit::jit_memory_map::JitMemoryMap;
use crate::jit::op::{MultipleTransfer, Op, SingleTransfer};
use crate::jit::reg::Reg;
use crate::jit::MemoryAmount;
use crate::logging::debug_println;
use crate::mmap::{flush_icache, set_protection, ArmContext, PAGE_SHIFT, PAGE_SIZE};
use crate::settings::{Arm7Emu, Settings};
use crate::utils;
use crate::utils::{HeapMem, HeapMemU8};
use bilge::prelude::{u4, u6};
use std::collections::VecDeque;
use std::hint::unreachable_unchecked;
use std::intrinsics::unlikely;
use std::ops::{Deref, DerefMut};
use std::{ptr, slice};
use CpuType::{ARM7, ARM9};

pub const JIT_MEMORY_SIZE: usize = 15 * 1024 * 1024;
pub const JIT_LIVE_RANGE_PAGE_SIZE_SHIFT: u32 = 8;
const JIT_LIVE_RANGE_PAGE_SIZE: u32 = 1 << JIT_LIVE_RANGE_PAGE_SIZE_SHIFT;
const JIT_ARM9_MEMORY_SIZE: usize = 12 * 1024 * 1024;
const JIT_ARM7_MEMORY_SIZE: usize = JIT_MEMORY_SIZE - JIT_ARM9_MEMORY_SIZE;

#[repr(align(4096))]
pub struct JitCache([u8; JIT_MEMORY_SIZE]);

impl Deref for JitCache {
    type Target = [u8; JIT_MEMORY_SIZE];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for JitCache {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[used]
#[link_section = ".test_arm"]
#[no_mangle]
pub static mut JIT_ARM_CACHE: JitCache = JitCache([0; JIT_MEMORY_SIZE]);

#[used]
#[link_section = ".text"]
#[no_mangle]
pub static mut JIT_THUMB_CACHE: JitCache = JitCache([0; JIT_MEMORY_SIZE]);

const SLOW_MEM_SINGLE_WRITE_LENGTH_THUMB: usize = 16;
const SLOW_MEM_SINGLE_READ_LENGTH_THUMB: usize = 12;
const SLOW_MEM_MULTIPLE_LENGTH_THUMB: usize = 20;

const SLOW_MEM_SINGLE_WRITE_LENGTH_ARM: usize = 20;
const SLOW_MEM_SINGLE_READ_LENGTH_ARM: usize = 16;
const SLOW_MEM_MULTIPLE_LENGTH_ARM: usize = 20;
pub const SLOW_SWP_MEM_SINGLE_WRITE_LENGTH_ARM: usize = 12;
pub const SLOW_SWP_MEM_SINGLE_READ_LENGTH_ARM: usize = 4;

#[derive(Copy, Clone)]
pub struct JitEntry(pub *const extern "C" fn(u32));

unsafe impl Sync for JitEntry {}

impl Default for JitEntry {
    fn default() -> Self {
        JitEntry(ptr::null())
    }
}

pub const DEFAULT_JIT_ENTRY: JitEntry = JitEntry(emit_code_block as _);

pub static BIOS_UNINTERRUPT_ENTRY_ARM9: JitEntry = JitEntry(hle_bios_uninterrupt::<{ ARM9 }> as _);
pub static BIOS_UNINTERRUPT_ENTRY_ARM7: JitEntry = JitEntry(hle_bios_uninterrupt::<{ ARM7 }> as _);

macro_rules! create_jit_blocks {
    ($([$block_name:ident, $size:expr]),+) => {
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
                    self.$block_name.fill(DEFAULT_JIT_ENTRY);
                )*
            }
        }
    };
}

create_jit_blocks!(
    [itcm, regions::ITCM_SIZE],
    [main, regions::MAIN_SIZE],
    [shared_wram_arm7, regions::SHARED_WRAM_SIZE],
    [wram_arm7, regions::ARM7_WRAM_SIZE],
    [vram, vram::ARM7_SIZE]
);

#[derive(Default)]
pub struct JitLiveRanges {
    pub itcm: HeapMemU8<{ (regions::ITCM_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 8) as usize }>,
    pub main: HeapMemU8<{ (regions::MAIN_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 8) as usize }>,
    pub shared_wram_arm7: HeapMemU8<{ (regions::SHARED_WRAM_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 8) as usize }>,
    pub wram_arm7: HeapMemU8<{ (regions::ARM7_WRAM_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 8) as usize }>,
    pub vram: HeapMemU8<{ (vram::ARM7_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 8) as usize }>, // Use arm7 vram size for arm9 as well
}

#[cfg(target_os = "linux")]
struct JitPerfMapRecord {
    perf_map_path: std::path::PathBuf,
    perf_map: std::fs::File,
}

#[cfg(target_os = "linux")]
impl JitPerfMapRecord {
    fn new() -> Self {
        let perf_map_path = std::path::PathBuf::from(format!("/tmp/perf-{}.map", std::process::id()));
        JitPerfMapRecord {
            perf_map_path: perf_map_path.clone(),
            perf_map: std::fs::File::create(perf_map_path).unwrap(),
        }
    }

    fn record(&mut self, jit_start: usize, jit_size: usize, guest_pc: u32, cpu_type: CpuType) {
        use std::io::Write;
        writeln!(self.perf_map, "{jit_start:x} {jit_size:x} {cpu_type:?}_{guest_pc:x}").unwrap();
    }

    fn reset(&mut self) {
        self.perf_map = std::fs::File::create(&self.perf_map_path).unwrap();
    }
}

#[cfg(target_os = "vita")]
struct JitPerfMapRecord;

#[cfg(target_os = "vita")]
impl JitPerfMapRecord {
    fn new() -> Self {
        JitPerfMapRecord
    }

    fn record_common(&mut self, jit_start: usize, jit_size: usize, name: impl AsRef<str>) {}

    fn record(&mut self, jit_start: usize, jit_size: usize, guest_pc: u32, cpu_type: CpuType) {}

    fn reset(&mut self) {}
}

#[derive(Copy, Clone)]
struct JitBlockMetadata {
    guest_pc: u32,
    guest_pc_end: u32,
    addr_offset_start: u16,
    addr_offset_end: u16,
}

impl JitBlockMetadata {
    fn new(guest_pc: u32, guest_pc_end: u32, addr_offset_start: u16, addr_offset_end: u16) -> Self {
        JitBlockMetadata {
            guest_pc,
            guest_pc_end,
            addr_offset_start,
            addr_offset_end,
        }
    }
}

#[derive(Clone)]
struct JitMemoryMetadata {
    size: usize,
    start: usize,
    end: usize,
    max_end: usize,
    jit_funcs: VecDeque<JitBlockMetadata>,
}

impl JitMemoryMetadata {
    fn new(size: usize, start: usize, end: usize) -> Self {
        JitMemoryMetadata {
            size,
            start,
            end,
            max_end: end,
            jit_funcs: VecDeque::new(),
        }
    }
}

pub struct JitMemory {
    arm9_arm_data: JitMemoryMetadata,
    arm9_thumb_data: JitMemoryMetadata,
    arm7_arm_data: JitMemoryMetadata,
    arm7_thumb_data: JitMemoryMetadata,
    jit_entries: JitEntries,
    jit_live_ranges: JitLiveRanges,
    pub jit_memory_map: JitMemoryMap,
    jit_perf_map_record: JitPerfMapRecord,
    pub guest_inst_arm_offsets: HeapMem<Vec<GuestInstOffset>, { JIT_MEMORY_SIZE / PAGE_SIZE }>,
    guest_inst_arm_metadata: HeapMem<Vec<GuestInstMetadata>, { JIT_MEMORY_SIZE / PAGE_SIZE }>,
    pub guest_inst_thumb_offsets: HeapMem<Vec<GuestInstOffset>, { JIT_MEMORY_SIZE / PAGE_SIZE }>,
    guest_inst_thumb_metadata: HeapMem<Vec<GuestInstMetadata>, { JIT_MEMORY_SIZE / PAGE_SIZE }>,
}

impl Emu {
    pub fn jit_insert_block(&mut self, block_asm: BlockAsm, guest_pc: u32, guest_pc_end: u32, cpu_type: CpuType) -> (*const extern "C" fn(u32), bool) {
        let thumb = block_asm.thumb;
        macro_rules! insert {
            ($entries:expr, $live_ranges:expr, $region:expr, [$($cpu_entry:expr),+]) => {{
                let ret = insert!($entries, $live_ranges);
                $(
                    let guest_pc_end = guest_pc_end - if thumb { 2 } else { 4 };
                    let begin = guest_pc >> MMU_PAGE_SHIFT;
                    let end = guest_pc_end >> MMU_PAGE_SHIFT;
                    for i in begin..=end {
                        self.mmu_remove_write::<{ $cpu_entry }>(i << MMU_PAGE_SHIFT, &$region);
                    }
                )*
                ret
            }};

            ($entries:expr, $live_ranges:expr) => {{
                let (allocated_offset_addr, aligned_size, flushed) = self.jit.insert(block_asm, cpu_type);
                debug_println!("{cpu_type:?} insert with size {aligned_size}");

                let jit_entry_addr = unsafe {
                    let jit_cache_ptr = if thumb { JIT_THUMB_CACHE.as_ptr() } else { JIT_ARM_CACHE.as_ptr() };
                    ((allocated_offset_addr + jit_cache_ptr as usize) | (thumb as usize)) as *const extern "C" fn(u32)
                };

                let guest_block_size = (guest_pc_end - guest_pc) as usize;
                debug_assert!(guest_block_size < PAGE_SIZE);
                self.jit.jit_memory_map.write_jit_entries(guest_pc, guest_block_size, JitEntry(jit_entry_addr));

                let metadata = JitBlockMetadata::new(guest_pc | (thumb as u32), guest_pc_end | (thumb as u32), (allocated_offset_addr >> PAGE_SHIFT) as u16, ((allocated_offset_addr + aligned_size) >> PAGE_SHIFT) as u16);
                self.jit.get_jit_data(thumb, cpu_type).jit_funcs.push_back(metadata);

                // >> 3 for u8 (each bit represents a page)
                let guest_pc_end = guest_pc_end - if thumb { 2 } else { 4 };
                let live_range_begin = guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT;
                let live_range_end = guest_pc_end >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT;
                for i in live_range_begin..=live_range_end {
                    let live_ranges_bit = i & 0x7;
                    unsafe { *self.jit.jit_memory_map.get_live_range(i << JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) |= 1 << live_ranges_bit };
                }

                self.jit.jit_perf_map_record.record(jit_entry_addr as usize, aligned_size, guest_pc, cpu_type);

                (jit_entry_addr, flushed)
            }};
        }

        match cpu_type {
            ARM9 => match guest_pc & 0xFF000000 {
                regions::ITCM_OFFSET | regions::ITCM_OFFSET2 => insert!(self.jit.jit_entries.itcm, self.jit.jit_live_ranges.itcm, regions::ITCM_REGION, [ARM9]),
                regions::MAIN_OFFSET => insert!(self.jit.jit_entries.main, self.jit.jit_live_ranges.main, regions::MAIN_REGION, [ARM9, ARM7]),
                regions::VRAM_OFFSET => insert!(self.jit.jit_entries.vram, self.jit.jit_live_ranges.vram),
                _ => todo!("{:x}", guest_pc),
            },
            ARM7 => match guest_pc & 0xFF000000 {
                regions::MAIN_OFFSET => insert!(self.jit.jit_entries.main, self.jit.jit_live_ranges.main, regions::MAIN_REGION, [ARM9, ARM7]),
                regions::SHARED_WRAM_OFFSET => {
                    if guest_pc & regions::ARM7_WRAM_OFFSET == regions::ARM7_WRAM_OFFSET {
                        insert!(self.jit.jit_entries.wram_arm7, self.jit.jit_live_ranges.wram_arm7, regions::ARM7_WRAM_REGION, [ARM7])
                    } else {
                        insert!(
                            self.jit.jit_entries.shared_wram_arm7,
                            self.jit.jit_live_ranges.shared_wram_arm7,
                            regions::SHARED_WRAM_ARM7_REGION,
                            [ARM7]
                        )
                    }
                }
                regions::VRAM_OFFSET => insert!(self.jit.jit_entries.vram, self.jit.jit_live_ranges.vram),
                _ => todo!("{:x}", guest_pc),
            },
        }
    }
}

impl JitMemory {
    pub fn new(settings: &Settings) -> Self {
        unsafe {
            set_protection(JIT_ARM_CACHE.as_mut_ptr(), JIT_ARM_CACHE.len(), true, true, true);
            set_protection(JIT_THUMB_CACHE.as_mut_ptr(), JIT_THUMB_CACHE.len(), true, true, true);
        }

        let arm9_data = if settings.arm7_hle() == Arm7Emu::Hle {
            JitMemoryMetadata::new(JIT_MEMORY_SIZE, 0, JIT_MEMORY_SIZE)
        } else {
            JitMemoryMetadata::new(JIT_ARM9_MEMORY_SIZE, 0, JIT_ARM9_MEMORY_SIZE)
        };

        let arm7_data = if settings.arm7_hle() == Arm7Emu::Hle {
            JitMemoryMetadata::new(0, 0, 0)
        } else {
            JitMemoryMetadata::new(JIT_ARM7_MEMORY_SIZE, JIT_ARM9_MEMORY_SIZE, JIT_MEMORY_SIZE)
        };

        let jit_entries = JitEntries::new();
        let jit_live_ranges = JitLiveRanges::default();
        let jit_memory_map = JitMemoryMap::new(&jit_entries, &jit_live_ranges);
        JitMemory {
            arm9_arm_data: arm9_data.clone(),
            arm9_thumb_data: arm9_data,
            arm7_arm_data: arm7_data.clone(),
            arm7_thumb_data: arm7_data,
            jit_entries,
            jit_live_ranges,
            jit_memory_map,
            jit_perf_map_record: JitPerfMapRecord::new(),
            guest_inst_arm_offsets: HeapMem::new(),
            guest_inst_arm_metadata: HeapMem::new(),
            guest_inst_thumb_offsets: HeapMem::new(),
            guest_inst_thumb_metadata: HeapMem::new(),
        }
    }

    fn get_jit_data(&mut self, thumb: bool, cpu_type: CpuType) -> &mut JitMemoryMetadata {
        match cpu_type {
            ARM9 => {
                if thumb {
                    &mut self.arm9_thumb_data
                } else {
                    &mut self.arm9_arm_data
                }
            }
            ARM7 => {
                if thumb {
                    &mut self.arm7_thumb_data
                } else {
                    &mut self.arm7_arm_data
                }
            }
        }
    }

    fn reset_blocks(&mut self, thumb: bool, cpu_type: CpuType) {
        self.jit_perf_map_record.reset();

        let block_metadata = self.get_jit_data(thumb, cpu_type).jit_funcs.pop_front().unwrap();
        self.jit_memory_map
            .write_jit_entries(block_metadata.guest_pc, (block_metadata.guest_pc_end - block_metadata.guest_pc) as usize, DEFAULT_JIT_ENTRY);

        let inst_metadata = if thumb { &mut self.guest_inst_thumb_metadata } else { &mut self.guest_inst_arm_metadata };
        for i in block_metadata.addr_offset_start..block_metadata.addr_offset_end {
            inst_metadata[i as usize].clear();
        }

        let jit_size = self.get_jit_data(thumb, cpu_type).size;
        let freed_start = block_metadata.addr_offset_start;
        let mut freed_end = block_metadata.addr_offset_end;
        while (freed_end - freed_start) < (jit_size / 4 / PAGE_SIZE) as u16 {
            let block_metadata = self.get_jit_data(thumb, cpu_type).jit_funcs.front().unwrap();
            if block_metadata.addr_offset_end < freed_start {
                break;
            }

            let addr_offset_start = block_metadata.addr_offset_start;
            let addr_offset_end = block_metadata.addr_offset_end;
            let guest_pc = block_metadata.guest_pc;
            let guest_block_size = (block_metadata.guest_pc_end - block_metadata.guest_pc) as usize;

            self.jit_memory_map.write_jit_entries(guest_pc, guest_block_size, DEFAULT_JIT_ENTRY);
            let inst_metadata = if thumb { &mut self.guest_inst_thumb_metadata } else { &mut self.guest_inst_arm_metadata };
            for i in addr_offset_start..addr_offset_end {
                inst_metadata[i as usize].clear();
            }

            freed_end = addr_offset_end;
            self.get_jit_data(thumb, cpu_type).jit_funcs.pop_front().unwrap();
        }

        let jit_data = self.get_jit_data(thumb, cpu_type);
        jit_data.start = (freed_start as usize) << PAGE_SHIFT;
        jit_data.end = (freed_end as usize) << PAGE_SHIFT;

        debug_println!("{cpu_type:?} Jit memory reset from {:x} - {:x}", jit_data.start, jit_data.end);
    }

    fn allocate_block(&mut self, required_size: usize, thumb: bool, cpu_type: CpuType) -> (usize, bool) {
        let mut flushed = false;
        let jit_data = self.get_jit_data(thumb, cpu_type);
        if jit_data.start + required_size > jit_data.end {
            if jit_data.start + required_size > jit_data.max_end {
                let block_metadata = jit_data.jit_funcs.back_mut().unwrap();
                block_metadata.addr_offset_end = (jit_data.max_end >> PAGE_SHIFT) as u16;
            }
            self.reset_blocks(thumb, cpu_type);
            let jit_data = self.get_jit_data(thumb, cpu_type);
            assert!(jit_data.start + required_size <= jit_data.end);
            flushed = true;
        }

        let jit_data = self.get_jit_data(thumb, cpu_type);
        let addr = jit_data.start;
        jit_data.start += required_size;
        (addr, flushed)
    }

    fn insert(&mut self, block_asm: BlockAsm, cpu_type: CpuType) -> (usize, usize, bool) {
        let opcodes = block_asm.get_code_buffer();
        let aligned_size = utils::align_up(opcodes.len(), PAGE_SIZE);
        let (allocated_offset_addr, flushed) = self.allocate_block(aligned_size, block_asm.thumb, cpu_type);

        unsafe {
            utils::write_to_mem_slice(if block_asm.thumb { JIT_THUMB_CACHE.as_mut() } else { JIT_ARM_CACHE.as_mut() }, allocated_offset_addr, &opcodes);

            let block_ptr = if block_asm.thumb { JIT_THUMB_CACHE.as_mut_ptr() } else { JIT_ARM_CACHE.as_mut_ptr() }.add(allocated_offset_addr);
            for branch in block_asm.direct_branches {
                let (has_return, offset, fun) = match branch {
                    DirectBranch::B(offset, fun) => (false, offset, fun),
                    DirectBranch::Bl(offset, fun) => (true, offset, fun),
                };
                debug_assert_eq!(fun as usize & 1, 1);

                let branch_op_ptr = block_ptr.add(offset);
                let b_offset = ((fun as usize & !1) as isize) - (branch_op_ptr as isize);
                let opcode = if block_asm.thumb {
                    debug_assert!(b_offset.abs() < 16 * 1024 * 1024, "function at {:x}", fun as usize);
                    if has_return {
                        thumb::Branch::bl(b_offset as i32 - 4)
                    } else {
                        thumb::Branch::b(b_offset as i32 - 4)
                    }
                } else {
                    debug_assert!(has_return);
                    debug_assert!(b_offset.abs() < 32 * 1024 * 1024, "function at {:x}", fun as usize);
                    arm::branch_assembler::B::blx(b_offset as i32 - 8)
                };

                branch_op_ptr.copy_from(opcode.to_le_bytes().as_ptr(), size_of::<u32>());
            }

            flush_icache(block_ptr, aligned_size);
        }

        let block_page = allocated_offset_addr >> PAGE_SHIFT;
        if block_asm.thumb {
            self.guest_inst_thumb_offsets[block_page] = block_asm.guest_inst_offsets;
        } else {
            self.guest_inst_arm_offsets[block_page] = block_asm.guest_inst_offsets;
        }
        let inst_metadata = if block_asm.thumb {
            &mut self.guest_inst_thumb_metadata
        } else {
            &mut self.guest_inst_arm_metadata
        };
        for (block_offset, metadata) in block_asm.guest_inst_metadata {
            inst_metadata[block_page + block_offset as usize].push(metadata);
        }

        (allocated_offset_addr, aligned_size, flushed)
    }

    pub fn get_jit_start_addr(&self, guest_pc: u32) -> *const extern "C" fn(u32) {
        unsafe { (*self.jit_memory_map.get_jit_entry(guest_pc)).0 }
    }

    #[inline(never)]
    pub fn invalidate_block(&mut self, guest_addr: u32, size: usize) {
        macro_rules! invalidate {
            ($guest_addr:expr) => {{
                let live_range = unsafe { self.jit_memory_map.get_live_range($guest_addr).as_mut_unchecked() };
                let live_ranges_bit = ($guest_addr >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 0x7;
                if unlikely(*live_range & (1 << live_ranges_bit) != 0) {
                    *live_range &= !(1 << live_ranges_bit);

                    let guest_addr_start = $guest_addr & !(JIT_LIVE_RANGE_PAGE_SIZE - 1);
                    debug_println!("Invalidating jit {guest_addr_start:x} - {:x}", guest_addr_start + JIT_LIVE_RANGE_PAGE_SIZE);
                    self.jit_memory_map.write_jit_entries(guest_addr_start, JIT_LIVE_RANGE_PAGE_SIZE as usize, DEFAULT_JIT_ENTRY);
                }
            }};
        }

        invalidate!(guest_addr);
        invalidate!(guest_addr + size as u32 - 1);
    }

    pub fn invalidate_wram(&mut self) {
        if self.arm7_arm_data.size != 0 || self.arm7_thumb_data.size != 0 {
            for live_range in self.jit_live_ranges.shared_wram_arm7.deref() {
                if *live_range != 0 {
                    self.jit_entries.shared_wram_arm7.fill(DEFAULT_JIT_ENTRY);
                    self.jit_live_ranges.shared_wram_arm7.fill(0);
                    return;
                }
            }
        }
    }

    pub fn invalidate_vram(&mut self) {
        for live_range in self.jit_live_ranges.vram.deref() {
            if *live_range != 0 {
                self.jit_entries.vram.fill(DEFAULT_JIT_ENTRY);
                self.jit_live_ranges.vram.fill(0);
                return;
            }
        }
    }

    fn get_inst_mem_handler_fun<const CPU: CpuType>(is_write: bool, transfer: SingleTransfer, guest_memory_addr: u32, cpsr_dirty: bool) -> *const fn() {
        macro_rules! _get_inst_mem_handler_fun {
            ($is_write:expr, $size:expr, $signed:expr, $write_func:ident, $read_func:ident, $read_func_64:ident) => {
                match ($is_write, $size) {
                    (true, 0) => $write_func::<CPU, { MemoryAmount::Byte }> as _,
                    (true, 1) => $write_func::<CPU, { MemoryAmount::Half }> as _,
                    (true, 2) => $write_func::<CPU, { MemoryAmount::Word }> as _,
                    (true, 3) => $write_func::<CPU, { MemoryAmount::Double }> as _,
                    (false, 0) => {
                        if $signed {
                            $read_func::<CPU, { MemoryAmount::Byte }, true> as _
                        } else {
                            $read_func::<CPU, { MemoryAmount::Byte }, false> as _
                        }
                    }
                    (false, 1) => {
                        if $signed {
                            $read_func::<CPU, { MemoryAmount::Half }, true> as _
                        } else {
                            $read_func::<CPU, { MemoryAmount::Half }, false> as _
                        }
                    }
                    (false, 2) => $read_func::<CPU, { MemoryAmount::Word }, false> as _,
                    (false, 3) => $read_func_64::<CPU> as _,
                    _ => unsafe { unreachable_unchecked() },
                }
            };
        }

        if CPU == ARM9 && is_write && guest_memory_addr >= 0x4000400 && guest_memory_addr < 0x4000440 && transfer.size() == 2 {
            if cpsr_dirty {
                _get_inst_mem_handler_fun!(
                    is_write,
                    transfer.size(),
                    transfer.signed(),
                    inst_write_mem_handler_gxfifo_with_cpsr,
                    inst_read_mem_handler_with_cpsr,
                    inst_read64_mem_handler_with_cpsr
                )
            } else {
                _get_inst_mem_handler_fun!(
                    is_write,
                    transfer.size(),
                    transfer.signed(),
                    inst_write_mem_handler_gxfifo,
                    inst_read_mem_handler,
                    inst_read64_mem_handler
                )
            }
        } else if cpsr_dirty {
            _get_inst_mem_handler_fun!(
                is_write,
                transfer.size(),
                transfer.signed(),
                inst_write_mem_handler_with_cpsr,
                inst_read_mem_handler_with_cpsr,
                inst_read64_mem_handler_with_cpsr
            )
        } else {
            _get_inst_mem_handler_fun!(is_write, transfer.size(), transfer.signed(), inst_write_mem_handler, inst_read_mem_handler, inst_read64_mem_handler)
        }
    }

    fn get_inst_mem_multiple_handler_fun<const CPU: CpuType>(is_write: bool, transfer: MultipleTransfer, guest_memory_addr: u32, cpsr_dirty: bool) -> *const fn() {
        macro_rules! _get_inst_mem_multiple_handler_fun {
            ($write_fun:ident, $read_func:ident) => {
                match (is_write, transfer.write_back(), !transfer.add()) {
                    (false, false, false) => $read_func::<CPU, false, false> as _,
                    (false, false, true) => $read_func::<CPU, false, true> as _,
                    (false, true, false) => $read_func::<CPU, true, false> as _,
                    (false, true, true) => $read_func::<CPU, true, true> as _,
                    (true, false, false) => $write_fun::<CPU, false, false> as _,
                    (true, false, true) => $write_fun::<CPU, false, true> as _,
                    (true, true, false) => $write_fun::<CPU, true, false> as _,
                    (true, true, true) => $write_fun::<CPU, true, true> as _,
                }
            };
        }

        if CPU == ARM9 && is_write && guest_memory_addr >= 0x4000400 && guest_memory_addr < 0x4000440 {
            if cpsr_dirty {
                _get_inst_mem_multiple_handler_fun!(inst_write_mem_handler_multiple_gxfifo_with_cpsr, inst_read_mem_handler_multiple_with_cpsr)
            } else {
                _get_inst_mem_multiple_handler_fun!(inst_write_mem_handler_multiple_gxfifo, inst_read_mem_handler_multiple)
            }
        } else if cpsr_dirty {
            _get_inst_mem_multiple_handler_fun!(inst_write_mem_handler_multiple_with_cpsr, inst_read_mem_handler_multiple_with_cpsr)
        } else {
            _get_inst_mem_multiple_handler_fun!(inst_write_mem_handler_multiple, inst_read_mem_handler_multiple)
        }
    }

    pub fn get_slow_mem_length(op: Op) -> usize {
        match op {
            Op::Str(transfer) => {
                if transfer.size() == 3 {
                    SLOW_MEM_SINGLE_WRITE_LENGTH_ARM
                } else {
                    SLOW_MEM_SINGLE_WRITE_LENGTH_ARM - 4
                }
            }
            Op::StrT(transfer) => {
                if transfer.size() == 3 {
                    SLOW_MEM_SINGLE_WRITE_LENGTH_THUMB
                } else {
                    SLOW_MEM_SINGLE_WRITE_LENGTH_THUMB - 2
                }
            }
            Op::Ldr(transfer) => {
                if transfer.size() == 3 {
                    SLOW_MEM_SINGLE_READ_LENGTH_ARM
                } else {
                    SLOW_MEM_SINGLE_READ_LENGTH_ARM - 4
                }
            }
            Op::LdrT(transfer) => {
                if transfer.size() == 3 {
                    SLOW_MEM_SINGLE_READ_LENGTH_THUMB
                } else {
                    SLOW_MEM_SINGLE_READ_LENGTH_THUMB - 2
                }
            }
            Op::Stm(_) | Op::Ldm(_) => SLOW_MEM_MULTIPLE_LENGTH_ARM,
            Op::StmT(_) | Op::LdmT(_) => SLOW_MEM_MULTIPLE_LENGTH_THUMB,
            _ => unsafe { unreachable_unchecked() },
        }
    }

    fn write_to_fast_mem<T>(fast_mem: &mut [u8], offset: &mut usize, value: T) {
        utils::write_to_mem(fast_mem, *offset as u32, value);
        *offset += size_of::<T>();
    }

    fn fast_mem_mov_reg<const THUMB: bool>(fast_mem: &mut [u8], offset: &mut usize, reg: Reg, reg2: Reg) {
        if THUMB {
            Self::write_to_fast_mem(fast_mem, offset, thumb::MovReg::mov(reg, reg2));
        } else {
            Self::write_to_fast_mem(fast_mem, offset, arm::alu_assembler::AluShiftImm::mov_al(reg, reg2));
        }
    }

    fn fast_mem_mov<const THUMB: bool>(fast_mem: &mut [u8], offset: &mut usize, reg: Reg, value: u32) {
        let (opcodes, length) = if THUMB { thumb::Mov::mov32(reg, value) } else { arm::alu_assembler::AluImm::mov32(reg, value) };
        for opcode in &opcodes[..length] {
            Self::write_to_fast_mem(fast_mem, offset, *opcode);
        }
    }

    fn fast_mem_bl<const THUMB: bool>(fast_mem: &mut [u8], offset: &mut usize, fun: *const fn()) {
        let offset_ptr = unsafe { fast_mem.as_ptr().add(*offset) };
        let b_offset = fun as isize - offset_ptr as isize;
        if THUMB {
            debug_assert!(b_offset.abs() < 16 * 1024 * 1024);
            Self::write_to_fast_mem(fast_mem, offset, thumb::Branch::bl(b_offset as i32 - 4));
        } else {
            debug_assert!(b_offset.abs() < 32 * 1024 * 1024);
            Self::write_to_fast_mem(fast_mem, offset, arm::branch_assembler::B::blx(b_offset as i32 - 8));
        }
    }

    unsafe fn execute_patch_slow_mem<const THUMB: bool>(host_pc: &mut usize, guest_memory_addr: u32, fast_mem: &mut [u8], guest_inst_metadata: &GuestInstMetadata, cpu: CpuType) {
        let mut slow_mem_length = 0;

        if guest_inst_metadata.op.is_single_mem_transfer() {
            let transfer = match guest_inst_metadata.op {
                Op::Ldr(transfer) | Op::LdrT(transfer) | Op::Str(transfer) | Op::StrT(transfer) => transfer,
                _ => unreachable_unchecked(),
            };

            let is_write = guest_inst_metadata.op.is_write_mem_transfer();

            let inst_mem_func = match cpu {
                ARM9 => Self::get_inst_mem_handler_fun::<{ ARM9 }>(is_write, transfer, guest_memory_addr, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR)),
                ARM7 => Self::get_inst_mem_handler_fun::<{ ARM7 }>(is_write, transfer, guest_memory_addr, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR)),
            };

            if is_write {
                let guest_inst_metadata_ptr = guest_inst_metadata as *const _;
                Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R12, guest_inst_metadata_ptr as u32);

                if guest_inst_metadata.op0 != Reg::R0 {
                    Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R0, guest_inst_metadata.op0);
                }

                if transfer.size() == 3 {
                    let mapped_next = guest_inst_metadata.mapped_guest_regs[guest_inst_metadata.operands.values[0].as_reg_no_shift().unwrap_unchecked() as usize + 1];
                    Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R0, mapped_next);
                }
            } else {
                Self::fast_mem_mov::<THUMB>(
                    fast_mem,
                    &mut slow_mem_length,
                    Reg::R0,
                    guest_inst_metadata.operands.values[0].as_reg_no_shift().unwrap_unchecked() as u32,
                );
            }

            Self::fast_mem_bl::<THUMB>(fast_mem, &mut slow_mem_length, inst_mem_func);

            if !is_write {
                Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, guest_inst_metadata.op0, Reg::R0);
                if transfer.size() == 3 {
                    let mapped_next = guest_inst_metadata.mapped_guest_regs[guest_inst_metadata.operands.values[0].as_reg_no_shift().unwrap_unchecked() as usize + 1];
                    Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, mapped_next, Reg::R1);
                }
            }

            let max_length = Self::get_slow_mem_length(guest_inst_metadata.op);
            debug_assert!(slow_mem_length <= max_length, "{slow_mem_length} <= {max_length}");
        } else if guest_inst_metadata.op.is_multiple_mem_transfer() {
            let transfer = match guest_inst_metadata.op {
                Op::Ldm(transfer) | Op::LdmT(transfer) | Op::Stm(transfer) | Op::StmT(transfer) => transfer,
                _ => unreachable_unchecked(),
            };

            let is_write = guest_inst_metadata.op.is_write_mem_transfer();

            let inst_mem_func = match cpu {
                ARM9 => Self::get_inst_mem_multiple_handler_fun::<{ ARM9 }>(is_write, transfer, guest_memory_addr, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR)),
                ARM7 => Self::get_inst_mem_multiple_handler_fun::<{ ARM7 }>(is_write, transfer, guest_memory_addr, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR)),
            };

            let mut pre = transfer.pre();
            if !transfer.add() {
                pre = !pre;
            }

            let op1 = guest_inst_metadata.operands.values[1].as_reg_list().unwrap_unchecked();
            let params = InstMemMultipleParams::new(op1.0 as u16, u4::new(op1.len() as u8), u4::new(guest_inst_metadata.op0 as u8), pre, transfer.user(), u6::new(0));

            Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R0, u32::from(params));

            let guest_inst_metadata_ptr = guest_inst_metadata as *const _;
            Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R1, guest_inst_metadata_ptr as u32);

            Self::fast_mem_bl::<THUMB>(fast_mem, &mut slow_mem_length, inst_mem_func);

            let max_length = Self::get_slow_mem_length(guest_inst_metadata.op);
            debug_assert!(slow_mem_length <= max_length, "{slow_mem_length} <= {max_length}");
        } else if !THUMB && matches!(guest_inst_metadata.op, Op::Swpb | Op::Swp) {
            let is_write = guest_inst_metadata.op0 == Reg::R1;

            let size = match guest_inst_metadata.op {
                Op::Swpb => 0,
                Op::Swp => 2,
                _ => unsafe { unreachable_unchecked() },
            };
            let transfer = SingleTransfer::new(false, false, false, false, size);
            let inst_mem_func = match cpu {
                ARM9 => Self::get_inst_mem_handler_fun::<{ ARM9 }>(is_write, transfer, guest_memory_addr, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR)),
                ARM7 => Self::get_inst_mem_handler_fun::<{ ARM7 }>(is_write, transfer, guest_memory_addr, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR)),
            };

            if is_write {
                let guest_inst_metadata_ptr = guest_inst_metadata as *const _;
                Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R12, guest_inst_metadata_ptr as u32);
            }

            Self::fast_mem_bl::<THUMB>(fast_mem, &mut slow_mem_length, inst_mem_func);

            let max_length = if is_write { SLOW_SWP_MEM_SINGLE_WRITE_LENGTH_ARM } else { SLOW_SWP_MEM_SINGLE_READ_LENGTH_ARM };
            debug_assert!(slow_mem_length <= max_length, "{slow_mem_length} <= {max_length}");
        } else {
            unreachable_unchecked()
        }

        debug_assert!(slow_mem_length <= fast_mem.len());
        if THUMB {
            for i in (slow_mem_length..fast_mem.len()).step_by(2) {
                fast_mem[i..i + 2].copy_from_slice(&thumb::NOP.to_le_bytes());
            }
        } else {
            for i in (slow_mem_length..fast_mem.len()).step_by(4) {
                fast_mem[i..i + 4].copy_from_slice(&arm::NOP.to_le_bytes());
            }
        }

        *host_pc = fast_mem.as_mut_ptr() as usize;
        flush_icache(fast_mem.as_ptr(), fast_mem.len());
    }

    pub unsafe fn patch_slow_mem(&mut self, host_pc: &mut usize, guest_memory_addr: u32, cpu: CpuType, _: &ArmContext) -> bool {
        if (*host_pc < JIT_ARM_CACHE.as_ptr() as usize || *host_pc >= JIT_ARM_CACHE.as_ptr() as usize + JIT_MEMORY_SIZE)
            && (*host_pc < JIT_THUMB_CACHE.as_ptr() as usize || *host_pc >= JIT_THUMB_CACHE.as_ptr() as usize + JIT_MEMORY_SIZE)
        {
            debug_println!("Segfault outside of guest context");
            return false;
        }

        let is_thumb = *host_pc >= JIT_THUMB_CACHE.as_ptr() as usize && *host_pc < JIT_THUMB_CACHE.as_ptr() as usize + JIT_MEMORY_SIZE;
        let jit_mem_offset = *host_pc - if is_thumb { JIT_THUMB_CACHE.as_ptr() } else { JIT_ARM_CACHE.as_ptr() } as usize;
        let metadata_block_page = jit_mem_offset >> PAGE_SHIFT;
        let opcode_offset = jit_mem_offset & (PAGE_SIZE - 1);
        let guest_inst_metadata_list = if is_thumb {
            &self.guest_inst_thumb_metadata[metadata_block_page]
        } else {
            &self.guest_inst_arm_metadata[metadata_block_page]
        };

        for metadata in guest_inst_metadata_list {
            if metadata.opcode_offset == opcode_offset {
                let thumb = metadata.pc & 1 == 1;
                let fast_mem_start = (*host_pc - metadata.fast_mem_start_offset as usize) as *mut u8;

                let fast_mem = slice::from_raw_parts_mut(fast_mem_start, metadata.fast_mem_size as usize);
                if thumb {
                    Self::execute_patch_slow_mem::<true>(host_pc, guest_memory_addr, fast_mem, metadata, cpu);
                } else {
                    Self::execute_patch_slow_mem::<false>(host_pc, guest_memory_addr, fast_mem, metadata, cpu);
                }

                return true;
            }
        }

        debug_println!("Can't find guest inst metadata");
        false
    }
}
