use crate::core::emu::Emu;
use crate::core::thread_regs::Cpsr;
use crate::core::CpuType;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::InstInfo;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::utils::Convert;
use std::cell::UnsafeCell;
use std::fs::File;
use std::hint::assert_unchecked;
use std::io::{BufReader, BufWriter, Read, Write};
use std::mem;

#[repr(C)]
#[derive(Copy, Clone)]
struct InstLogRecord {
    regs: [u32; 15],
    pc: u32,
    cpsr: u32,
    spsr: u32,
    opcode: u32,
    cpu: u8,
}

const RECORD_SIZE: usize = size_of::<InstLogRecord>();

const TAG_INST: u8 = 0;
const TAG_TEXT: u8 = 1;
const TAG_INST_DELTA: u8 = 2;
const TAG_MEM: u8 = 3;

// Delta records (TAG_INST_DELTA) carry only what changed since the same cpu's previous
// record — consecutive instructions share almost all register state, so this shrinks traces
// roughly 5x. A full TAG_INST keyframe is emitted per cpu at the start and every
// KEYFRAME_INTERVAL records, bounding how much a corrupt byte can poison.
//
// Layout: flags u8, mask u16 (bit i = reg i changed, i < 15), then LE u32s in order:
// pc (unless FLAG_PC_SEQ), cpsr / spsr / opcode (unless their _SAME flag), changed regs
// ascending. FLAG_PC_SEQ means pc = prev pc + (4 >> new cpsr thumb bit).
const FLAG_CPU7: u8 = 1 << 0;
const FLAG_PC_SEQ: u8 = 1 << 1;
const FLAG_OPCODE_SAME: u8 = 1 << 2;
const FLAG_SPSR_SAME: u8 = 1 << 3;
const FLAG_CPSR_SAME: u8 = 1 << 4;
const KEYFRAME_INTERVAL: u32 = 1 << 20;

#[derive(Copy, Clone, Default)]
struct PrevState {
    regs: [u32; 15],
    pc: u32,
    cpsr: u32,
    spsr: u32,
    opcode: u32,
    valid: bool,
    since_keyframe: u32,
}

// Per-cpu previous record for delta encoding; only touched from the cpu thread.
struct PrevStates(UnsafeCell<[PrevState; 2]>);

unsafe impl Sync for PrevStates {}

impl PrevState {
    const fn default_const() -> Self {
        PrevState {
            regs: [0; 15],
            pc: 0,
            cpsr: 0,
            spsr: 0,
            opcode: 0,
            valid: false,
            since_keyframe: 0,
        }
    }
}

struct Logger {
    writer: UnsafeCell<Option<BufWriter<File>>>,
}

unsafe impl Sync for Logger {}

static LOGGER: Logger = Logger { writer: UnsafeCell::new(None) };

// Optional record budget (DSVITA_INST_LOG_MAX): after this many instruction records the log
// is flushed and closed from the logging thread itself — a deterministic, race-free stop for
// A/B captures (a SIGINT flush can tear a record mid-write and corrupt the stream framing).
// 0 = unlimited. The remaining count is only touched from the cpu thread.
struct RecordBudget(UnsafeCell<u64>);

unsafe impl Sync for RecordBudget {}

static RECORD_BUDGET: RecordBudget = RecordBudget(UnsafeCell::new(0));

