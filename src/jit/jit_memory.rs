use crate::core::emu::Emu;
use crate::core::memory::{regions, vram};
use crate::core::CpuType;
use crate::jit::assembler::block_asm::GuestInstMetadata;
use crate::jit::assembler::{arm, thumb};
use crate::jit::inst_mem_handler::{
    inst_mem_handler_multiple, inst_read64_mem_handler, inst_read64_mem_handler_with_cpsr, inst_read_mem_handler, inst_read_mem_handler_with_cpsr, inst_write_mem_handler,
    inst_write_mem_handler_with_cpsr, InstMemMultipleParams,
};
use crate::jit::jit_asm::{emit_code_block, hle_bios_uninterrupt};
use crate::jit::jit_memory_map::JitMemoryMap;
use crate::jit::op::{MultipleTransfer, Op, SingleTransfer};
use crate::jit::reg::Reg;
use crate::jit::{Cond, MemoryAmount};
use crate::logging::debug_println;
use crate::mmap::{flush_icache, ArmContext, Mmap, PAGE_SHIFT, PAGE_SIZE};
use crate::settings::{Arm7Emu, Settings};
use crate::utils;
use crate::utils::{HeapMem, HeapMemU8};
use bilge::prelude::{u4, u6};
use std::collections::VecDeque;
use std::hint::unreachable_unchecked;
use std::intrinsics::unlikely;
use std::ops::Deref;
use std::{ptr, slice};
use CpuType::{ARM7, ARM9};

const JIT_MEMORY_SIZE: usize = 24 * 1024 * 1024;
pub const JIT_LIVE_RANGE_PAGE_SIZE_SHIFT: u32 = 8;
const JIT_LIVE_RANGE_PAGE_SIZE: u32 = 1 << JIT_LIVE_RANGE_PAGE_SIZE_SHIFT;
const JIT_ARM9_MEMORY_SIZE: usize = 20 * 1024 * 1024;
const JIT_ARM7_MEMORY_SIZE: usize = JIT_MEMORY_SIZE - JIT_ARM9_MEMORY_SIZE;

const SLOW_MEM_SINGLE_WRITE_LENGTH_THUMB: usize = 22;
const SLOW_MEM_SINGLE_READ_LENGTH_THUMB: usize = 18;
const SLOW_MEM_MULTIPLE_LENGTH_THUMB: usize = 26;

const SLOW_MEM_SINGLE_WRITE_LENGTH_ARM: usize = 28;
const SLOW_MEM_SINGLE_READ_LENGTH_ARM: usize = 24;
const SLOW_MEM_MULTIPLE_LENGTH_ARM: usize = 28;
pub const SLOW_SWP_MEM_SINGLE_WRITE_LENGTH_ARM: usize = 20;
pub const SLOW_SWP_MEM_SINGLE_READ_LENGTH_ARM: usize = 12;

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
    guest_inst_metadata: HeapMem<Vec<GuestInstMetadata>, { JIT_MEMORY_SIZE / PAGE_SIZE }>,
}

