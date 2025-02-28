use crate::core::emu::{get_mmu, Emu};
use crate::core::memory::{regions, vram};
use crate::core::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_mem_handler::{inst_mem_handler_multiple_write_gx_fifo, inst_mem_handler_write_gx_fifo};
use crate::jit::jit_asm::{emit_code_block, hle_bios_uninterrupt};
use crate::jit::jit_memory_map::JitMemoryMap;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::{Cond, MemoryAmount};
use crate::logging::debug_println;
use crate::mmap::{flush_icache, Mmap, PAGE_SHIFT, PAGE_SIZE};
use crate::settings::{Arm7Emu, Settings};
use crate::utils;
use crate::utils::{HeapMem, HeapMemU8};
use std::collections::VecDeque;
use std::intrinsics::unlikely;
use std::ops::Deref;
use std::ptr;
use CpuType::{ARM7, ARM9};

const JIT_MEMORY_SIZE: usize = 24 * 1024 * 1024;
pub const JIT_LIVE_RANGE_PAGE_SIZE_SHIFT: u32 = 8;
const JIT_LIVE_RANGE_PAGE_SIZE: u32 = 1 << JIT_LIVE_RANGE_PAGE_SIZE_SHIFT;
const JIT_ARM9_MEMORY_SIZE: usize = 20 * 1024 * 1024;
const JIT_ARM7_MEMORY_SIZE: usize = JIT_MEMORY_SIZE - JIT_ARM9_MEMORY_SIZE;

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

struct JitMemoryMetadata {
    size: usize,
    start: usize,
    end: usize,
    max_end: usize,
    jit_funcs: VecDeque<(usize, u16, u16)>,
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
    mem: Mmap,
    arm9_data: JitMemoryMetadata,
    arm7_data: JitMemoryMetadata,
    jit_entries: JitEntries,
    jit_live_ranges: JitLiveRanges,
    pub jit_memory_map: JitMemoryMap,
    jit_perf_map_record: JitPerfMapRecord,
}

impl JitMemory {
    pub fn new(settings: &Settings) -> Self {
        let jit_entries = JitEntries::new();
        let jit_live_ranges = JitLiveRanges::default();
        let jit_memory_map = JitMemoryMap::new(&jit_entries, &jit_live_ranges);
        JitMemory {
            mem: Mmap::executable("jit", JIT_MEMORY_SIZE).unwrap(),
            arm9_data: if settings.arm7_hle() == Arm7Emu::Hle {
                JitMemoryMetadata::new(JIT_MEMORY_SIZE, 0, JIT_MEMORY_SIZE)
            } else {
                JitMemoryMetadata::new(JIT_ARM9_MEMORY_SIZE, 0, JIT_ARM9_MEMORY_SIZE)
            },
            arm7_data: if settings.arm7_hle() == Arm7Emu::Hle {
                JitMemoryMetadata::new(0, 0, 0)
            } else {
                JitMemoryMetadata::new(JIT_ARM7_MEMORY_SIZE, JIT_ARM9_MEMORY_SIZE, JIT_MEMORY_SIZE)
            },
            jit_entries,
            jit_live_ranges,
            jit_memory_map,
            jit_perf_map_record: JitPerfMapRecord::new(),
        }
    }

    fn get_jit_data(&mut self, cpu_type: CpuType) -> &mut JitMemoryMetadata {
        match cpu_type {
            ARM9 => &mut self.arm9_data,
            ARM7 => &mut self.arm7_data,
        }
    }

    fn reset_blocks(&mut self, cpu_type: CpuType) {
        // self.jit_perf_map_record.reset();

        let jit_data = self.get_jit_data(cpu_type);

        let (jit_entry, addr_offset_start, addr_offset_end) = jit_data.jit_funcs.pop_front().unwrap();
        let jit_entry = jit_entry as *mut JitEntry;
        unsafe { *jit_entry = DEFAULT_JIT_ENTRY };
        let freed_start = addr_offset_start;
        let mut freed_end = addr_offset_end;
        while (freed_end - freed_start) < (jit_data.size / 4 / PAGE_SIZE) as u16 {
            let (jit_entry, _, addr_offset_end) = jit_data.jit_funcs.front().unwrap();
            if *addr_offset_end < freed_start {
                break;
            }
            let jit_entry = *jit_entry as *mut JitEntry;
            unsafe { *jit_entry = DEFAULT_JIT_ENTRY };
            freed_end = *addr_offset_end;
            jit_data.jit_funcs.pop_front().unwrap();
        }

        jit_data.start = (freed_start as usize) << PAGE_SHIFT;
        jit_data.end = (freed_end as usize) << PAGE_SHIFT;

        debug_println!("{cpu_type:?} Jit memory reset from {:x} - {:x}", jit_data.start, jit_data.end);
    }