// env reads must never happen on the vita; the whole capture configuration is linux-only
// (inst logs are only reachable through the linux cli anyway).
#[cfg(target_os = "linux")]
fn init_budget() {
    let max = std::env::var("DSVITA_INST_LOG_MAX").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    unsafe { *RECORD_BUDGET.0.get() = max };
    // DSVITA_INST_LOG_TEXT=0 drops the interleaved text records (debug_println/branch_println
    // lines). Instruction-only logs are what the strict differ compares, at a fraction of the
    // size — a commercial boot's text records outweigh the instruction records.
    let text = std::env::var("DSVITA_INST_LOG_TEXT").map(|v| v != "0").unwrap_or(true);
    TEXT_ENABLED.store(text, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(not(target_os = "linux"))]
fn init_budget() {}

static TEXT_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn init(path: &str) {
    let file = File::create(path).unwrap_or_else(|e| panic!("failed to create inst log {path}: {e}"));
    unsafe { *LOGGER.writer.get() = Some(BufWriter::with_capacity(1 << 20, file)) };
    init_budget();

    #[cfg(target_os = "linux")]
    unsafe {
        libc::signal(libc::SIGINT, sigint_handler as libc::sighandler_t)
    };
}

// Lazy variant: logging starts only once SIGUSR2 arrives (`kill -USR2 <pid>`), so a trace can be
// limited to the interesting final stretch of a long run instead of gigabytes of boot.
struct LazyPath(UnsafeCell<Option<String>>);

// SAFETY: written once during startup before the cpu thread exists.
unsafe impl Sync for LazyPath {}

static LAZY_PATH: LazyPath = LazyPath(UnsafeCell::new(None));
static LAZY_ARMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn init_lazy(path: &str) {
    unsafe { *LAZY_PATH.0.get() = Some(path.to_string()) };
    init_budget();

    #[cfg(target_os = "linux")]
    unsafe {
        libc::signal(libc::SIGINT, sigint_handler as libc::sighandler_t);
        libc::signal(libc::SIGUSR2, sigusr2_handler as libc::sighandler_t);
    };
}

#[cfg(target_os = "linux")]
extern "C" fn sigusr2_handler(_sig: i32) {
    // Only flag here (signal-safe); the logging sites open the file lazily.
    LAZY_ARMED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Open the lazy log once SIGUSR2 armed it. Called from the logging sites (cpu thread), not the
/// signal handler, so file creation is safe.
#[inline]
fn lazy_writer() -> Option<&'static mut BufWriter<File>> {
    let writer = unsafe { &mut *LOGGER.writer.get() };
    if writer.is_none() {
        if !LAZY_ARMED.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        let path = unsafe { (*LAZY_PATH.0.get()).as_ref()? };
        let file = File::create(path).unwrap_or_else(|e| panic!("failed to create inst log {path}: {e}"));
        *writer = Some(BufWriter::with_capacity(1 << 20, file));
    }
    writer.as_mut()
}

#[cfg(target_os = "linux")]
extern "C" fn sigint_handler(_sig: i32) {
    flush();
    // 128 + SIGINT, the conventional exit status for a ctrl-c termination.
    unsafe { libc::_exit(130) };
}

/// Append the post-execution state of the instruction at `pc` (opcode `opcode`) to the log.
#[inline]
pub fn log(emu: &Emu, cpu: CpuType, pc: u32, opcode: u32) {
    let Some(writer) = lazy_writer() else { return };

    let budget = unsafe { &mut *RECORD_BUDGET.0.get() };
    if *budget != 0 {
        *budget -= 1;
        if *budget == 0 {
            // Budget exhausted: this record is the last one. Flush and close from here (the
            // logging thread) so the file ends on a clean record boundary.
            write_record(writer, emu, cpu, pc, opcode);
            let _ = writer.flush();
            unsafe { *LOGGER.writer.get() = None };
            unsafe { *LAZY_PATH.0.get() = None };
            println!("inst log record budget exhausted, log closed");
            // Memory-state fingerprints at the deterministic stop point: register traces
            // can't see wrong STOREs (a bad str/stm corrupts vram/main invisibly until
            // something reads it back) — cross-engine runs compare these hashes to catch
            // exactly that class.
            {
                use xxhash_rust::xxh32::xxh32;
                let palettes = emu.mem_get_palettes();
                let oam = emu.mem_get_oam();
                let main = unsafe {
                    std::slice::from_raw_parts(
                        emu.mem.shm.as_ptr().add(crate::core::memory::regions::MAIN_REGION.shm_offset),
                        crate::core::memory::regions::MAIN_REGION.size,
                    )
                };
                let vram_hash = xxh32(emu.mem.vram.banks.mem.as_slice(), 0);
                eprintln!("MEMHASH main={:08x} vram={vram_hash:08x} palettes={:08x} oam={:08x}", xxh32(main, 0), xxh32(palettes, 0), xxh32(oam, 0));
            }
            return;
        }
    }
    write_record(writer, emu, cpu, pc, opcode);
}

#[inline]
fn write_record(writer: &mut BufWriter<File>, emu: &Emu, cpu: CpuType, pc: u32, opcode: u32) {
    let mut regs = [0u32; 15];
    for (i, r) in regs.iter_mut().enumerate() {
        *r = *emu.thread_get_reg(cpu, Reg::from(i as u8));
    }
    let cpsr = *emu.thread_get_reg(cpu, Reg::CPSR);
    let spsr = *emu.thread_get_reg(cpu, Reg::SPSR);

    static PREV_STATES: PrevStates = PrevStates(UnsafeCell::new([PrevState::default_const(), PrevState::default_const()]));
    let prev = unsafe { &mut (*PREV_STATES.0.get())[cpu as usize] };
    if !prev.valid || prev.since_keyframe >= KEYFRAME_INTERVAL {
        let record = InstLogRecord {
            regs,
            pc,
            cpsr,
            spsr,
            opcode,
            cpu: cpu as u8,
        };
        let bytes: &[u8; RECORD_SIZE] = unsafe { mem::transmute(&record) };
        let _ = writer.write_all(&[TAG_INST]);
        let _ = writer.write_all(bytes);
        prev.since_keyframe = 0;
    } else {
        // Delta record: flags + changed-reg mask, then only the changed words.
        let mut flags = if cpu == CpuType::ARM7 { FLAG_CPU7 } else { 0 };
        // payload: [pc][cpsr][spsr][opcode][regs...] — worst case 19 words.
        let mut payload = [0u32; 19];
        let mut n = 0;
        if pc == prev.pc.wrapping_add(if Cpsr::from(cpsr).thumb() { 2 } else { 4 }) {
            flags |= FLAG_PC_SEQ;
        } else {
            payload[n] = pc;
            n += 1;
        }
        if cpsr == prev.cpsr {
            flags |= FLAG_CPSR_SAME;
        } else {
            payload[n] = cpsr;
            n += 1;
        }
        if spsr == prev.spsr {
            flags |= FLAG_SPSR_SAME;
        } else {
            payload[n] = spsr;
            n += 1;
        }
        if opcode == prev.opcode {
            flags |= FLAG_OPCODE_SAME;
        } else {
            payload[n] = opcode;
            n += 1;
        }
        let mut mask = 0u16;
        for i in 0..15 {
            if regs[i] != prev.regs[i] {
                mask |= 1 << i;
                payload[n] = regs[i];
                n += 1;
            }
        }
        unsafe { assert_unchecked(n <= payload.len()) };
        let _ = writer.write_all(&[TAG_INST_DELTA, flags, (mask & 0xFF) as u8, (mask >> 8) as u8]);
        let payload_bytes = unsafe { std::slice::from_raw_parts(payload.as_ptr().cast::<u8>(), n * 4) };
        let _ = writer.write_all(payload_bytes);
        prev.since_keyframe += 1;
    }

    prev.regs = regs;
    prev.pc = pc;
    prev.cpsr = cpsr;
    prev.spsr = spsr;
    prev.opcode = opcode;
    prev.valid = true;
}

#[inline]
pub fn is_logging() -> bool {
    unsafe { (*LOGGER.writer.get()).is_some() }
}

/// Append a text line (`branch_println!` / `debug_println!` / `block_asm_println!`) to the log,
/// interleaved with the instruction records so control flow can be diffed alongside the register
/// snapshots. `s` is the already-formatted line; a newline is added back on decode.
#[inline]
pub fn log_text(s: &str) {
    write_text(s, true);
}

/// Like [`log_text`] but without a trailing newline (mirrors `print!` vs `println!`), so a partial
/// line emitted across several calls reassembles into one line on decode.
// Only reached through `block_asm_print!`, which currently has no call sites.
#[allow(dead_code)]
#[inline]
pub fn log_text_no_newline(s: &str) {
    write_text(s, false);
}

#[inline]
fn write_text(s: &str, newline: bool) {
    if !TEXT_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let Some(writer) = lazy_writer() else { return };

    let _ = writer.write_all(&[TAG_TEXT]);
    let _ = writer.write_all(&(s.len() as u32).to_le_bytes());
    let _ = writer.write_all(s.as_bytes());
    let _ = writer.write_all(&[newline as u8]);
}

// Compact memory-access records (TAG_MEM) replace the extremely frequent "memory read/write at"
// debug_println! lines from mem.rs — the decoder regenerates the exact same text at a fraction of
// the serialized size. Layout: meta u8 (bits 0-3 kind, bit 4 cpu7, bits 5-6 value width code),
// addr LE u32, then per kind:
// - single kinds: the value/size in MEM_WIDTHS[code] bytes (code 0 = value 0, no bytes),
// - slice kinds: element count LE u32, then count elements of MEM_WIDTHS[code] bytes each; the
//   per-element address is re-derived from the kind's stride (Slice* = element width, Fixed* = 0).
#[derive(Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum MemLogKind {
    Read = 0,
    ReadValue = 1,
    WriteValue = 2,
    SliceReadSize = 3,
    FixedReadSize = 4,
    FixedWriteSize = 5,
    MemsetWriteSize = 6,
    SliceReadValue = 7,
    FixedReadValue = 8,
    SliceWriteValue = 9,
    FixedWriteValue = 10,
}