impl Emu {
    pub fn jit_insert_block(&mut self, opcodes: &[u8], guest_inst_metadata: &[(u16, GuestInstMetadata)], guest_pc: u32, thumb: bool, cpu_type: CpuType) -> (*const extern "C" fn(), bool) {
        macro_rules! insert {
            ($entries:expr, $live_ranges:expr, $region:expr, [$($cpu_entry:expr),+]) => {{
                let ret = insert!($entries, $live_ranges);
                $(
                    self.mmu_remove_write::<{ $cpu_entry }>(guest_pc, &$region);
                )*
                ret
            }};

            ($entries:expr, $live_ranges:expr) => {{
                let (allocated_offset_addr, aligned_size, flushed) = self.jit.insert(opcodes, guest_inst_metadata, cpu_type);

                let jit_entry_addr = ((allocated_offset_addr + self.jit.mem.as_ptr() as usize) | (thumb as usize)) as *const extern "C" fn();

                let entries_index = (guest_pc >> 1) as usize;
                let entries_index = entries_index % $entries.len();
                $entries[entries_index] = JitEntry(jit_entry_addr);
                assert_eq!(ptr::addr_of!($entries[entries_index]), self.jit.jit_memory_map.get_jit_entry(guest_pc), "jit memory mapping {guest_pc:x}");

                let entry_addr = ptr::addr_of!($entries[entries_index]) as usize;
                self.jit.get_jit_data(cpu_type).jit_funcs.push_back((entry_addr, (allocated_offset_addr >> PAGE_SHIFT) as u16, ((allocated_offset_addr + aligned_size) >> PAGE_SHIFT) as u16));

                // >> 3 for u8 (each bit represents a page)
                let live_ranges_index = ((guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) >> 3) as usize;
                let live_ranges_index = live_ranges_index % $live_ranges.len();
                let live_ranges_bit = (guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 0x7;
                $live_ranges[live_ranges_index] |= 1 << live_ranges_bit;
                assert_eq!(ptr::addr_of!($live_ranges[live_ranges_index]), self.jit.jit_memory_map.get_live_range(guest_pc), "jit live ranges mapping {guest_pc:x}");

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
            guest_inst_metadata: HeapMem::new(),
        }
    }

    fn get_jit_data(&mut self, cpu_type: CpuType) -> &mut JitMemoryMetadata {
        match cpu_type {
            ARM9 => &mut self.arm9_data,
            ARM7 => &mut self.arm7_data,
        }
    }

    fn reset_blocks(&mut self, cpu_type: CpuType) {
        self.jit_perf_map_record.reset();

        let (jit_entry, addr_offset_start, addr_offset_end) = self.get_jit_data(cpu_type).jit_funcs.pop_front().unwrap();
        let jit_entry = jit_entry as *mut JitEntry;

        unsafe { *jit_entry = DEFAULT_JIT_ENTRY };
        for i in addr_offset_start..addr_offset_end {
            self.guest_inst_metadata[i as usize].clear();
        }

        let jit_size = self.get_jit_data(cpu_type).size;
        let freed_start = addr_offset_start;
        let mut freed_end = addr_offset_end;
        while (freed_end - freed_start) < (jit_size / 4 / PAGE_SIZE) as u16 {
            let &(jit_entry, addr_offset_start, addr_offset_end) = self.get_jit_data(cpu_type).jit_funcs.front().unwrap();
            if addr_offset_end < freed_start {
                break;
            }
            let jit_entry = jit_entry as *mut JitEntry;

            unsafe { *jit_entry = DEFAULT_JIT_ENTRY };
            for i in addr_offset_start..addr_offset_end {
                self.guest_inst_metadata[i as usize].clear();
            }

            freed_end = addr_offset_end;
            self.get_jit_data(cpu_type).jit_funcs.pop_front().unwrap();
        }

        let jit_data = self.get_jit_data(cpu_type);
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

    fn insert(&mut self, opcodes: &[u8], guest_inst_metadata: &[(u16, GuestInstMetadata)], cpu_type: CpuType) -> (usize, usize, bool) {
        let aligned_size = utils::align_up(opcodes.len(), PAGE_SIZE);
        let (allocated_offset_addr, flushed) = self.allocate_block(aligned_size, cpu_type);

        utils::write_to_mem_slice(&mut self.mem, allocated_offset_addr, opcodes);
        unsafe { flush_icache(self.mem.as_ptr().add(allocated_offset_addr), aligned_size) };

        let block_page = allocated_offset_addr >> PAGE_SHIFT;
        for (block_offset, metadata) in guest_inst_metadata {
            self.guest_inst_metadata[block_page + *block_offset as usize].push(metadata.clone());
        }

        (allocated_offset_addr, aligned_size, flushed)
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

    fn get_inst_mem_handler_fun<const CPU: CpuType>(is_write: bool, transfer: SingleTransfer, cpsr_dirty: bool) -> *const () {
        if cpsr_dirty {
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

    fn get_inst_mem_multiple_handler_fun<const CPU: CpuType>(is_write: bool, transfer: MultipleTransfer) -> *const () {
        match (is_write, transfer.write_back(), !transfer.add()) {
            (false, false, false) => inst_mem_handler_multiple::<CPU, false, false, false> as _,
            (false, false, true) => inst_mem_handler_multiple::<CPU, false, false, true> as _,
            (false, true, false) => inst_mem_handler_multiple::<CPU, false, true, false> as _,
            (false, true, true) => inst_mem_handler_multiple::<CPU, false, true, true> as _,
            (true, false, false) => inst_mem_handler_multiple::<CPU, true, false, false> as _,
            (true, false, true) => inst_mem_handler_multiple::<CPU, true, false, true> as _,
            (true, true, false) => inst_mem_handler_multiple::<CPU, true, true, false> as _,
            (true, true, true) => inst_mem_handler_multiple::<CPU, true, true, true> as _,
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

    fn fast_mem_blx<const THUMB: bool>(fast_mem: &mut [u8], offset: &mut usize, reg: Reg) {
        if THUMB {
            Self::write_to_fast_mem(fast_mem, offset, thumb::BlxReg::blx_reg(reg));
        } else {
            Self::write_to_fast_mem(fast_mem, offset, arm::branch_assembler::Bx::blx(reg, Cond::AL));
        }
    }

    unsafe fn execute_patch_slow_mem<const THUMB: bool>(host_pc: &mut usize, guest_memory_addr: u32, fast_mem: &mut [u8], guest_inst_metadata: &GuestInstMetadata, cpu: CpuType) {
        let mut slow_mem_length = 0;

        if guest_inst_metadata.op.is_single_mem_transfer() {
            let transfer = match guest_inst_metadata.op {
                Op::Ldr(transfer) | Op::LdrT(transfer) | Op::Str(transfer) | Op::StrT(transfer) => transfer,
                _ => unreachable_unchecked(),
            };

            let inst_mem_func = match cpu {
                ARM9 => Self::get_inst_mem_handler_fun::<{ ARM9 }>(guest_inst_metadata.op.is_write_mem_transfer(), transfer, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR)),
                ARM7 => Self::get_inst_mem_handler_fun::<{ ARM7 }>(guest_inst_metadata.op.is_write_mem_transfer(), transfer, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR)),
            };
            Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R12, inst_mem_func as u32);

            if guest_inst_metadata.op.is_write_mem_transfer() {
                let guest_inst_metadata_ptr = guest_inst_metadata as *const _;
                Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R3, guest_inst_metadata_ptr as u32);

                if guest_inst_metadata.op0 != Reg::R0 {
                    Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R0, guest_inst_metadata.op0);
                    if transfer.size() == 3 {
                        let mapped_next = guest_inst_metadata.mapped_guest_regs[guest_inst_metadata.operands.values[0].as_reg_no_shift().unwrap_unchecked() as usize + 1];
                        Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R0, mapped_next);
                    }
                }
            } else {
                Self::fast_mem_mov::<THUMB>(
                    fast_mem,
                    &mut slow_mem_length,
                    Reg::R0,
                    guest_inst_metadata.operands.values[0].as_reg_no_shift().unwrap_unchecked() as u32,
                );
            }

            Self::fast_mem_blx::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R12);

            if !guest_inst_metadata.op.is_write_mem_transfer() {
                Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, guest_inst_metadata.op0, Reg::R0);
                if transfer.size() == 3 {
                    let mapped_next = guest_inst_metadata.mapped_guest_regs[guest_inst_metadata.operands.values[0].as_reg_no_shift().unwrap_unchecked() as usize + 1];
                    Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, mapped_next, Reg::R1);
                }
            }

            let max_length = Self::get_slow_mem_length(guest_inst_metadata.op);
            debug_assert!(slow_mem_length <= max_length, "{slow_mem_length} < {max_length}");
        } else if guest_inst_metadata.op.is_multiple_mem_transfer() {
            let transfer = match guest_inst_metadata.op {
                Op::Ldm(transfer) | Op::LdmT(transfer) | Op::Stm(transfer) | Op::StmT(transfer) => transfer,
                _ => unreachable_unchecked(),
            };

            let inst_mem_func = match cpu {
                ARM9 => Self::get_inst_mem_multiple_handler_fun::<{ ARM9 }>(guest_inst_metadata.op.is_write_mem_transfer(), transfer),
                ARM7 => Self::get_inst_mem_multiple_handler_fun::<{ ARM7 }>(guest_inst_metadata.op.is_write_mem_transfer(), transfer),
            };

            let mut pre = transfer.pre();
            if !transfer.add() {
                pre = !pre;
            }

            let op1 = guest_inst_metadata.operands.values[1].as_reg_list().unwrap_unchecked();
            let params = InstMemMultipleParams::new(op1.0 as u16, u4::new(op1.len() as u8), u4::new(guest_inst_metadata.op0 as u8), pre, transfer.user(), u6::new(0));

            Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R12, inst_mem_func as u32);

            Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R0, u32::from(params));

