// The binary instruction-trace (.ilog) format and its decoder, shared verbatim between the
// in-emulator writer/decoder (debug_inst_log.rs) and the standalone x86 decoder
// (tools/trace-decode, which `#[path]`-includes this file). Keeping the record layout, the delta
// encoding and the decode/format logic in one place means the two can never drift out of sync.
//
// It depends only on `crate::jit::{disassembler, inst_info, reg}` — no `crate::core`, no emulator
// state — so it compiles in both the emulator crate and the tiny trace-decode crate (whose jit.rs
// re-exports the real disassembler island). `cpu` is a raw u8 (0 = ARM9, 1 = ARM7) throughout to
// avoid pulling in the emulator's CpuType.

use crate::jit::disassembler::lookup_table::lookup_opcode;
use crate::jit::disassembler::thumb::lookup_table_thumb::lookup_thumb_opcode;
use crate::jit::inst_info::InstInfo;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};

#[repr(C)]
#[derive(Copy, Clone)]
pub struct InstLogRecord {
    pub regs: [u32; 15],
    pub pc: u32,
    pub cpsr: u32,
    pub spsr: u32,
    pub opcode: u32,
    pub cpu: u8,
}

pub const RECORD_SIZE: usize = size_of::<InstLogRecord>();

pub const TAG_INST: u8 = 0;
pub const TAG_TEXT: u8 = 1;
pub const TAG_INST_DELTA: u8 = 2;
pub const TAG_MEM: u8 = 3;

// Delta records (TAG_INST_DELTA) carry only what changed since the same cpu's previous record —
// consecutive instructions share almost all register state, so this shrinks traces roughly 5x. A
// full TAG_INST keyframe is emitted per cpu at the start and every KEYFRAME_INTERVAL records,
// bounding how much a corrupt byte can poison.
//
// Layout: flags u8, mask u16 (bit i = reg i changed, i < 15), then LE u32s in order: pc (unless
// FLAG_PC_SEQ), cpsr / spsr / opcode (unless their _SAME flag), changed regs ascending. FLAG_PC_SEQ
// means pc = prev pc + (4 >> new cpsr thumb bit).
pub const FLAG_CPU7: u8 = 1 << 0;
pub const FLAG_PC_SEQ: u8 = 1 << 1;
pub const FLAG_OPCODE_SAME: u8 = 1 << 2;
pub const FLAG_SPSR_SAME: u8 = 1 << 3;
pub const FLAG_CPSR_SAME: u8 = 1 << 4;
pub const KEYFRAME_INTERVAL: u32 = 1 << 20;

// Compact memory-access records (TAG_MEM). Layout: meta u8 (bits 0-3 kind, bit 4 cpu7, bits 5-6
// value width code), addr LE u32, then per kind: single kinds = the value/size in MEM_WIDTHS[code]
// bytes (code 0 = value 0, no bytes); slice kinds = element count LE u32, then count elements of
// MEM_WIDTHS[code] bytes each, per-element address re-derived from the kind's stride.
pub const MEM_FLAG_CPU7: u8 = 1 << 4;
pub const MEM_WIDTHS: [usize; 4] = [0, 1, 2, 4];

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

impl MemLogKind {
    pub fn element_stride(self, element_size: usize) -> usize {
        match self {
            MemLogKind::SliceReadValue | MemLogKind::SliceWriteValue => element_size,
            _ => 0,
        }
    }
}

#[inline]
pub fn cpu_name(cpu: u8) -> &'static str {
    if cpu & 1 == 0 {
        "ARM9"
    } else {
        "ARM7"
    }
}

#[inline]
pub fn thumb(cpsr: u32) -> bool {
    cpsr & 0x20 != 0 // CPSR T bit
}

// Encoder helper: pack the meta byte for a mem record.
pub fn mem_meta(cpu: u8, kind: MemLogKind, width_code: u8) -> u8 {
    kind as u8 | (if cpu & 1 == 1 { MEM_FLAG_CPU7 } else { 0 }) | (width_code << 5)
}

// The exact text the emulator's `debug_println!` produced for a mem access; the decoder reprints it.
pub fn format_mem_line(cpu: u8, kind: u8, addr: u32, value: u32) -> String {
    let c = cpu_name(cpu);
    match kind {
        0 => format!("{c} memory read at {addr:x}"),
        1 => format!("{c} memory read at {addr:x} with value {value:x}"),
        2 => format!("{c} memory write at {addr:x} with value {value:x}"),
        3 => format!("{c} slice memory read at {addr:x} with size {value}"),
        4 => format!("{c} fixed slice memory read at {addr:x} with size {value}"),
        5 => format!("{c} fixed slice memory write at {addr:x} with size {value}"),
        6 => format!("{c} multiple memset memory write at {addr:x} with size {value}"),
        7 => format!("{c} slice memory read at {addr:x} with value {value:x}"),
        8 => format!("{c} fixed slice memory read at {addr:x} with value {value:x}"),
        9 => format!("{c} slice memory write at {addr:x} with value {value:x}"),
        10 => format!("{c} fixed slice memory write at {addr:x} with value {value:x}"),
        _ => format!("{c} unknown mem kind {kind} at {addr:x}"),
    }
}

fn mem_is_slice(kind: u8) -> bool {
    matches!(kind, 7 | 8 | 9 | 10)
}

fn mem_stride(kind: u8, element_size: usize) -> usize {
    match kind {
        7 | 9 => element_size, // Slice* advance by the element width; Fixed* stay put
        _ => 0,
    }
}

#[inline]
fn record_reg(record: &InstLogRecord, reg: Reg) -> u32 {
    match reg {
        Reg::PC => record.pc,
        Reg::CPSR => record.cpsr,
        Reg::SPSR => record.spsr,
        _ => record.regs[reg as usize], // R0..R12 = 0..12, SP = 13, LR = 14
    }
}

