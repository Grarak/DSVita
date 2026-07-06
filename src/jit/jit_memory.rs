use crate::core::emu::Emu;
use crate::core::memory::io_arm7::io_arm7;
use crate::core::memory::io_arm9::io_arm9;
use crate::core::memory::mmu::MMU_PAGE_SHIFT;
use crate::core::memory::{regions, vram};
use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType;
#[cfg(target_arch = "arm")]
use crate::jit::assembler::arm::alu_assembler::AluShiftImm;
#[cfg(target_arch = "arm")]
use crate::jit::assembler::arm::transfer_assembler::{LdrStrImm, LdrStrImmSBHD};
#[cfg(target_arch = "arm")]
use crate::jit::assembler::block_asm::BlockAsm;
#[cfg(target_arch = "arm")]
use crate::jit::assembler::{arm, thumb};
use crate::jit::assembler::{GuestInstMetadata, GuestInstOffset};
#[cfg(target_arch = "arm")]
use crate::jit::inst_mem_handler::{
    inst_read64_mem_handler, inst_read64_mem_handler_with_cpsr, inst_read_io_mem_handler, inst_read_io_mem_handler_with_cpsr, inst_read_mem_handler, inst_read_mem_handler_multiple,
    inst_read_mem_handler_multiple_with_cpsr, inst_read_mem_handler_with_cpsr, inst_write_io_mem_handler, inst_write_io_mem_handler_with_cpsr, inst_write_mem_handler, inst_write_mem_handler_gxfifo,
    inst_write_mem_handler_gxfifo_with_cpsr, inst_write_mem_handler_multiple, inst_write_mem_handler_multiple_gxfifo, inst_write_mem_handler_multiple_gxfifo_with_cpsr,
    inst_write_mem_handler_multiple_with_cpsr, inst_write_mem_handler_with_cpsr, InstMemMultipleParams,
};
use crate::jit::jit_asm::{emit_code_block, hle_bios_uninterrupt, JitDebugInfo};
use crate::jit::jit_memory_map::JitMemoryMap;
use crate::jit::op::{MultipleTransfer, Op, SingleTransfer};
use crate::jit::reg::Reg;
use crate::jit::{Cond, MemoryAmount};
use crate::logging::debug_println;
use crate::mmap::{flush_icache, ArmContext, MemRegion, Mmap, PAGE_SHIFT, PAGE_SIZE};

#[cfg(target_os = "linux")]
fn block_hash_log(cpu: CpuType, guest_pc: u32, thumb: bool, code: &[u8]) {
    use std::io::Write;
    // One-time env lookup; None = disabled (the common case, one branch per block insert).
    static WRITER: std::sync::OnceLock<Option<std::sync::Mutex<std::io::BufWriter<std::fs::File>>>> = std::sync::OnceLock::new();
    let writer = WRITER.get_or_init(|| {
        std::env::var("DSVITA_BLOCK_HASH_LOG")
            .ok()
            .map(|path| std::sync::Mutex::new(std::io::BufWriter::new(std::fs::File::create(path).unwrap())))
    });
    if let Some(writer) = writer {
        // DSVITA_BLOCK_DUMP_PC=<hex pc> additionally dumps that block's raw bytes — the
        // debugging companion when the gate reports a mismatch.
        if let Ok(dump_pc) = std::env::var("DSVITA_BLOCK_DUMP_PC") {
            if u32::from_str_radix(&dump_pc, 16) == Ok(guest_pc) {
                let _ = std::fs::write(format!("/tmp/block_{guest_pc:x}_{}_{}.bin", thumb as u8, std::process::id()), code);
            }
        }
        // Baked host pointers differ between BINARIES of the same source lineage — and under
        // qemu's PIE layout they collide with guest-range values, so masking goes by the
        // MATERIALIZATION PATTERN on the pristine bytes (a blanket value mask would zero
        // instruction words too and blind the gate). Emitted code only materializes host
        // pointers into scratch registers (r0-r3 via the driver seam, r12 via call());
        // masked before hashing:
        //   - movw/movt pairs targeting a scratch register (both ISAs),
        //   - the pool words of pc-relative literal loads: always for a scratch rd, and for
        //     other rds only when the word sits in the unambiguous high host ranges (heap
        //     pointers in the HLE substitution blocks).
        // Scratch-register guest constants get masked too — deterministically on both
        // sides, so the compare stays sound (a small false-negative surface; lengths and
        // all other bytes still compare exactly).
        const SCRATCH: [u32; 5] = [0, 1, 2, 3, 12];
        let mut masked = code.to_vec();
        let n_words = masked.len() / 4;
        // (pool word index, rd) of every pc-relative literal load.
        let mut pool_refs = Vec::new();
        if thumb {
            let hw = |b: &[u8], i: usize| u16::from_le_bytes([b[i * 2], b[i * 2 + 1]]);
            let n = masked.len() / 2;
            for i in 0..n {
                let h1 = hw(&masked, i);
                // T16 ldr rd, [pc, #imm8*4]: pool word at align4(pc+4)+imm.
                if h1 & 0xF800 == 0x4800 {
                    let target = ((i * 2 + 4) & !3) + ((h1 & 0xFF) as usize) * 4;
                    if target / 4 < n_words {
                        pool_refs.push((target / 4, (h1 >> 8 & 7) as u32));
                    }
                }
                // T32 ldr.w rd, [pc, #imm12] (F8DF).
                if h1 == 0xF8DF && i + 1 < n {
                    let h2 = hw(&masked, i + 1);
                    let target = ((i * 2 + 4) & !3) + (h2 & 0xFFF) as usize;
                    if target / 4 < n_words {
                        pool_refs.push((target / 4, (h2 >> 12) as u32));
                    }
                }
                // T32 movw/movt pair into a scratch reg.
                if i + 3 < n {
                    let (h2, h3, h4) = (hw(&masked, i + 1), hw(&masked, i + 2), hw(&masked, i + 3));
                    let rd = (h2 >> 8 & 0xF) as u32;
                    if h1 & 0xFBF0 == 0xF240 && h3 & 0xFBF0 == 0xF2C0 && h4 >> 8 & 0xF == h2 >> 8 & 0xF && SCRATCH.contains(&rd) {
                        for (j, h) in [(i, h1 & !0x040F), (i + 1, h2 & !0x70FF), (i + 2, h3 & !0x040F), (i + 3, h4 & !0x70FF)] {
                            masked[j * 2..j * 2 + 2].copy_from_slice(&h.to_le_bytes());
                        }
                    }
                }
            }
        } else {
            let wd = |b: &[u8], i: usize| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
            for i in 0..n_words {
                let w1 = wd(&masked, i);
                // A32 ldr rd, [pc, #imm12]: pool word at pc+8+imm.
                if w1 & 0x0FFF0000 == 0x059F0000 {
                    let target = i * 4 + 8 + (w1 & 0xFFF) as usize;
                    if target / 4 < n_words {
                        pool_refs.push((target / 4, w1 >> 12 & 0xF));
                    }
                }
                // A32 low-half + movt pair into a scratch reg. The low half is movw, or a
                // plain mov/mvn when the value's bottom 16 bits are rotation-encodable
                // (vixl picks the shorter form).
                if i + 1 < n_words {
                    let w2 = wd(&masked, i + 1);
                    let rd = w1 >> 12 & 0xF;
                    let is_movw = w1 & 0x0FF00000 == 0x03000000;
                    let is_mov_imm = w1 & 0x0FEF0000 == 0x03A00000 || w1 & 0x0FEF0000 == 0x03E00000;
                    if (is_movw || is_mov_imm) && w2 & 0x0FF00000 == 0x03400000 && w2 >> 12 & 0xF == rd && SCRATCH.contains(&rd) {
                        // Canonicalize: vixl picks movw or a rotated mov for the low half
                        // depending on the value, so the surviving opcode bits must not
                        // depend on that choice — only cond and rd stay.
                        let canon1 = (w1 & 0xF0000000) | 0x03000000 | (rd << 12);
                        let canon2 = (w2 & 0xF0000000) | 0x03400000 | (rd << 12);
                        masked[i * 4..i * 4 + 4].copy_from_slice(&canon1.to_le_bytes());
                        masked[(i + 1) * 4..(i + 1) * 4 + 4].copy_from_slice(&canon2.to_le_bytes());
                    }
                }
            }
        }
        for (word, rd) in pool_refs {
            let value = u32::from_le_bytes(masked[word * 4..word * 4 + 4].try_into().unwrap());
            let high_host = (0x10000000..0x70000000).contains(&value) || (0xA2000000..0xFFFF0000).contains(&value);
            if SCRATCH.contains(&rd) || high_host {
                masked[word * 4..word * 4 + 4].fill(0);
            }
        }
        let hash = xxhash_rust::xxh32::xxh32(&masked, 0);
        let mut writer = writer.lock().unwrap();
        let _ = writeln!(writer, "{} {guest_pc:x} {} {} {hash:08x}", cpu as u8, thumb as u8, code.len());
        let _ = writer.flush();
    }
}
use crate::settings::{Arm7Emu, Settings};
use crate::utils;
use crate::utils::{HeapArray, HeapArrayU8};
use bilge::prelude::{u4, u6};
use std::collections::VecDeque;
use std::hint::{assert_unchecked, unreachable_unchecked};
use std::intrinsics::unlikely;
use std::ops::{Deref, DerefMut};
use std::{mem, ptr, slice};
use CpuType::{ARM7, ARM9};

