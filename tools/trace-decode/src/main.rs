// Standalone x86 decoder for dsvita binary instruction traces (.ilog).
//
// dsvita's own decoder is compiled into the emulator, which only builds for arm/aarch64 — so a
// trace pulled to the x86 dev box can't be decoded there. This binary shares dsvita's REAL
// disassembler (via the #[path] includes in jit.rs) AND its record format + decoder (via
// inst_log_format, the exact file the emulator's debug_inst_log.rs writes/decodes with) — so its
// output matches `decode-inst-log` exactly, while linking no C/C++ and no emulator.
//
// Usage: trace_decode <trace.ilog> [--cpu N] [--limit N] [--start N] [--index]
//   --cpu N    only print inst records for cpu N (0=ARM9, 1=ARM7); text/mem lines are dropped
//   --limit N  stop after N printed inst records
//   --start N  skip the first N inst records (1-based) before printing
//   --index    prefix each inst line with its 1-based record number (matches trace_diff pairs)

#![feature(adt_const_params)]
#![feature(generic_const_exprs)]
#![allow(incomplete_features)]

mod jit;

// The shared format + decoder. Its `crate::jit::…` paths resolve against the jit.rs island above.
#[path = "../../../src/inst_log_format.rs"]
mod inst_log_format;

use inst_log_format::{decode_file, Opts};
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args[1] == "-h" || args[1] == "--help" {
        eprintln!("Usage: trace_decode <trace.ilog> [--cpu N] [--limit N] [--start N] [--index]");
        exit(2);
    }
    let path = args[1].clone();
    let mut opts = Opts::default();
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--cpu" => {
                opts.cpu = Some(args[i + 1].parse().expect("--cpu N"));
                i += 2;
            }
            "--limit" => {
                opts.limit = Some(args[i + 1].parse().expect("--limit N"));
                i += 2;
            }
            "--start" => {
                opts.start = args[i + 1].parse().expect("--start N");
                i += 2;
            }
            "--index" => {
                opts.index = true;
                i += 1;
            }
            other => {
                eprintln!("unknown arg: {other}");
                exit(2);
            }
        }
    }
    decode_file(&path, &opts);
}