const MEM_FLAG_CPU7: u8 = 1 << 4;
const MEM_WIDTHS: [usize; 4] = [0, 1, 2, 4];

impl MemLogKind {
    fn from_u8(value: u8) -> Self {
        if value > MemLogKind::FixedWriteValue as u8 {
            panic!("unknown mem log record kind {value}");
        }
        unsafe { mem::transmute(value) }
    }

    fn is_slice(self) -> bool {
        matches!(
            self,
            MemLogKind::SliceReadValue | MemLogKind::FixedReadValue | MemLogKind::SliceWriteValue | MemLogKind::FixedWriteValue
        )
    }

    fn element_stride(self, element_size: usize) -> usize {
        match self {
            MemLogKind::SliceReadValue | MemLogKind::SliceWriteValue => element_size,
            _ => 0,
        }
    }
}

fn format_mem_line(cpu: CpuType, kind: MemLogKind, addr: u32, value: u32) -> String {
    match kind {
        MemLogKind::Read => format!("{cpu:?} memory read at {addr:x}"),
        MemLogKind::ReadValue => format!("{cpu:?} memory read at {addr:x} with value {value:x}"),
        MemLogKind::WriteValue => format!("{cpu:?} memory write at {addr:x} with value {value:x}"),
        MemLogKind::SliceReadSize => format!("{cpu:?} slice memory read at {addr:x} with size {value}"),
        MemLogKind::FixedReadSize => format!("{cpu:?} fixed slice memory read at {addr:x} with size {value}"),
        MemLogKind::FixedWriteSize => format!("{cpu:?} fixed slice memory write at {addr:x} with size {value}"),
        MemLogKind::MemsetWriteSize => format!("{cpu:?} multiple memset memory write at {addr:x} with size {value}"),
        MemLogKind::SliceReadValue => format!("{cpu:?} slice memory read at {addr:x} with value {value:x}"),
        MemLogKind::FixedReadValue => format!("{cpu:?} fixed slice memory read at {addr:x} with value {value:x}"),
        MemLogKind::SliceWriteValue => format!("{cpu:?} slice memory write at {addr:x} with value {value:x}"),
        MemLogKind::FixedWriteValue => format!("{cpu:?} fixed slice memory write at {addr:x} with value {value:x}"),
    }
}

