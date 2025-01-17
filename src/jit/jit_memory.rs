use crate::core::emu::{get_mmu, Emu};
use crate::core::memory::{regions, vram};
use crate::core::CpuType;
use crate::jit::assembler::arm::alu_assembler::AluShiftImm;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::jit_asm::{emit_code_block, hle_bios_uninterrupt};
use crate::jit::jit_memory_map::JitMemoryMap;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::logging::debug_println;
use crate::mmap::{flush_icache, Mmap, PAGE_SHIFT, PAGE_SIZE};
use crate::utils;
use crate::utils::{HeapMem, HeapMemU8};
use paste::paste;
use std::collections::VecDeque;
use std::intrinsics::unlikely;
use std::ops::Deref;
use std::{ptr, slice};
use CpuType::{ARM7, ARM9};

const JIT_MEMORY_SIZE: usize = 24 * 1024 * 1024;
pub const JIT_LIVE_RANGE_PAGE_SIZE_SHIFT: u32 = 10;
const JIT_LIVE_RANGE_PAGE_SIZE: u32 = 1 << JIT_LIVE_RANGE_PAGE_SIZE_SHIFT;

#[derive(Copy, Clone)]
pub struct JitEntry(pub *const extern "C" fn());

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
                        self.$block_name.fill(DEFAULT_JIT_ENTRY);
                    )*
                }
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
    common_records: Vec<(usize, usize, String)>,
    perf_map_path: std::path::PathBuf,
    perf_map: std::fs::File,
}

#[cfg(target_os = "linux")]
impl JitPerfMapRecord {
    fn new() -> Self {
        let perf_map_path = std::path::PathBuf::from(format!("/tmp/perf-{}.map", std::process::id()));
        JitPerfMapRecord {
            common_records: Vec::new(),
            perf_map_path: perf_map_path.clone(),
            perf_map: std::fs::File::create(perf_map_path).unwrap(),
        }
    }

    fn record_common(&mut self, jit_start: usize, jit_size: usize, name: impl AsRef<str>) {
        self.common_records.push((jit_start, jit_size, name.as_ref().to_string()));
        use std::io::Write;
        writeln!(self.perf_map, "{jit_start:x} {jit_size:x} {}", name.as_ref()).unwrap();
    }

    fn record(&mut self, jit_start: usize, jit_size: usize, guest_pc: u32, cpu_type: CpuType) {
        use std::io::Write;
        writeln!(self.perf_map, "{jit_start:x} {jit_size:x} {cpu_type:?}_{guest_pc:x}").unwrap();
    }