    fn allocate_block(&mut self, required_size: usize, cpu_type: CpuType) -> (usize, bool) {
        let mut flushed = false;
        let jit_data = self.get_jit_data(cpu_type);
        if jit_data.start + required_size > jit_data.end {
            if jit_data.start + required_size > jit_data.max_end {
                let (_, _, last_addr_end) = jit_data.jit_funcs.back_mut().unwrap();
                *last_addr_end = (jit_data.max_end >> PAGE_SHIFT) as u16;
            }
            self.reset_blocks(cpu_type);
            let jit_data = self.get_jit_data(cpu_type);
            assert!(jit_data.start + required_size <= jit_data.end);
            flushed = true;
        }

        let jit_data = self.get_jit_data(cpu_type);
        let addr = jit_data.start;
        jit_data.start += required_size;
        (addr, flushed)
    }

    fn insert(&mut self, opcodes: &[u32], cpu_type: CpuType) -> (usize, usize, bool) {
        let aligned_size = utils::align_up(size_of_val(opcodes), PAGE_SIZE);
        let (allocated_offset_addr, flushed) = self.allocate_block(aligned_size, cpu_type);

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
                let (allocated_offset_addr, aligned_size, flushed) = self.insert(opcodes, cpu_type);

                let jit_entry_addr = (allocated_offset_addr + self.mem.as_ptr() as usize) as *const extern "C" fn();

                let entries_index = (guest_pc >> 1) as usize;
                let entries_index = entries_index % $entries.len();
                $entries[entries_index] = JitEntry(jit_entry_addr);
                assert_eq!(ptr::addr_of!($entries[entries_index]), self.jit_memory_map.get_jit_entry(guest_pc), "jit memory mapping {guest_pc:x}");

                let entry_addr = ptr::addr_of!($entries[entries_index]) as usize;
                self.get_jit_data(cpu_type).jit_funcs.push_back((entry_addr, (allocated_offset_addr >> PAGE_SHIFT) as u16, ((allocated_offset_addr + aligned_size) >> PAGE_SHIFT) as u16));

                // >> 3 for u8 (each bit represents a page)
                let live_ranges_index = ((guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) >> 3) as usize;
                let live_ranges_index = live_ranges_index % $live_ranges.len();
                let live_ranges_bit = (guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 0x7;
                $live_ranges[live_ranges_index] |= 1 << live_ranges_bit;
                assert_eq!(ptr::addr_of!($live_ranges[live_ranges_index]), self.jit_memory_map.get_live_range(guest_pc), "jit live ranges mapping {guest_pc:x}");

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
                    self.jit_memory_map.write_jit_entries(guest_addr_start, JIT_LIVE_RANGE_PAGE_SIZE as usize, DEFAULT_JIT_ENTRY);
                }
            }};
        }

        invalidate!(guest_addr);
        invalidate!(guest_addr + size as u32 - 1);
    }

    pub fn invalidate_wram(&mut self) {
        if self.arm7_data.size != 0 {
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

    pub fn patch_slow_mem(&mut self, host_pc: &mut usize, guest_memory_addr: u32, cpu: CpuType) -> bool {
        if *host_pc < self.mem.as_ptr() as usize || *host_pc >= self.mem.as_ptr() as usize + JIT_MEMORY_SIZE {
            debug_println!("Segfault outside of guest context");
            return false;
        }

        let nop_opcode = AluShiftImm::mov_al(Reg::R0, Reg::R0);
        for pc_offset in (4..256).step_by(4) {
            let ptr = (*host_pc + pc_offset) as *mut u32;
            let opcode = unsafe { ptr.read() };
            if opcode == nop_opcode {
                // Already patched, double transfers
                *host_pc += 4;
                return true;
            } else {
                let (op, _) = lookup_opcode(opcode);
                if *op == Op::B {
                    break;
                }
            }
        }

        let mut fast_mem_begin = *host_pc - 4;
        let mut found = false;
        while *host_pc - fast_mem_begin < 256 {
            let ptr = fast_mem_begin as *const u32;
            if unsafe { ptr.read() } == nop_opcode {
                found = true;
                break;
            }
            fast_mem_begin -= 4;
        }
        if !found {
            return false;
        }
        let mut fast_mem_end = *host_pc + 4;
        found = false;
        while fast_mem_end - *host_pc < 256 {
            let ptr = fast_mem_end as *const u32;
            let (op, _) = lookup_opcode(unsafe { ptr.read() });
            if *op == Op::B {
                found = true;
                break;
            }
            fast_mem_end += 4;
        }
        if !found {
            return false;
        }

        let slow_mem_branch = fast_mem_end + 4;
        let slow_mem_branch_ptr = slow_mem_branch as *const u32;
        let slow_mem_branch_opcode = unsafe { slow_mem_branch_ptr.read() };
        let (op, fun) = lookup_opcode(slow_mem_branch_opcode);
        if *op != Op::B {
            return false;
        }
        let slow_mem_branch_inst_info = fun(slow_mem_branch_opcode, *op);
        let slow_mem_relative_pc = *slow_mem_branch_inst_info.operands()[0].as_imm().unwrap() + 8;
        let slow_mem_begin = (slow_mem_branch as i32 + slow_mem_relative_pc as i32) as usize;

        if cpu == ARM9 && guest_memory_addr >= 0x4000400 && guest_memory_addr < 0x4000440 {
            let (fault_op, _) = lookup_opcode(unsafe { (*host_pc as *const u32).read() });
            if fault_op.mem_is_write() {
                let mut slow_mem_end = slow_mem_begin + 4;
                found = false;
                while slow_mem_end - slow_mem_begin < 256 {
                    let ptr = slow_mem_end as *const u32;
                    let (op, _) = lookup_opcode(unsafe { ptr.read() });
                    if *op == Op::B {
                        found = true;
                        break;
                    }
                    slow_mem_end += 4;
                }

                if !found {
                    return false;
                }

                let guest_op_ptr = (slow_mem_end + 4) as *const u32;
                let guest_op = unsafe { guest_op_ptr.read() };
                let op = if guest_op == guest_op & 0xFFFF {
                    lookup_thumb_opcode(guest_op as u16).0
                } else {
                    lookup_opcode(guest_op).0
                };

                if (op.is_single_mem_transfer() && MemoryAmount::from(op) == MemoryAmount::Word) || op.is_multiple_mem_transfer() {
                    let mut blx_opcode_pc = slow_mem_begin + 4;
                    let mut blx_reg = Reg::R0;
                    found = false;
                    while blx_opcode_pc - slow_mem_begin < 256 {
                        let ptr = blx_opcode_pc as *const u32;
                        let opcode = unsafe { ptr.read() };
                        let (op, fun) = lookup_opcode(opcode);
                        if *op == Op::BlxReg {
                            found = true;
                            let inst_info = fun(opcode, *op);
                            blx_reg = *inst_info.operands()[0].as_reg_no_shift().unwrap();
                            break;
                        }
                        blx_opcode_pc += 4;
                    }

                    if !found {
                        return false;
                    }

                    const MOV_MASK: u32 = 0xFFF0F000;
                    let mov16_op = AluImm::mov16_al(blx_reg, 0) & MOV_MASK;
                    let mov_t_op = AluImm::mov_t_al(blx_reg, 0) & MOV_MASK;

                    found = false;

                    let mut mov_reg_pc = blx_opcode_pc - 4;
                    while blx_opcode_pc - mov_reg_pc < 256 {
                        let mov_16_ptr = (mov_reg_pc - 4) as *const u32;
                        let mov_t_ptr = mov_reg_pc as *const u32;
                        mov_reg_pc -= 4;
                        if unsafe { mov_16_ptr.read() } & MOV_MASK == mov16_op && unsafe { mov_t_ptr.read() } & MOV_MASK == mov_t_op {
                            found = true;
                            break;
                        }
                    }

                    if !found {
                        return false;
                    }

                    let func = if op.is_single_mem_transfer() {
                        inst_mem_handler_write_gx_fifo as *const ()
                    } else {
                        match (op.mem_transfer_pre(), op.mem_transfer_write_back(), op.mem_transfer_decrement()) {
                            (false, false, false) => inst_mem_handler_multiple_write_gx_fifo::<false, false, false> as _,
                            (false, false, true) => inst_mem_handler_multiple_write_gx_fifo::<false, false, true> as _,
                            (false, true, false) => inst_mem_handler_multiple_write_gx_fifo::<false, true, false> as _,
                            (false, true, true) => inst_mem_handler_multiple_write_gx_fifo::<false, true, true> as _,
                            (true, false, false) => inst_mem_handler_multiple_write_gx_fifo::<true, false, false> as _,
                            (true, false, true) => inst_mem_handler_multiple_write_gx_fifo::<true, false, true> as _,
                            (true, true, false) => inst_mem_handler_multiple_write_gx_fifo::<true, true, false> as _,
                            (true, true, true) => inst_mem_handler_multiple_write_gx_fifo::<true, true, true> as _,
                        }
                    };

                    let (mov_opcodes, mov_length) = AluImm::mov32(blx_reg, func as u32);
                    debug_assert_eq!(mov_length, 2);
                    unsafe { (mov_reg_pc as *mut u32).write(mov_opcodes[0]) };
                    unsafe { ((mov_reg_pc + 4) as *mut u32).write(mov_opcodes[1]) };
                    unsafe { flush_icache(mov_reg_pc as _, 8) };
                }
            }
        }

        let diff = (slow_mem_begin - fast_mem_begin) >> 2;
        unsafe {
            (fast_mem_begin as *mut u32).write(B::b(diff as i32 - 2, Cond::AL));
            (fast_mem_end as *mut u32).write(nop_opcode);
        }
        *host_pc += 4;
        unsafe { flush_icache(fast_mem_begin as _, fast_mem_end - fast_mem_begin + 4) }
        true
    }
}
