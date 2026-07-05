use crate::core::emu::Emu;
use crate::core::thread_regs::Cpsr;
use crate::core::CpuType;
use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::InstInfo;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use std::cell::UnsafeCell;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};

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

struct Logger {
    writer: UnsafeCell<Option<BufWriter<File>>>,
}

unsafe impl Sync for Logger {}

static LOGGER: Logger = Logger { writer: UnsafeCell::new(None) };

pub fn init(path: &str) {
    let file = File::create(path).unwrap_or_else(|e| panic!("failed to create inst log {path}: {e}"));
    unsafe { *LOGGER.writer.get() = Some(BufWriter::with_capacity(1 << 20, file)) };

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

    let mut regs = [0u32; 15];
    for (i, r) in regs.iter_mut().enumerate() {
        *r = *emu.thread_get_reg(cpu, Reg::from(i as u8));
    }
    let record = InstLogRecord {
        regs,
        pc,
        cpsr: *emu.thread_get_reg(cpu, Reg::CPSR),
        spsr: *emu.thread_get_reg(cpu, Reg::SPSR),
        opcode,
        cpu: cpu as u8,
    };

    let bytes = unsafe { std::slice::from_raw_parts((&record as *const InstLogRecord).cast::<u8>(), RECORD_SIZE) };
    let _ = writer.write_all(&[TAG_INST]);
    let _ = writer.write_all(bytes);
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
    let Some(writer) = lazy_writer() else { return };

    let _ = writer.write_all(&[TAG_TEXT]);
    let _ = writer.write_all(&(s.len() as u32).to_le_bytes());
    let _ = writer.write_all(s.as_bytes());
    let _ = writer.write_all(&[newline as u8]);
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

    let mut tag = [0u8; 1];
    while reader.read_exact(&mut tag).is_ok() {
        match tag[0] {
            TAG_INST => {
                let mut buf = [0u8; RECORD_SIZE];
                reader.read_exact(&mut buf).expect("truncated inst record");
                let record: InstLogRecord = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast()) };
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
                    output += &format!("{reg:?}: {:x}, ", record_reg(&record, reg));
                }
                let _ = writeln!(out, "{cpu:?} {output}\n\t{cpu:?} {inst_info:?}");
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
            other => panic!("unknown inst log record tag {other}"),
        }
    }
}