    fn reset(&mut self) {
        self.perf_map = std::fs::File::create(&self.perf_map_path).unwrap();
        for (jit_start, jit_size, name) in &self.common_records {
            use std::io::Write;
            writeln!(self.perf_map, "{jit_start:x} {jit_size:x} {name}").unwrap();
        }
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

pub struct JitMemory {
    mem: Mmap,
    mem_common_end: usize,
    mem_start: usize,
    mem_end: usize,
    jit_funcs: VecDeque<(usize, u16, u16)>,
    jit_entries: JitEntries,
    jit_live_ranges: JitLiveRanges,
    pub jit_memory_map: JitMemoryMap,
    jit_perf_map_record: JitPerfMapRecord,
    pub arm7_hle: bool,
}

impl JitMemory {
    pub fn new() -> Self {
        let jit_entries = JitEntries::new();
        let jit_live_ranges = JitLiveRanges::default();
        let jit_memory_map = JitMemoryMap::new(&jit_entries, &jit_live_ranges);
        JitMemory {
            mem: Mmap::executable("jit", JIT_MEMORY_SIZE).unwrap(),
            mem_common_end: 0,
            mem_start: 0,
            mem_end: JIT_MEMORY_SIZE,
            jit_funcs: VecDeque::new(),
            jit_entries,
            jit_live_ranges,
            jit_memory_map,
            jit_perf_map_record: JitPerfMapRecord::new(),
            arm7_hle: false,
        }
    }

    fn reset_blocks(&mut self) {
        self.jit_perf_map_record.reset();

        let (jit_entry, addr_offset_start, addr_offset_end) = self.jit_funcs.pop_front().unwrap();
        let jit_entry = jit_entry as *mut JitEntry;
        unsafe { *jit_entry = DEFAULT_JIT_ENTRY };
        let freed_start = addr_offset_start;
        let mut freed_end = addr_offset_end;
        while (freed_end - freed_start) < (JIT_MEMORY_SIZE / 6 / PAGE_SIZE) as u16 {
            let (jit_entry, _, addr_offset_end) = self.jit_funcs.front().unwrap();
            if *addr_offset_end < freed_start {
                break;
            }
            let jit_entry = *jit_entry as *mut JitEntry;
            unsafe { *jit_entry = DEFAULT_JIT_ENTRY };
            freed_end = *addr_offset_end;
            self.jit_funcs.pop_front().unwrap();
        }

        self.mem_start = (freed_start as usize) << PAGE_SHIFT;
        self.mem_end = (freed_end as usize) << PAGE_SHIFT;

        debug_println!("Jit memory reset from {:x} - {:x}", self.mem_start, self.mem_end);
    }

    fn allocate_block(&mut self, required_size: usize) -> (usize, bool) {
        let mut flushed = false;
        if self.mem_start + required_size > self.mem_end {
            self.reset_blocks();
            assert!(self.mem_start + required_size <= self.mem_end);
            flushed = true;
        }

        let addr = self.mem_start;
        self.mem_start += required_size;
        (addr, flushed)
    }

    pub fn get_start_entry(&self) -> usize {
        self.mem.as_ptr() as _
    }

    pub fn get_next_entry(&self, opcodes_len: usize) -> usize {
        let aligned_size = utils::align_up(opcodes_len << 2, PAGE_SIZE);
        if self.mem_start + aligned_size > self.mem_end {
            self.mem_common_end
        } else {
            self.mem_start
        }
    }

    pub fn insert_common_fun_block(&mut self, opcodes: &[u32], name: impl AsRef<str>) -> *const extern "C" fn() {
        let aligned_size = utils::align_up(size_of_val(opcodes), PAGE_SIZE);
        let mem_start = self.mem_start;

        utils::write_to_mem_slice(&mut self.mem, mem_start, opcodes);
        unsafe { flush_icache(self.mem.as_ptr().add(mem_start), aligned_size) };

        self.mem_start += aligned_size;
        self.mem_common_end = self.mem_start;

        let jit_entry_addr = mem_start + self.mem.as_ptr() as usize;
        self.jit_perf_map_record.record_common(jit_entry_addr, aligned_size, name);
        jit_entry_addr as _
    }

    fn insert(&mut self, opcodes: &[u32]) -> (usize, usize, bool) {
        let aligned_size = utils::align_up(size_of_val(opcodes), PAGE_SIZE);
        let (allocated_offset_addr, flushed) = self.allocate_block(aligned_size);

        utils::write_to_mem_slice(&mut self.mem, allocated_offset_addr, opcodes);
        unsafe { flush_icache(self.mem.as_ptr().add(allocated_offset_addr), aligned_size) };

        (allocated_offset_addr, aligned_size, flushed)
    }

    pub fn insert_block(&mut self, opcodes: &[u32], guest_pc: u32, cpu_type: CpuType, emu: &Emu) -> (*const extern "C" fn(), bool) {
        macro_rules! insert {
            ($entries:expr, $live_ranges:expr, $region:expr, [$($cpu_entry:expr),+]) => {{
                let ret = insert!($entries, $live_ranges);
                $(
                    let mmu = get_mmu!(emu, $cpu_entry);
                    mmu.remove_write(guest_pc, &$region);
                )*
                ret
            }};

            ($entries:expr, $live_ranges:expr) => {{
                let (allocated_offset_addr, aligned_size, flushed) = self.insert(opcodes);

                let jit_entry_addr = (allocated_offset_addr + self.mem.as_ptr() as usize) as *const extern "C" fn();

                let entries_index = (guest_pc >> 1) as usize;
                let entries_index = entries_index % $entries.len();
                $entries[entries_index] = JitEntry(jit_entry_addr);
                assert_eq!(ptr::addr_of!($entries[entries_index]), self.jit_memory_map.get_jit_entry(guest_pc), "jit memory mapping {guest_pc:x}");

                self.jit_funcs.push_back((ptr::addr_of!($entries[entries_index]) as usize, (allocated_offset_addr >> PAGE_SHIFT) as u16, ((allocated_offset_addr + aligned_size) >> PAGE_SHIFT) as u16));

                // >> 3 for u8 (each bit represents a page)
                let live_ranges_index = ((guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) >> 3) as usize;
                let live_ranges_index = live_ranges_index % $live_ranges.len();
                let live_ranges_bit = (guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 0x7;
                $live_ranges[live_ranges_index] |= 1 << live_ranges_bit;
                assert_eq!(ptr::addr_of!($live_ranges[live_ranges_index]), self.jit_memory_map.get_live_range(guest_pc), "jit live ranges mapping {guest_pc:x}");

                let per = (self.mem_start * 100) as f32 / JIT_MEMORY_SIZE as f32;
                debug_println!(
                    "Insert new jit ({:x}) block with size {} at {:x}, {}% allocated with guest pc {:x}",
                    self.mem.as_ptr() as usize,
                    aligned_size,
                    allocated_offset_addr,
                    per,
                    guest_pc
                );

                self.jit_perf_map_record.record(jit_entry_addr as usize, aligned_size, guest_pc, cpu_type);

                (jit_entry_addr, flushed)
            }};
        }

        match cpu_type {
            ARM9 => match guest_pc & 0xFF000000 {
                regions::ITCM_OFFSET | regions::ITCM_OFFSET2 => insert!(self.jit_entries.itcm, self.jit_live_ranges.itcm, regions::ITCM_REGION, [ARM9]),
                regions::MAIN_OFFSET => insert!(self.jit_entries.main, self.jit_live_ranges.main, regions::MAIN_REGION, [ARM9, ARM7]),
                regions::VRAM_OFFSET => insert!(self.jit_entries.vram, self.jit_live_ranges.vram),
                _ => todo!("{:x}", guest_pc),
            },
            ARM7 => match guest_pc & 0xFF000000 {
                regions::MAIN_OFFSET => insert!(self.jit_entries.main, self.jit_live_ranges.main, regions::MAIN_REGION, [ARM9, ARM7]),
                regions::SHARED_WRAM_OFFSET => {
                    if guest_pc & regions::ARM7_WRAM_OFFSET == regions::ARM7_WRAM_OFFSET {
                        insert!(self.jit_entries.wram_arm7, self.jit_live_ranges.wram_arm7, regions::ARM7_WRAM_REGION, [ARM7])
                    } else {
                        insert!(self.jit_entries.shared_wram_arm7, self.jit_live_ranges.shared_wram_arm7, regions::SHARED_WRAM_ARM7_REGION, [ARM7])
                    }
                }
                regions::VRAM_OFFSET => insert!(self.jit_entries.vram, self.jit_live_ranges.vram),
                _ => todo!("{:x}", guest_pc),
            },
        }
    }

    pub fn get_jit_start_addr(&self, guest_pc: u32) -> *const extern "C" fn() {
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
                    let jit_entry_start = self.jit_memory_map.get_jit_entry(guest_addr_start);
                    unsafe { slice::from_raw_parts_mut(jit_entry_start, JIT_LIVE_RANGE_PAGE_SIZE as usize).fill(DEFAULT_JIT_ENTRY) }
                }
            }};
        }

        invalidate!(guest_addr);
        invalidate!(guest_addr + size as u32 - 1);
    }

    pub fn invalidate_wram(&mut self) {
        if !self.arm7_hle {
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

    pub fn patch_slow_mem(&mut self, host_pc: &mut usize) -> bool {
        if *host_pc < self.mem.as_ptr() as usize || *host_pc >= self.mem.as_ptr() as usize + JIT_MEMORY_SIZE {
            return false;
        }

        let nop_opcode = AluShiftImm::mov_al(Reg::R0, Reg::R0);
        for pc_offset in (4..128).step_by(4) {
            let ptr = (*host_pc + pc_offset) as *mut u32;
            let opcode = unsafe { ptr.read() };
            if opcode == nop_opcode {
                // Already patched, double transfers
                *host_pc += 4;
                return true;
            } else {
                let (op, _) = lookup_opcode(opcode);
                if *op == Op::B {
                    unsafe { ptr.write(nop_opcode) };
                    *host_pc += 4;
                    unsafe { flush_icache(ptr as _, 4) };
                    return true;
                }
            }
        }

        false
    }
}