// Decode filters, all no-ops for the emulator's `decode-inst-log` (which passes Default). The
// standalone tool exposes them as CLI flags.
#[derive(Default)]
pub struct Opts {
    pub cpu: Option<u8>, // only print inst records for this cpu; drops text/mem lines
    pub limit: Option<u64>,
    pub start: u64, // skip the first N inst records (1-based)
    pub index: bool, // prefix each inst line with its 1-based record number
}

// Read a whole .ilog and print the reconstructed per-instruction / text / mem lines, byte-for-byte
// as the emulator's `debug_println!` stream. Truncated tails (from a signal-killed capture) end the
// decode cleanly rather than panicking.
pub fn decode_file(path: &str, opts: &Opts) {
    let mut reader = BufReader::new(File::open(path).unwrap_or_else(|e| panic!("failed to open inst log {path}: {e}")));
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    let mut prev: [Option<InstLogRecord>; 2] = [None, None];
    let mut n_inst: u64 = 0;
    let mut shown: u64 = 0;

    let print_record = |out: &mut BufWriter<std::io::StdoutLock>, record: &InstLogRecord, n: u64| -> bool {
        if let Some(want) = opts.cpu {
            if record.cpu != want {
                return false;
            }
        }
        if n <= opts.start {
            return false;
        }
        let cname = cpu_name(record.cpu);
        let inst_info = if thumb(record.cpsr) {
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
        let prefix = if opts.index { format!("#{n} ") } else { String::new() };
        let _ = writeln!(out, "{prefix}{cname} {output}\n\t{cname} {inst_info:?}");
        true
    };

    let mut tag = [0u8; 1];
    while reader.read_exact(&mut tag).is_ok() {
        match tag[0] {
            TAG_INST => {
                let mut buf = [0u8; RECORD_SIZE];
                if reader.read_exact(&mut buf).is_err() {
                    break; // truncated tail from a killed capture
                }
                let record: InstLogRecord = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast()) };
                if record.cpu < 2 {
                    prev[record.cpu as usize] = Some(record);
                }
                n_inst += 1;
                if print_record(&mut out, &record, n_inst) {
                    shown += 1;
                    if opts.limit == Some(shown) {
                        break;
                    }
                }
            }
            TAG_INST_DELTA => {
                let mut head = [0u8; 3];
                if reader.read_exact(&mut head).is_err() {
                    break;
                }
                let flags = head[0];
                let mask = u16::from_le_bytes([head[1], head[2]]);
                let cpu_index = (flags & FLAG_CPU7 != 0) as usize;
                let Some(mut record) = prev[cpu_index] else {
                    break; // delta before keyframe: corrupt
                };

                let mut word = [0u8; 4];
                let mut truncated = false;
                let mut next_word = |reader: &mut BufReader<File>| {
                    if reader.read_exact(&mut word).is_err() {
                        truncated = true;
                        return 0;
                    }
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
                if truncated {
                    break;
                }
                record.pc = match explicit_pc {
                    Some(pc) => pc,
                    None => record.pc.wrapping_add(if thumb(record.cpsr) { 2 } else { 4 }),
                };
                prev[cpu_index] = Some(record);
                n_inst += 1;
                if print_record(&mut out, &record, n_inst) {
                    shown += 1;
                    if opts.limit == Some(shown) {
                        break;
                    }
                }
            }
            TAG_TEXT => {
                let mut len = [0u8; 4];
                if reader.read_exact(&mut len).is_err() {
                    break;
                }
                let mut text = vec![0u8; u32::from_le_bytes(len) as usize];
                let mut newline = [0u8; 1];
                if reader.read_exact(&mut text).is_err() || reader.read_exact(&mut newline).is_err() {
                    break;
                }
                if opts.cpu.is_none() {
                    let _ = out.write_all(&text);
                    if newline[0] != 0 {
                        let _ = out.write_all(b"\n");
                    }
                }
            }
            TAG_MEM => {
                let mut head = [0u8; 5];
                if reader.read_exact(&mut head).is_err() {
                    break;
                }
                let kind = head[0] & 0xF;
                let cpu = (head[0] >> 4) & 1;
                let width = MEM_WIDTHS[((head[0] >> 5) & 3) as usize];
                let addr = u32::from_le_bytes([head[1], head[2], head[3], head[4]]);
                let next_value = |reader: &mut BufReader<File>| -> Option<u32> {
                    let mut buf = [0u8; 4];
                    reader.read_exact(&mut buf[..width]).ok()?;
                    Some(u32::from_le_bytes(buf))
                };
                let emit = opts.cpu.is_none();
                if mem_is_slice(kind) {
                    let mut count = [0u8; 4];
                    if reader.read_exact(&mut count).is_err() {
                        break;
                    }
                    let stride = mem_stride(kind, width) as u32;
                    for i in 0..u32::from_le_bytes(count) {
                        let Some(value) = next_value(&mut reader) else {
                            break;
                        };
                        if emit {
                            let _ = writeln!(out, "{}", format_mem_line(cpu, kind, addr.wrapping_add(i.wrapping_mul(stride)), value));
                        }
                    }
                } else {
                    let value = if width == 0 {
                        0
                    } else {
                        match next_value(&mut reader) {
                            Some(v) => v,
                            None => break,
                        }
                    };
                    if emit {
                        let _ = writeln!(out, "{}", format_mem_line(cpu, kind, addr, value));
                    }
                }
            }
            _ => break, // unknown tag: corrupt tail, treat as EOF
        }
    }
    let _ = out.flush();
}