fn mem_meta(cpu: CpuType, kind: MemLogKind, width_code: u8) -> u8 {
    kind as u8 | (if cpu == CpuType::ARM7 { MEM_FLAG_CPU7 } else { 0 }) | (width_code << 5)
}

/// Append a single memory-access record (`kind` must not be a slice kind; `value` is the value or
/// size the kind's suffix prints, 0 for [`MemLogKind::Read`]). Falls back to the same stdout line
/// `debug_println!` produced when no inst log is active, so this can replace those call sites 1:1.
#[inline]
pub fn log_mem(cpu: CpuType, kind: MemLogKind, addr: u32, value: u32) {
    if !crate::DEBUG_LOG {
        return;
    }
    if !is_logging() {
        let current_thread = std::thread::current();
        println!("[{}] {}", current_thread.name().unwrap(), format_mem_line(cpu, kind, addr, value));
        return;
    }
    if !TEXT_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let Some(writer) = lazy_writer() else { return };

    let width_code = if value == 0 {
        0
    } else if value <= 0xFF {
        1
    } else if value <= 0xFFFF {
        2
    } else {
        3
    };
    let mut buf = [0u8; 10];
    buf[0] = TAG_MEM;
    buf[1] = mem_meta(cpu, kind, width_code);
    buf[2..6].copy_from_slice(&addr.to_le_bytes());
    let n = MEM_WIDTHS[width_code as usize];
    buf[6..6 + n].copy_from_slice(&value.to_le_bytes()[..n]);
    let _ = writer.write_all(&buf[..6 + n]);
}

