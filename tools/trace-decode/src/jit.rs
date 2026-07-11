// Slim mirror of dsvita's `crate::jit` module — ONLY the disassembler island, with the leaf
// modules pointed at the real dsvita source files. Because `disassembler` points at the real
// disassembler/mod.rs, that whole subtree (alu/branch/transfer/delegations + thumb/**) resolves
// itself relative to the real file's directory, so the only things authored here are the module
// wiring and the three jit-root items the closure references (`Cond`, `Op`, `ShiftType`).
//
// The `use crate::jit::…` paths inside the included files resolve against THIS tree, so nothing
// reaches into the emulator/JIT/native code. The real files carry code the decoder never calls
// (register-allocation helpers, etc.); the allow(dead_code) below keeps that quiet.
#![allow(dead_code)]

// Private-`use` re-export, exactly as dsvita's jit/mod.rs does it: descendant modules
// (inst_info, disassembler, …) reference `crate::jit::Op`.
pub use op::Op;

use std::marker::ConstParamTy;

#[path = "../../../src/jit/op.rs"]
pub mod op;
#[path = "../../../src/jit/reg.rs"]
pub mod reg;
#[path = "../../../src/jit/inst_info.rs"]
pub mod inst_info;
#[path = "../../../src/jit/inst_info_thumb.rs"]
pub mod inst_info_thumb;
#[path = "../../../src/jit/disassembler/mod.rs"]
pub mod disassembler;

pub type Cond = vixl::Cond;

// Copied verbatim from src/jit/mod.rs — the disassembler builds shift operands from these
// discriminants and const-generic-parameterises handlers on them.
#[repr(u8)]
#[derive(Copy, Clone, ConstParamTy, Debug, PartialEq, Eq)]
pub enum ShiftType {
    Lsl = 0,
    Lsr = 1,
    Asr = 2,
    Ror = 3,
}