            let guest_inst_metadata_ptr = guest_inst_metadata as *const _;
            Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R1, guest_inst_metadata_ptr as u32);

            Self::fast_mem_blx::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R12);

            let max_length = Self::get_slow_mem_length(guest_inst_metadata.op);
            debug_assert!(slow_mem_length <= max_length, "{slow_mem_length} < {max_length}");
        } else if !THUMB && matches!(guest_inst_metadata.op, Op::Swpb | Op::Swp) {
            let is_write = guest_inst_metadata.op0 == Reg::R1;

            let size = match guest_inst_metadata.op {
                Op::Swpb => 0,
                Op::Swp => 2,
                _ => unsafe { unreachable_unchecked() },
            };
            let transfer = SingleTransfer::new(false, false, false, false, size);
            let inst_mem_func = match cpu {
                ARM9 => Self::get_inst_mem_handler_fun::<{ ARM9 }>(is_write, transfer, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR)),
                ARM7 => Self::get_inst_mem_handler_fun::<{ ARM7 }>(is_write, transfer, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR)),
            };
            Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R12, inst_mem_func as u32);

            if is_write {
                let guest_inst_metadata_ptr = guest_inst_metadata as *const _;
                Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R3, guest_inst_metadata_ptr as u32);
            }

            Self::fast_mem_blx::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R12);

            let max_length = if is_write { SLOW_SWP_MEM_SINGLE_WRITE_LENGTH_ARM } else { SLOW_SWP_MEM_SINGLE_READ_LENGTH_ARM };
            debug_assert!(slow_mem_length <= max_length, "{slow_mem_length} < {max_length}");
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

    unsafe fn find_fast_mem<const THUMB: bool>(pc: *mut u8) -> (*mut u8, usize) {
        let pc_shift = if THUMB { 1 } else { 2 };
        let mut fast_mem_start = ptr::null_mut();
        for i in 1..64 {
            let pc = pc.sub(i << pc_shift);
            if THUMB {
                if (pc as *mut u16).read() == thumb::NOP {
                    fast_mem_start = pc;
                    break;
                }
            } else if (pc as *mut u32).read() == arm::NOP {
                fast_mem_start = pc;
                break;
            }
        }
        if fast_mem_start.is_null() {
            return (ptr::null_mut(), 0);
        }
        let mut fast_mem_end = ptr::null_mut();
        for i in 1..64 {
            let pc = pc.add(i << pc_shift);
            let mut found = false;
            if THUMB {
                if (pc as *mut u16).read() == thumb::NOP {
                    fast_mem_end = pc;
                    found = true;
                }
            } else if (pc as *mut u32).read() == arm::NOP {
                fast_mem_end = pc;
                found = true;
            }
            if !found && !fast_mem_end.is_null() {
                break;
            }
        }
        if fast_mem_end.is_null() {
            return (ptr::null_mut(), 0);
        }
        (fast_mem_start, fast_mem_end as usize - fast_mem_start as usize + (1 << pc_shift))
    }

    pub unsafe fn patch_slow_mem(&mut self, host_pc: &mut usize, guest_memory_addr: u32, cpu: CpuType, _: &ArmContext) -> bool {
        if *host_pc < self.mem.as_ptr() as usize || *host_pc >= self.mem.as_ptr() as usize + JIT_MEMORY_SIZE {
            debug_println!("Segfault outside of guest context");
            return false;
        }

        let jit_mem_offset = *host_pc - self.mem.as_mut_ptr() as usize;
        let metadata_block_page = jit_mem_offset >> PAGE_SHIFT;
        let opcode_offset = jit_mem_offset & (PAGE_SIZE - 1);
        let guest_inst_metadata_list = &self.guest_inst_metadata[metadata_block_page];

        for metadata in guest_inst_metadata_list {
            if metadata.opcode_offset == opcode_offset {
                let thumb = metadata.pc & 1 == 1;

                let (fast_mem_start, fast_mem_size) = if thumb {
                    Self::find_fast_mem::<true>(*host_pc as _)
                } else {
                    Self::find_fast_mem::<false>(*host_pc as _)
                };

                if fast_mem_start.is_null() {
                    debug_println!("Can't find fast mem range");
                    return false;
                }

                let fast_mem = slice::from_raw_parts_mut(fast_mem_start, fast_mem_size);
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