/// Append the per-element value lines of a slice access as one batched record (`kind` must be a
/// slice kind). `addr` is the first element's address; the decoder advances it by the kind's
/// stride per element and reprints the exact per-element `debug_println!` lines.
#[inline]
pub fn log_mem_slice<T: Convert>(cpu: CpuType, kind: MemLogKind, addr: u32, values: &[T]) {
    if !crate::DEBUG_LOG {
        return;
    }
    if !is_logging() {
        let current_thread = std::thread::current();
        let name = current_thread.name().unwrap();
        let stride = kind.element_stride(size_of::<T>());
        for (i, &value) in values.iter().enumerate() {
            println!("[{name}] {}", format_mem_line(cpu, kind, addr.wrapping_add((i * stride) as u32), value.into()));
        }
        return;
    }
    if !TEXT_ENABLED.load(std::sync::atomic::Ordering::Relaxed) || values.is_empty() {
        return;
    }
    let Some(writer) = lazy_writer() else { return };

    let width_code = match size_of::<T>() {
        1 => 1,
        2 => 2,
        _ => 3,
    };
    let mut head = [0u8; 10];
    head[0] = TAG_MEM;
    head[1] = mem_meta(cpu, kind, width_code);
    head[2..6].copy_from_slice(&addr.to_le_bytes());
    head[6..10].copy_from_slice(&(values.len() as u32).to_le_bytes());
    let _ = writer.write_all(&head);
    let payload = unsafe { std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), size_of_val(values)) };
    let _ = writer.write_all(payload);
}

pub fn flush() {
    if let Some(writer) = unsafe { (*LOGGER.writer.get()).as_mut() } {
        let _ = writer.flush();
    }
}

#[inline]
fn record_reg(record: &InstLogRecord, reg: Reg) -> u32 {
    match reg {
        Reg::PC => record.pc,
        Reg::CPSR => record.cpsr,
        Reg::SPSR => record.spsr,
        // R0..R12 = 0..12, SP = 13, LR = 14
        _ => record.regs[reg as usize],
    }
}