pub const JIT_MEMORY_SIZE: usize = 32 * 1024 * 1024;
pub const JIT_LIVE_RANGE_PAGE_SIZE_SHIFT: u32 = 8;
const JIT_LIVE_RANGE_PAGE_SIZE: u32 = 1 << JIT_LIVE_RANGE_PAGE_SIZE_SHIFT;
const JIT_ARM9_MEMORY_SIZE: usize = 28 * 1024 * 1024;
const JIT_ARM7_MEMORY_SIZE: usize = JIT_MEMORY_SIZE - JIT_ARM9_MEMORY_SIZE;

const SLOW_MEM_SINGLE_WRITE_LENGTH_THUMB: usize = 22;
const SLOW_MEM_SINGLE_READ_LENGTH_THUMB: usize = 22;
const SLOW_MEM_MULTIPLE_LENGTH_THUMB: usize = 26;

const SLOW_MEM_SINGLE_WRITE_LENGTH_ARM: usize = 28;
const SLOW_MEM_SINGLE_READ_LENGTH_ARM: usize = 28;
const SLOW_MEM_MULTIPLE_LENGTH_ARM: usize = 28;
pub const SLOW_SWP_MEM_SINGLE_WRITE_LENGTH_ARM: usize = 20;
pub const SLOW_SWP_MEM_SINGLE_READ_LENGTH_ARM: usize = 20;

/// Per-instruction dispatch metadata of an aarch64 block, keyed by the block's first jit
/// page: the entry-pc dispatch resolves a mid-block guest pc to its host code offset and
/// the pre_cycle_count_sum the block's flush math expects there (the a64 twin of the
/// arm32 GuestInstOffset tables — no register mappings, everything lives in memory).
#[cfg(target_arch = "aarch64")]
#[derive(Copy, Clone, Default)]
pub struct A64InstOffset {
    pub code_offset: u32,
    pub pre_cycle_count_sum: u16,
}