pub fn decode_file(path: &str) {
    let mut reader = BufReader::new(File::open(path).unwrap_or_else(|e| panic!("failed to open inst log {path}: {e}")));
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    // Delta decode state: the last full record seen per cpu.
    let mut prev: [Option<InstLogRecord>; 2] = [None, None];

    let print_record = |out: &mut BufWriter<std::io::StdoutLock>, record: &InstLogRecord| {
        let cpu = CpuType::from(record.cpu);
        let cpsr = Cpsr::from(record.cpsr);
        let inst_info = if cpsr.thumb() {
            let (op, func) = lookup_thumb_opcode(record.opcode as u16);
            InstInfo::from(func(record.opcode as u16, *op))
        } else {
            let (op, func) = lookup_opcode(record.opcode);
            func(record.opcode, *op)
        };

        let mut output = "Executed ".to_owned();
        for reg in reg_reserve!(Reg::SP, Reg::LR, Reg::PC, Reg::CPSR, Reg::SPSR) + RegReserve::gp() {
            output += &format!("{reg:?}: {:x}, ", record_reg(record, reg));
        }
        let _ = writeln!(out, "{cpu:?} {output}\n\t{cpu:?} {inst_info:?}");
    };

    let mut tag = [0u8; 1];
    while reader.read_exact(&mut tag).is_ok() {
        match tag[0] {
            TAG_INST => {
                let mut buf = [0u8; RECORD_SIZE];
                reader.read_exact(&mut buf).expect("truncated inst record");
                let record: InstLogRecord = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast()) };
                print_record(&mut out, &record);
                prev[record.cpu as usize] = Some(record);
            }
            TAG_INST_DELTA => {
                let mut head = [0u8; 3];
                reader.read_exact(&mut head).expect("truncated delta record header");
                let flags = head[0];
                let mask = u16::from_le_bytes([head[1], head[2]]);
                let cpu_index = (flags & FLAG_CPU7 != 0) as usize;
                let mut record = prev[cpu_index].expect("delta record before keyframe");

                let mut word = [0u8; 4];
                let mut next_word = |reader: &mut BufReader<File>| {
                    reader.read_exact(&mut word).expect("truncated delta record payload");
                    u32::from_le_bytes(word)
                };
                let explicit_pc = if flags & FLAG_PC_SEQ == 0 { Some(next_word(&mut reader)) } else { None };
                if flags & FLAG_CPSR_SAME == 0 {
                    record.cpsr = next_word(&mut reader);
                }
                if flags & FLAG_SPSR_SAME == 0 {
                    record.spsr = next_word(&mut reader);
                }
                if flags & FLAG_OPCODE_SAME == 0 {
                    record.opcode = next_word(&mut reader);
                }
                for i in 0..15 {
                    if mask & (1 << i) != 0 {
                        record.regs[i] = next_word(&mut reader);
                    }
                }
                record.pc = match explicit_pc {
                    Some(pc) => pc,
                    None => record.pc.wrapping_add(if Cpsr::from(record.cpsr).thumb() { 2 } else { 4 }),
                };
                print_record(&mut out, &record);
                prev[cpu_index] = Some(record);
            }
            TAG_TEXT => {
                let mut len = [0u8; 4];
                reader.read_exact(&mut len).expect("truncated text record length");
                let mut text = vec![0u8; u32::from_le_bytes(len) as usize];
                reader.read_exact(&mut text).expect("truncated text record");
                let mut newline = [0u8; 1];
                reader.read_exact(&mut newline).expect("truncated text record newline flag");
                let _ = out.write_all(&text);
                if newline[0] != 0 {
                    let _ = out.write_all(b"\n");
                }
            }
            TAG_MEM => {
                let mut head = [0u8; 5];
                reader.read_exact(&mut head).expect("truncated mem record header");
                let kind = MemLogKind::from_u8(head[0] & 0xF);
                let cpu = CpuType::from((head[0] >> 4) & 1);
                let width = MEM_WIDTHS[((head[0] >> 5) & 3) as usize];
                let addr = u32::from_le_bytes([head[1], head[2], head[3], head[4]]);
                let next_value = |reader: &mut BufReader<File>| {
                    let mut buf = [0u8; 4];
                    reader.read_exact(&mut buf[..width]).expect("truncated mem record value");
                    u32::from_le_bytes(buf)
                };
                if kind.is_slice() {
                    let mut count = [0u8; 4];
                    reader.read_exact(&mut count).expect("truncated mem record count");
                    let stride = kind.element_stride(width) as u32;
                    for i in 0..u32::from_le_bytes(count) {
                        let value = next_value(&mut reader);
                        let _ = writeln!(out, "{}", format_mem_line(cpu, kind, addr.wrapping_add(i.wrapping_mul(stride)), value));
                    }
                } else {
                    let value = if width == 0 { 0 } else { next_value(&mut reader) };
                    let _ = writeln!(out, "{}", format_mem_line(cpu, kind, addr, value));
                }
            }
            other => panic!("unknown inst log record tag {other}"),
        }
    }
}