#[cfg(target_arch = "aarch64")]
#[derive(Default)]
pub struct A64BlockMeta {
    pub guest_pc_start: u32,
    pub insts: Vec<A64InstOffset>,
}

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
                pub $block_name: HeapArray<JitEntry, { $size as usize / 2 }>,
            )*
        }

        impl JitEntries {
            fn new() -> Self {
                let mut instance = JitEntries {
                    $(
                        $block_name: HeapArray::default(),
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
    pub itcm: HeapArrayU8<{ (regions::ITCM_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 8) as usize }>,
    pub main: HeapArrayU8<{ (regions::MAIN_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 8) as usize }>,
    pub vram: HeapArrayU8<{ (vram::ARM7_SIZE / JIT_LIVE_RANGE_PAGE_SIZE / 8) as usize }>, // Use arm7 vram size for arm9 as well
}

// The interpreter's hotness counters (see jit/interpreter): one u8 per halfword of executable
// memory, laid out per region exactly like JitEntries and indexed through jit_memory_map, which
// collapses memory mirrors the same way it does for the jit entries. A single set serves both
// cpus — arm9 and arm7 don't execute from overlapping address ranges.
#[derive(Default)]
pub struct JitExecCounts {
    pub itcm: HeapArrayU8<{ regions::ITCM_SIZE as usize / 2 }>,
    pub main: HeapArrayU8<{ regions::MAIN_SIZE as usize / 2 }>,
    pub shared_wram_arm7: HeapArrayU8<{ regions::SHARED_WRAM_SIZE as usize / 2 }>,
    pub wram_arm7: HeapArrayU8<{ regions::ARM7_WRAM_SIZE as usize / 2 }>,
    pub vram: HeapArrayU8<{ vram::ARM7_SIZE as usize / 2 }>, // Use arm7 vram size for arm9 as well
}

impl JitExecCounts {
    fn reset(&mut self) {
        self.itcm.fill(0);
        self.main.fill(0);
        self.shared_wram_arm7.fill(0);
        self.wram_arm7.fill(0);
        self.vram.fill(0);
    }
}

#[cfg(target_os = "linux")]
struct JitPerfMapRecord {
    perf_map_path: std::path::PathBuf,
    perf_map: std::fs::File,
    // With DSVITA_JITDUMP set, blocks are additionally written as a perf jitdump (code bytes
    // included, thumb bit on the address) so `perf inject --jit` + a genelf-patched perf can
    // annotate inside jit blocks; the plain map stays for flat symbolization.
    jit_dump: Option<crate::jit::jit_perf_log::JitPerfLog>,
}

#[cfg(target_os = "linux")]
impl JitPerfMapRecord {
    fn new() -> Self {
        let perf_map_path = std::path::PathBuf::from(format!("/tmp/perf-{}.map", std::process::id()));
        JitPerfMapRecord {
            perf_map_path: perf_map_path.clone(),
            perf_map: std::fs::File::create(perf_map_path).unwrap(),
            jit_dump: std::env::var("DSVITA_JITDUMP")
                .is_ok()
                .then(|| crate::jit::jit_perf_log::JitPerfLog::new(std::path::PathBuf::from("/tmp"))),
        }
    }

    fn record(&mut self, jit_start: usize, jit_size: usize, guest_pc: u32, cpu_type: CpuType, thumb: bool) {
        use std::io::Write;
        writeln!(self.perf_map, "{jit_start:x} {jit_size:x} {cpu_type:?}_{guest_pc:x}").unwrap();
        if let Some(dump) = &mut self.jit_dump {
            let code = unsafe { slice::from_raw_parts((jit_start & !1) as *const u8, jit_size) };
            dump.load(&format!("{cpu_type:?}_{guest_pc:x}"), code, thumb);
        }
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

    fn record(&mut self, jit_start: usize, jit_size: usize, guest_pc: u32, cpu_type: CpuType, thumb: bool) {}

    fn reset(&mut self) {}
}

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

#[derive(Default)]
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
    pub mem: Mmap,
    arm9_data: JitMemoryMetadata,
    arm7_data: JitMemoryMetadata,
    jit_entries: JitEntries,
    jit_live_ranges: JitLiveRanges,
    jit_exec_counts: JitExecCounts,
    pub jit_memory_map: JitMemoryMap,
    jit_perf_map_record: JitPerfMapRecord,
    pub guest_inst_offsets: HeapArray<Vec<GuestInstOffset>, { JIT_MEMORY_SIZE / PAGE_SIZE }>,
    guest_inst_metadata: HeapArray<Vec<GuestInstMetadata>, { JIT_MEMORY_SIZE / PAGE_SIZE }>,
    #[cfg(target_arch = "aarch64")]
    pub a64_block_meta: HeapArray<A64BlockMeta, { JIT_MEMORY_SIZE / PAGE_SIZE }>,
}

impl Emu {
    pub fn jit_protect_region<const CPU: CpuType>(&mut self, guest_pc: u32, guest_pc_end: u32, thumb: bool, region: &MemRegion) {
        let guest_pc_end = guest_pc_end - if thumb { 2 } else { 4 };
        let begin = guest_pc >> MMU_PAGE_SHIFT;
        let end = guest_pc_end >> MMU_PAGE_SHIFT;
        for i in begin..=end {
            self.mmu_remove_write::<CPU>(i << MMU_PAGE_SHIFT, region);
        }
    }

    pub fn jit_set_live_range(&mut self, guest_pc: u32, guest_pc_end: u32, thumb: bool) {
        // >> 3 for u8 (each bit represents a page)
        let guest_pc_end = guest_pc_end - if thumb { 2 } else { 4 };
        let live_range_begin = guest_pc >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT;
        let live_range_end = guest_pc_end >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT;
        for i in live_range_begin..=live_range_end {
            let live_ranges_bit = i & 0x7;
            let live_range_ptr = self.jit.jit_memory_map.get_live_range(i << JIT_LIVE_RANGE_PAGE_SIZE_SHIFT);
            if !live_range_ptr.is_null() {
                unsafe { *live_range_ptr |= 1 << live_ranges_bit };
            }
        }
    }

    #[cfg(target_arch = "arm")]
    pub fn jit_insert_block(&mut self, block_asm: BlockAsm, debug_info: &JitDebugInfo, guest_pc: u32, guest_pc_end: u32, thumb: bool, cpu: CpuType) -> (*const extern "C" fn(u32), bool) {
        // Byte-identity gate for backend refactors (the armv7-sacred check): with
        // DSVITA_BLOCK_HASH_LOG=<path> every compiled block's pre-relocation bytes are
        // hashed and appended as "cpu pc thumb len hash" — two builds of the same source
        // lineage must produce identical streams over a deterministic boot.
        #[cfg(target_os = "linux")]
        block_hash_log(cpu, guest_pc, thumb, block_asm.get_code_buffer());
        macro_rules! insert {
            ($entries:expr, $region:expr, [$($cpu_entry:expr),+]) => {{
                let ret = insert!($entries);
                $(
                    self.jit_protect_region::<{ $cpu_entry }>(guest_pc, guest_pc_end, thumb, &$region);
                )*
                ret
            }};

            ($entries:expr) => {{
                let (allocated_offset_addr, aligned_size, flushed) = self.jit.insert(block_asm, cpu);

                let jit_entry_addr = ((allocated_offset_addr + self.jit.mem.as_ptr() as usize) | (thumb as usize)) as *const extern "C" fn(u32);

                let guest_block_size = (guest_pc_end - guest_pc) as usize;
                debug_assert!(guest_block_size < PAGE_SIZE);
                self.jit.jit_memory_map.write_jit_entries(guest_pc, guest_block_size, JitEntry(jit_entry_addr));

                let metadata = JitBlockMetadata::new(guest_pc | (thumb as u32), guest_pc_end | (thumb as u32), (allocated_offset_addr >> PAGE_SHIFT) as u16, ((allocated_offset_addr + aligned_size) >> PAGE_SHIFT) as u16);
                self.jit.get_jit_data(cpu).jit_funcs.push_back(metadata);

                self.jit_set_live_range(guest_pc, guest_pc_end, thumb);

                #[cfg(any(debug_assertions, target_os = "linux"))]
                for &(pc, offset, size) in &debug_info.blocks {
                    self.jit.jit_perf_map_record.record(jit_entry_addr as usize + offset, size, pc, cpu, thumb);
                }

                (jit_entry_addr, flushed)
            }};
        }

        match cpu {
            ARM9 => match guest_pc & 0xFF000000 {
                regions::ITCM_OFFSET | regions::ITCM_OFFSET2 => insert!(self.jit.jit_entries.itcm, regions::ITCM_REGION, [ARM9]),
                regions::MAIN_OFFSET => {
                    if self.fs_clear_overlay_image_addr != 0 && self.nitro_sdk_version.rely_on_fs_invalidation() {
                        insert!(self.jit.jit_entries.main)
                    } else {
                        insert!(self.jit.jit_entries.main, regions::MAIN_REGION, [ARM9, ARM7])
                    }
                }
                regions::VRAM_OFFSET => insert!(self.jit.jit_entries.vram),
                _ => todo!("{:x}", guest_pc),
            },
            ARM7 => match guest_pc & 0xFF000000 {
                regions::MAIN_OFFSET => insert!(self.jit.jit_entries.main),
                regions::SHARED_WRAM_OFFSET => {
                    if guest_pc & regions::ARM7_WRAM_OFFSET == regions::ARM7_WRAM_OFFSET {
                        insert!(self.jit.jit_entries.wram_arm7)
                    } else {
                        insert!(self.jit.jit_entries.shared_wram_arm7)
                    }
                }
                regions::VRAM_OFFSET => insert!(self.jit.jit_entries.vram),
                _ => todo!("{:x}", guest_pc),
            },
        }
    }

    /// aarch64 twin of jit_insert_block: takes finalized code bytes plus the per-inst
    /// dispatch metadata (no entry patching or fastmem yet — later stage-5 slices).
    /// Returns the entry and whether the allocation flushed jit memory — the caller must
    /// exit the guest context after running a block that flushed, exactly like arm32
    /// (host frames above may belong to freed blocks).
    #[cfg(target_arch = "aarch64")]
    pub fn jit_insert_block_a64(&mut self, code: &[u8], block_meta: A64BlockMeta, guest_pc: u32, guest_pc_end: u32, thumb: bool, cpu: CpuType) -> (*const extern "C" fn(u32), bool) {
        block_hash_log(cpu, guest_pc, thumb, code);

        macro_rules! insert {
            ($entries:expr, $region:expr, [$($cpu_entry:expr),+]) => {{
                let ret = insert!($entries);
                $(
                    self.jit_protect_region::<{ $cpu_entry }>(guest_pc, guest_pc_end, thumb, &$region);
                )*
                ret
            }};

            ($entries:expr) => {{
                let aligned_size = utils::align_up(code.len(), PAGE_SIZE);
                let (allocated_offset_addr, flushed) = self.jit.allocate_block(aligned_size, cpu);
                utils::write_to_mem_slice(&mut self.jit.mem, allocated_offset_addr, code);
                unsafe { flush_icache(self.jit.mem.as_ptr().add(allocated_offset_addr), aligned_size) };

                let jit_entry_addr = ((allocated_offset_addr + self.jit.mem.as_ptr() as usize) | (thumb as usize)) as *const extern "C" fn(u32);

                let guest_block_size = (guest_pc_end - guest_pc) as usize;
                debug_assert!(guest_block_size < PAGE_SIZE);
                self.jit.jit_memory_map.write_jit_entries(guest_pc, guest_block_size, JitEntry(jit_entry_addr));

                let metadata = JitBlockMetadata::new(guest_pc | (thumb as u32), guest_pc_end | (thumb as u32), (allocated_offset_addr >> PAGE_SHIFT) as u16, ((allocated_offset_addr + aligned_size) >> PAGE_SHIFT) as u16);
                self.jit.get_jit_data(cpu).jit_funcs.push_back(metadata);

                self.jit_set_live_range(guest_pc, guest_pc_end, thumb);

                self.jit.a64_block_meta[allocated_offset_addr >> PAGE_SHIFT] = block_meta;

                (jit_entry_addr, flushed)
            }};
        }

        match cpu {
            ARM9 => match guest_pc & 0xFF000000 {
                regions::ITCM_OFFSET | regions::ITCM_OFFSET2 => insert!(self.jit.jit_entries.itcm, regions::ITCM_REGION, [ARM9]),
                regions::MAIN_OFFSET => insert!(self.jit.jit_entries.main, regions::MAIN_REGION, [ARM9, ARM7]),
                regions::VRAM_OFFSET => insert!(self.jit.jit_entries.vram),
                _ => todo!("{:x}", guest_pc),
            },
            ARM7 => match guest_pc & 0xFF000000 {
                regions::MAIN_OFFSET => insert!(self.jit.jit_entries.main),
                regions::SHARED_WRAM_OFFSET => {
                    if guest_pc & regions::ARM7_WRAM_OFFSET == regions::ARM7_WRAM_OFFSET {
                        insert!(self.jit.jit_entries.wram_arm7)
                    } else {
                        insert!(self.jit.jit_entries.shared_wram_arm7)
                    }
                }
                regions::VRAM_OFFSET => insert!(self.jit.jit_entries.vram),
                _ => todo!("{:x}", guest_pc),
            },
        }
    }
}

impl JitMemory {
    pub fn new() -> Self {
        let jit_entries = JitEntries::new();
        let jit_live_ranges = JitLiveRanges::default();
        let jit_exec_counts = JitExecCounts::default();
        let jit_memory_map = JitMemoryMap::new(&jit_entries, &jit_live_ranges, &jit_exec_counts);
        JitMemory {
            mem: Mmap::executable("jit", JIT_MEMORY_SIZE).unwrap(),
            arm9_data: JitMemoryMetadata::default(),
            arm7_data: JitMemoryMetadata::default(),
            jit_entries,
            jit_live_ranges,
            jit_exec_counts,
            jit_memory_map,
            jit_perf_map_record: JitPerfMapRecord::new(),
            guest_inst_offsets: HeapArray::default(),
            guest_inst_metadata: HeapArray::default(),
            #[cfg(target_arch = "aarch64")]
            a64_block_meta: HeapArray::default(),
        }
    }

    pub fn init(&mut self, settings: &Settings) {
        self.arm9_data = if settings.arm7_emu() == Arm7Emu::Hle {
            JitMemoryMetadata::new(JIT_MEMORY_SIZE, 0, JIT_MEMORY_SIZE)
        } else {
            JitMemoryMetadata::new(JIT_ARM9_MEMORY_SIZE, 0, JIT_ARM9_MEMORY_SIZE)
        };
        self.arm7_data = if settings.arm7_emu() == Arm7Emu::Hle {
            JitMemoryMetadata::new(0, 0, 0)
        } else {
            JitMemoryMetadata::new(JIT_ARM7_MEMORY_SIZE, JIT_ARM9_MEMORY_SIZE, JIT_MEMORY_SIZE)
        };
        self.jit_entries.reset();
        self.jit_live_ranges.itcm.fill(0);
        self.jit_live_ranges.main.fill(0);
        self.jit_live_ranges.vram.fill(0);
        self.jit_exec_counts.reset();
        self.jit_memory_map = JitMemoryMap::new(&self.jit_entries, &self.jit_live_ranges, &self.jit_exec_counts);
        for vec in self.guest_inst_offsets.deref_mut() {
            vec.clear();
        }
        for vec in self.guest_inst_metadata.deref_mut() {
            vec.clear();
        }
        #[cfg(target_arch = "aarch64")]
        for meta in self.a64_block_meta.deref_mut() {
            *meta = A64BlockMeta::default();
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

        let block_metadata = self.get_jit_data(cpu_type).jit_funcs.pop_front().unwrap();
        self.jit_memory_map
            .write_jit_entries(block_metadata.guest_pc, (block_metadata.guest_pc_end - block_metadata.guest_pc) as usize, DEFAULT_JIT_ENTRY);
        for i in block_metadata.addr_offset_start..block_metadata.addr_offset_end {
            self.guest_inst_metadata[i as usize].clear();
            #[cfg(target_arch = "aarch64")]
            {
                self.a64_block_meta[i as usize] = A64BlockMeta::default();
            }
        }

        let jit_size = self.get_jit_data(cpu_type).size;
        let freed_start = block_metadata.addr_offset_start;
        let mut freed_end = block_metadata.addr_offset_end;
        while (freed_end - freed_start) < (jit_size / 4 / PAGE_SIZE) as u16 {
            let block_metadata = self.get_jit_data(cpu_type).jit_funcs.front().unwrap();
            if block_metadata.addr_offset_end < freed_start {
                break;
            }

            let addr_offset_start = block_metadata.addr_offset_start;
            let addr_offset_end = block_metadata.addr_offset_end;
            let guest_pc = block_metadata.guest_pc;
            let guest_block_size = (block_metadata.guest_pc_end - block_metadata.guest_pc) as usize;

            self.jit_memory_map.write_jit_entries(guest_pc, guest_block_size, DEFAULT_JIT_ENTRY);
            for i in addr_offset_start..addr_offset_end {
                self.guest_inst_metadata[i as usize].clear();
                #[cfg(target_arch = "aarch64")]
                {
                    self.a64_block_meta[i as usize] = A64BlockMeta::default();
                }
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
                let block_metadata = jit_data.jit_funcs.back_mut().unwrap();
                block_metadata.addr_offset_end = (jit_data.max_end >> PAGE_SHIFT) as u16;
            }
            self.reset_blocks(cpu_type);
            let jit_data = self.get_jit_data(cpu_type);
            // reset_blocks frees at least a quarter of the region, so the allocation
            // always fits — checked in debug, assumed in release.
            unsafe { assert_unchecked(jit_data.start + required_size <= jit_data.end) };
            flushed = true;
        }

        let jit_data = self.get_jit_data(cpu_type);
        let addr = jit_data.start;
        jit_data.start += required_size;
        (addr, flushed)
    }

    #[cfg(target_arch = "arm")]
    fn insert(&mut self, block_asm: BlockAsm, cpu_type: CpuType) -> (usize, usize, bool) {
        let opcodes = block_asm.get_code_buffer();
        let aligned_size = utils::align_up(opcodes.len(), PAGE_SIZE);
        let (allocated_offset_addr, flushed) = self.allocate_block(aligned_size, cpu_type);

        utils::write_to_mem_slice(&mut self.mem, allocated_offset_addr, opcodes);

        let jit_entry_addr = (self.mem.as_ptr() as usize + allocated_offset_addr) as u32 | (block_asm.thumb as u32);
        for (reg, addr_offset) in block_asm.jit_entry_insert_locations {
            let addr_offset = allocated_offset_addr + addr_offset;
            let mut offset = 0;
            if block_asm.thumb {
                Self::fast_mem_mov::<true>(&mut self.mem[addr_offset..], &mut offset, reg, jit_entry_addr);
            } else {
                Self::fast_mem_mov::<false>(&mut self.mem[addr_offset..], &mut offset, reg, jit_entry_addr);
            }
        }

        unsafe { flush_icache(self.mem.as_ptr().add(allocated_offset_addr), aligned_size) };

        let block_page = allocated_offset_addr >> PAGE_SHIFT;
        self.guest_inst_offsets[block_page] = block_asm.guest_inst_offsets;
        for (block_offset, metadata) in block_asm.guest_inst_metadata {
            self.guest_inst_metadata[block_page + block_offset as usize].push(metadata);
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

    pub fn invalidate_blocks(&mut self, guest_addr: u32, size: usize) {
        let guest_addr_begin = guest_addr & !(JIT_LIVE_RANGE_PAGE_SIZE - 1);
        let guest_addr_end = utils::align_up(guest_addr as usize + size, JIT_LIVE_RANGE_PAGE_SIZE as usize) as u32;
        for addr in (guest_addr_begin..guest_addr_end).step_by(JIT_LIVE_RANGE_PAGE_SIZE as usize) {
            let live_range = unsafe { self.jit_memory_map.get_live_range(addr).as_mut_unchecked() };
            let live_ranges_bit = (addr >> JIT_LIVE_RANGE_PAGE_SIZE_SHIFT) & 0x7;
            if unlikely(*live_range & (1 << live_ranges_bit) != 0) {
                *live_range &= !(1 << live_ranges_bit);

                debug_println!("Invalidating multiple jit {addr:x} - {:x}", addr + JIT_LIVE_RANGE_PAGE_SIZE);
                self.jit_memory_map.write_jit_entries(addr, JIT_LIVE_RANGE_PAGE_SIZE as usize, DEFAULT_JIT_ENTRY);
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

    #[cfg(target_arch = "arm")]
    unsafe fn find_guest_inst_metadata(&mut self, jit_pc: usize) -> &mut GuestInstMetadata {
        let jit_mem_offset = jit_pc - self.mem.as_ptr() as usize;
        let metadata_block_page = jit_mem_offset >> PAGE_SHIFT;
        let opcode_offset = (jit_mem_offset & (PAGE_SIZE - 1)) & !1;
        let guest_inst_metadata_list = unsafe { self.guest_inst_metadata.get_unchecked_mut(metadata_block_page) };

        for guest_inst_metadata in guest_inst_metadata_list {
            if opcode_offset == guest_inst_metadata.s.fast.opcode_offset {
                return guest_inst_metadata;
            }
        }

        unsafe { unreachable_unchecked() }
    }

    #[cfg(target_arch = "arm")]
    fn get_inst_mem_handler_fun<const CPU: CpuType>(is_write: bool, transfer: SingleTransfer, guest_memory_addr: u32, cpsr_dirty: bool, io_func: &mut Option<*const ()>) -> *const () {
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
            return if cpsr_dirty {
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
            };
        }

        if transfer.size() != 3 && guest_memory_addr & 0xFF000000 == regions::IO_PORTS_OFFSET {
            let io_addr = guest_memory_addr & 0xFFFFFF;
            let dma_range = 0xB0..=0xEC;
            let spu_range = 0x400..=0x4FC;
            let size = 1 << transfer.size();
            let aligned_addr = io_addr & !(size - 1);
            *io_func = match CPU {
                ARM9 if !dma_range.contains(&io_addr) => {
                    if is_write {
                        io_arm9::get_write_with_size(aligned_addr, size as usize).map(|f| f as _)
                    } else {
                        io_arm9::get_read_with_size(aligned_addr, size as usize).map(|f| f as _)
                    }
                }
                ARM7 if !dma_range.contains(&io_addr) && !spu_range.contains(&io_addr) => {
                    if is_write {
                        io_arm7::get_write_with_size(aligned_addr, size as usize).map(|f| f as _)
                    } else {
                        io_arm7::get_read_with_size(aligned_addr, size as usize).map(|f| f as _)
                    }
                }
                _ => None,
            };

            if io_func.is_some() {
                return if cpsr_dirty {
                    _get_inst_mem_handler_fun!(
                        is_write,
                        transfer.size(),
                        transfer.signed(),
                        inst_write_io_mem_handler_with_cpsr,
                        inst_read_io_mem_handler_with_cpsr,
                        inst_read64_mem_handler_with_cpsr
                    )
                } else {
                    _get_inst_mem_handler_fun!(
                        is_write,
                        transfer.size(),
                        transfer.signed(),
                        inst_write_io_mem_handler,
                        inst_read_io_mem_handler,
                        inst_read64_mem_handler
                    )
                };
            }
        }

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

    #[cfg(target_arch = "arm")]
    fn get_inst_mem_multiple_handler_fun<const CPU: CpuType>(
        is_write: bool,
        transfer: MultipleTransfer,
        user: bool,
        valid: bool,
        needs_pc: bool,
        guest_memory_addr: u32,
        cpsr_dirty: bool,
    ) -> *const () {
        let is_gx_fifo = guest_memory_addr >= 0x4000400 && guest_memory_addr < 0x4000440;
        unsafe {
            assert_unchecked(transfer.write_back() || valid);
            assert_unchecked(is_write || !needs_pc);
            assert_unchecked(is_write || !is_gx_fifo);
            assert_unchecked(!user || !needs_pc);
        }

        macro_rules! _get_inst_mem_multiple_handler_fun {
            ($write_func:ident, $read_func:ident) => {
                match (is_write, transfer.write_back(), !transfer.add(), valid, user, needs_pc) {
                    (false, false, false, false, false, false) => $read_func::<CPU, false, false, false, false, false> as _,
                    (true, false, false, false, false, false) => $write_func::<CPU, false, false, false, false, false> as _,
                    (false, true, false, false, false, false) => $read_func::<CPU, true, false, false, false, false> as _,
                    (true, true, false, false, false, false) => $write_func::<CPU, true, false, false, false, false> as _,
                    (false, false, true, false, false, false) => $read_func::<CPU, false, true, false, false, false> as _,
                    (true, false, true, false, false, false) => $write_func::<CPU, false, true, false, false, false> as _,
                    (false, true, true, false, false, false) => $read_func::<CPU, true, true, false, false, false> as _,
                    (true, true, true, false, false, false) => $write_func::<CPU, true, true, false, false, false> as _,
                    (false, false, false, true, false, false) => $read_func::<CPU, false, false, true, false, false> as _,
                    (true, false, false, true, false, false) => $write_func::<CPU, false, false, true, false, false> as _,
                    (false, true, false, true, false, false) => $read_func::<CPU, true, false, true, false, false> as _,
                    (true, true, false, true, false, false) => $write_func::<CPU, true, false, true, false, false> as _,
                    (false, false, true, true, false, false) => $read_func::<CPU, false, true, true, false, false> as _,
                    (true, false, true, true, false, false) => $write_func::<CPU, false, true, true, false, false> as _,
                    (false, true, true, true, false, false) => $read_func::<CPU, true, true, true, false, false> as _,
                    (true, true, true, true, false, false) => $write_func::<CPU, true, true, true, false, false> as _,
                    (false, false, false, false, true, false) => $read_func::<CPU, false, false, false, true, false> as _,
                    (true, false, false, false, true, false) => $write_func::<CPU, false, false, false, true, false> as _,
                    (false, true, false, false, true, false) => $read_func::<CPU, true, false, false, true, false> as _,
                    (true, true, false, false, true, false) => $write_func::<CPU, true, false, false, true, false> as _,
                    (false, false, true, false, true, false) => $read_func::<CPU, false, true, false, true, false> as _,
                    (true, false, true, false, true, false) => $write_func::<CPU, false, true, false, true, false> as _,
                    (false, true, true, false, true, false) => $read_func::<CPU, true, true, false, true, false> as _,
                    (true, true, true, false, true, false) => $write_func::<CPU, true, true, false, true, false> as _,
                    (false, false, false, true, true, false) => $read_func::<CPU, false, false, true, true, false> as _,
                    (true, false, false, true, true, false) => $write_func::<CPU, false, false, true, true, false> as _,
                    (false, true, false, true, true, false) => $read_func::<CPU, true, false, true, true, false> as _,
                    (true, true, false, true, true, false) => $write_func::<CPU, true, false, true, true, false> as _,
                    (false, false, true, true, true, false) => $read_func::<CPU, false, true, true, true, false> as _,
                    (true, false, true, true, true, false) => $write_func::<CPU, false, true, true, true, false> as _,
                    (false, true, true, true, true, false) => $read_func::<CPU, true, true, true, true, false> as _,
                    (true, true, true, true, true, false) => $write_func::<CPU, true, true, true, true, false> as _,
                    (false, false, false, false, false, true) => $read_func::<CPU, false, false, false, false, true> as _,
                    (true, false, false, false, false, true) => $write_func::<CPU, false, false, false, false, true> as _,
                    (false, true, false, false, false, true) => $read_func::<CPU, true, false, false, false, true> as _,
                    (true, true, false, false, false, true) => $write_func::<CPU, true, false, false, false, true> as _,
                    (false, false, true, false, false, true) => $read_func::<CPU, false, true, false, false, true> as _,
                    (true, false, true, false, false, true) => $write_func::<CPU, false, true, false, false, true> as _,
                    (false, true, true, false, false, true) => $read_func::<CPU, true, true, false, false, true> as _,
                    (true, true, true, false, false, true) => $write_func::<CPU, true, true, false, false, true> as _,
                    (false, false, false, true, false, true) => $read_func::<CPU, false, false, true, false, true> as _,
                    (true, false, false, true, false, true) => $write_func::<CPU, false, false, true, false, true> as _,
                    (false, true, false, true, false, true) => $read_func::<CPU, true, false, true, false, true> as _,
                    (true, true, false, true, false, true) => $write_func::<CPU, true, false, true, false, true> as _,
                    (false, false, true, true, false, true) => $read_func::<CPU, false, true, true, false, true> as _,
                    (true, false, true, true, false, true) => $write_func::<CPU, false, true, true, false, true> as _,
                    (false, true, true, true, false, true) => $read_func::<CPU, true, true, true, false, true> as _,
                    (true, true, true, true, false, true) => $write_func::<CPU, true, true, true, false, true> as _,
                    (false, false, false, false, true, true) => $read_func::<CPU, false, false, false, true, true> as _,
                    (true, false, false, false, true, true) => $write_func::<CPU, false, false, false, true, true> as _,
                    (false, true, false, false, true, true) => $read_func::<CPU, true, false, false, true, true> as _,
                    (true, true, false, false, true, true) => $write_func::<CPU, true, false, false, true, true> as _,
                    (false, false, true, false, true, true) => $read_func::<CPU, false, true, false, true, true> as _,
                    (true, false, true, false, true, true) => $write_func::<CPU, false, true, false, true, true> as _,
                    (false, true, true, false, true, true) => $read_func::<CPU, true, true, false, true, true> as _,
                    (true, true, true, false, true, true) => $write_func::<CPU, true, true, false, true, true> as _,
                    (false, false, false, true, true, true) => $read_func::<CPU, false, false, true, true, true> as _,
                    (true, false, false, true, true, true) => $write_func::<CPU, false, false, true, true, true> as _,
                    (false, true, false, true, true, true) => $read_func::<CPU, true, false, true, true, true> as _,
                    (true, true, false, true, true, true) => $write_func::<CPU, true, false, true, true, true> as _,
                    (false, false, true, true, true, true) => $read_func::<CPU, false, true, true, true, true> as _,
                    (true, false, true, true, true, true) => $write_func::<CPU, false, true, true, true, true> as _,
                    (false, true, true, true, true, true) => $read_func::<CPU, true, true, true, true, true> as _,
                    (true, true, true, true, true, true) => $write_func::<CPU, true, true, true, true, true> as _,
                }
            };
        }

        if CPU == ARM9 && is_write && is_gx_fifo {
            if cpsr_dirty {
                _get_inst_mem_multiple_handler_fun!(inst_write_mem_handler_multiple_gxfifo_with_cpsr, inst_read_mem_handler_multiple_with_cpsr)
            } else {
                _get_inst_mem_multiple_handler_fun!(inst_write_mem_handler_multiple_gxfifo, inst_read_mem_handler_multiple)
            }
        // }else if !is_write && guest_memory_addr == 0x4000210 && rlist_len == 2 {
        } else if cpsr_dirty {
            _get_inst_mem_multiple_handler_fun!(inst_write_mem_handler_multiple_with_cpsr, inst_read_mem_handler_multiple_with_cpsr)
        } else {
            _get_inst_mem_multiple_handler_fun!(inst_write_mem_handler_multiple, inst_read_mem_handler_multiple)
        }
    }

    #[cfg(target_arch = "arm")]
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

    #[cfg(target_arch = "arm")]
    fn write_to_fast_mem<T>(fast_mem: &mut [u8], offset: &mut usize, value: T) {
        let ptr: &u8 = unsafe { mem::transmute(&value) };
        utils::write_to_mem_slice(fast_mem, *offset, unsafe { slice::from_raw_parts(ptr, size_of::<T>()) });
        *offset += size_of::<T>();
    }

    #[cfg(target_arch = "arm")]
    fn fast_mem_mov_reg<const THUMB: bool>(fast_mem: &mut [u8], offset: &mut usize, reg: Reg, reg2: Reg) {
        if THUMB {
            Self::write_to_fast_mem(fast_mem, offset, thumb::MovReg::mov(reg, reg2));
        } else {
            Self::write_to_fast_mem(fast_mem, offset, arm::alu_assembler::AluShiftImm::mov_al(reg, reg2));
        }
    }

    #[cfg(target_arch = "arm")]
    fn fast_mem_mov<const THUMB: bool>(fast_mem: &mut [u8], offset: &mut usize, reg: Reg, value: u32) {
        let (opcodes, length) = if THUMB { thumb::Mov::mov32(reg, value) } else { arm::alu_assembler::AluImm::mov32(reg, value) };
        for opcode in &opcodes[..length] {
            Self::write_to_fast_mem(fast_mem, offset, *opcode);
        }
    }

    #[cfg(target_arch = "arm")]
    fn fast_mem_blx<const THUMB: bool>(fast_mem: &mut [u8], offset: &mut usize, reg: Reg) {
        if THUMB {
            Self::write_to_fast_mem(fast_mem, offset, thumb::BlxReg::blx_reg(reg));
        } else {
            Self::write_to_fast_mem(fast_mem, offset, arm::branch_assembler::Bx::blx(reg, Cond::AL));
        }
    }

    #[cfg(target_arch = "arm")]
    unsafe fn execute_patch_slow_mem<const THUMB: bool>(host_pc: &mut usize, guest_memory_addr: u32, fast_mem: &mut [u8], guest_inst_metadata: &mut GuestInstMetadata, cpu: CpuType) {
        let mut slow_mem_length = 0;

        if !THUMB && guest_inst_metadata.s.fast.is_os_irq_handler {
            let is_write = guest_inst_metadata.s.fast.op.is_write_mem_transfer();
            if guest_inst_metadata.s.fast.op.is_single_mem_transfer() {
                let transfer = match guest_inst_metadata.s.fast.op {
                    Op::Ldr(transfer) | Op::Str(transfer) => transfer,
                    _ => unreachable_unchecked(),
                };

                if transfer.size() == 2 {
                    let op0 = guest_inst_metadata.s.fast.operands.values[0].as_reg_no_shift().unwrap_unchecked();
                    let op0_mapped = guest_inst_metadata.mapped_guest_regs[op0 as usize];
                    if is_write && guest_memory_addr == 0x4000214 {
                        let thread_reg_offset = mem::offset_of!(ThreadRegs, irf);
                        Self::write_to_fast_mem(fast_mem, &mut slow_mem_length, LdrStrImm::ldr_offset_al(Reg::R0, Reg::R3, thread_reg_offset as u16));
                        Self::write_to_fast_mem(fast_mem, &mut slow_mem_length, AluShiftImm::bic_al(Reg::R0, Reg::R0, op0_mapped));
                        Self::write_to_fast_mem(fast_mem, &mut slow_mem_length, LdrStrImm::str_offset_al(Reg::R0, Reg::R3, thread_reg_offset as u16));
                    } else if guest_memory_addr == 0x4000208 {
                        let thread_reg_offset = mem::offset_of!(ThreadRegs, ime);
                        let opcode = LdrStrImm::ldrb_offset_al(op0_mapped, Reg::R3, thread_reg_offset as u16);
                        Self::write_to_fast_mem(fast_mem, &mut slow_mem_length, opcode);
                    }
                }
            } else if guest_inst_metadata.s.fast.op.is_multiple_mem_transfer() {
                let rlist = guest_inst_metadata.s.fast.operands.values[1].as_reg_list().unwrap();
                if !is_write && guest_memory_addr == 0x4000210 && rlist.len() == 2 {
                    let op0 = rlist.get_lowest_reg();
                    let op1 = rlist.get_highest_reg();
                    let mapped_0 = guest_inst_metadata.mapped_guest_regs[op0 as usize];
                    let mapped_1 = guest_inst_metadata.mapped_guest_regs[op1 as usize];

                    let thread_reg_ie_offset = mem::offset_of!(ThreadRegs, ie);
                    let opcode = LdrStrImmSBHD::ldrd_al(Reg::R0, Reg::R3, thread_reg_ie_offset as u8);
                    Self::write_to_fast_mem(fast_mem, &mut slow_mem_length, opcode);

                    if mapped_0 == Reg::None {
                        let opcode = LdrStrImm::str_offset_al(Reg::R0, Reg::R3, (op0 as u16) << 2);
                        Self::write_to_fast_mem(fast_mem, &mut slow_mem_length, opcode);
                    } else {
                        Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, mapped_0, Reg::R0);
                    }

                    if mapped_1 == Reg::None {
                        let opcode = LdrStrImm::str_offset_al(Reg::R1, Reg::R3, (op1 as u16) << 2);
                        Self::write_to_fast_mem(fast_mem, &mut slow_mem_length, opcode);
                    } else {
                        Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, mapped_1, Reg::R0);
                    }
                }
            }
        }

        if slow_mem_length == 0 {
            if guest_inst_metadata.s.fast.op.is_single_mem_transfer() {
                let transfer = match guest_inst_metadata.s.fast.op {
                    Op::Ldr(transfer) | Op::LdrT(transfer) | Op::Str(transfer) | Op::StrT(transfer) => transfer,
                    _ => unreachable_unchecked(),
                };

                let is_write = guest_inst_metadata.s.fast.op.is_write_mem_transfer();
                let mut io_func = None;

                let inst_mem_func = match cpu {
                    ARM9 => Self::get_inst_mem_handler_fun::<{ ARM9 }>(is_write, transfer, guest_memory_addr, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR), &mut io_func),
                    ARM7 => Self::get_inst_mem_handler_fun::<{ ARM7 }>(is_write, transfer, guest_memory_addr, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR), &mut io_func),
                };
                Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::LR, inst_mem_func as u32);

                if is_write {
                    if guest_inst_metadata.s.fast.op0 != Reg::R0 {
                        Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R0, guest_inst_metadata.s.fast.op0);
                    }

                    if transfer.size() == 3 {
                        let op0 = guest_inst_metadata.s.fast.operands.values[0].as_reg_no_shift().unwrap_unchecked();
                        let mapped_next = guest_inst_metadata.mapped_guest_regs[op0 as usize + 1];
                        Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R1, mapped_next);
                    }
                }

                let guest_inst_metadata_ptr = guest_inst_metadata as *const _ as u32;
                if is_write {
                    Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R12, guest_inst_metadata_ptr);
                } else if io_func.is_some() {
                    Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R0, guest_inst_metadata_ptr);
                }

                Self::fast_mem_blx::<THUMB>(fast_mem, &mut slow_mem_length, Reg::LR);

                if !is_write {
                    Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, guest_inst_metadata.s.fast.op0, Reg::R0);
                    if transfer.size() == 3 {
                        let op0 = guest_inst_metadata.s.fast.operands.values[0].as_reg_no_shift().unwrap_unchecked();
                        let mapped_next = guest_inst_metadata.mapped_guest_regs[op0 as usize + 1];
                        Self::fast_mem_mov_reg::<THUMB>(fast_mem, &mut slow_mem_length, mapped_next, Reg::R1);
                    }
                }

                let max_length = Self::get_slow_mem_length(guest_inst_metadata.s.fast.op);
                debug_assert!(slow_mem_length <= max_length, "{slow_mem_length} <= {max_length}");

                if let Some(io_func) = io_func {
                    guest_inst_metadata.s.slow.initial_patch_addr = guest_memory_addr;
                    guest_inst_metadata.s.slow.io_func = io_func;
                }
            } else if guest_inst_metadata.s.fast.op.is_multiple_mem_transfer() {
                let transfer = match guest_inst_metadata.s.fast.op {
                    Op::Ldm(transfer) | Op::LdmT(transfer) | Op::Stm(transfer) | Op::StmT(transfer) => transfer,
                    _ => unreachable_unchecked(),
                };

                let is_write = guest_inst_metadata.s.fast.op.is_write_mem_transfer();
                let op1 = guest_inst_metadata.s.fast.operands.values[1].as_reg_list().unwrap_unchecked();
                let valid = !transfer.write_back() || !op1.is_reserved(guest_inst_metadata.s.fast.op0);

                let inst_mem_func = match cpu {
                    ARM9 => Self::get_inst_mem_multiple_handler_fun::<{ ARM9 }>(
                        is_write,
                        transfer,
                        transfer.user() && !op1.is_reserved(Reg::PC),
                        valid,
                        op1.is_reserved(Reg::PC),
                        guest_memory_addr,
                        guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR),
                    ),
                    ARM7 => Self::get_inst_mem_multiple_handler_fun::<{ ARM7 }>(
                        is_write,
                        transfer,
                        transfer.user() && !op1.is_reserved(Reg::PC),
                        valid,
                        op1.is_reserved(Reg::PC),
                        guest_memory_addr,
                        guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR),
                    ),
                };

                let mut pre = transfer.pre();
                if !transfer.add() {
                    pre = !pre;
                }

                let params = InstMemMultipleParams::new(op1.0 as u16, u4::new(op1.len() as u8), u4::new(guest_inst_metadata.s.fast.op0 as u8), pre, transfer.user(), u6::new(0));

                Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::LR, inst_mem_func as u32);

                Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R0, u32::from(params));

                let guest_inst_metadata_ptr = guest_inst_metadata as *const _;
                Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R1, guest_inst_metadata_ptr as u32);

                Self::fast_mem_blx::<THUMB>(fast_mem, &mut slow_mem_length, Reg::LR);

                let max_length = Self::get_slow_mem_length(guest_inst_metadata.s.fast.op);
                debug_assert!(slow_mem_length <= max_length, "{slow_mem_length} <= {max_length}");
            } else if !THUMB && matches!(guest_inst_metadata.s.fast.op, Op::Swpb | Op::Swp) {
                let is_write = guest_inst_metadata.s.fast.op0 == Reg::R1;
                let mut io_func = None;

                let size = match guest_inst_metadata.s.fast.op {
                    Op::Swpb => 0,
                    Op::Swp => 2,
                    _ => unsafe { unreachable_unchecked() },
                };
                let transfer = SingleTransfer::new(false, false, false, false, size);
                let inst_mem_func = match cpu {
                    ARM9 => Self::get_inst_mem_handler_fun::<{ ARM9 }>(is_write, transfer, guest_memory_addr, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR), &mut io_func),
                    ARM7 => Self::get_inst_mem_handler_fun::<{ ARM7 }>(is_write, transfer, guest_memory_addr, guest_inst_metadata.dirty_guest_regs.is_reserved(Reg::CPSR), &mut io_func),
                };
                Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::LR, inst_mem_func as u32);

                let guest_inst_metadata_ptr = guest_inst_metadata as *const _ as u32;
                if is_write {
                    Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R12, guest_inst_metadata_ptr);
                } else if io_func.is_some() {
                    Self::fast_mem_mov::<THUMB>(fast_mem, &mut slow_mem_length, Reg::R0, guest_inst_metadata_ptr);
                }

                Self::fast_mem_blx::<THUMB>(fast_mem, &mut slow_mem_length, Reg::LR);

                let max_length = if is_write { SLOW_SWP_MEM_SINGLE_WRITE_LENGTH_ARM } else { SLOW_SWP_MEM_SINGLE_READ_LENGTH_ARM };
                debug_assert!(slow_mem_length <= max_length, "{slow_mem_length} <= {max_length}");

                if let Some(io_func) = io_func {
                    guest_inst_metadata.s.slow.initial_patch_addr = guest_memory_addr;
                    guest_inst_metadata.s.slow.io_func = io_func;
                }
            } else {
                unreachable_unchecked()
            }
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

        *host_pc = fast_mem.as_ptr() as usize;
        flush_icache(fast_mem.as_ptr(), fast_mem.len());
    }

    pub fn is_in_jit_mem(&self, addr: usize) -> bool {
        addr >= self.mem.as_ptr() as usize && addr < self.mem.as_ptr() as usize + JIT_MEMORY_SIZE
    }

    #[cfg(target_arch = "arm")]
    pub unsafe fn patch_slow_mem(&mut self, host_pc: &mut usize, guest_memory_addr: u32, cpu: CpuType, _: &ArmContext) -> bool {
        if !self.is_in_jit_mem(*host_pc) {
            eprintln!("Segfault outside of guest context");
            return false;
        }

        let metadata = self.find_guest_inst_metadata(*host_pc);

        let fast_mem_start = (*host_pc - metadata.s.fast.start_offset as usize) as *mut u8;
        let fast_mem = slice::from_raw_parts_mut(fast_mem_start, metadata.s.fast.size as usize);

        debug_println!("{cpu:?} slow mem patch at {:x} {:?} addr {guest_memory_addr:x}", metadata.pc, metadata.s.fast.op);

        let thumb = metadata.pc & 1 == 1;
        if thumb {
            Self::execute_patch_slow_mem::<true>(host_pc, guest_memory_addr, fast_mem, metadata, cpu);
        } else {
            Self::execute_patch_slow_mem::<false>(host_pc, guest_memory_addr, fast_mem, metadata, cpu);
        }
        true
    }
}
