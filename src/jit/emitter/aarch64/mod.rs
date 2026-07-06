// The aarch64 code-generation backend's emitter, laid out like emitter/arm32: emit.rs
// drives the block loop and owns the support valve, emit_alu.rs lowers data-processing
// (the flags-in-memory strategy), emit_branch.rs lowers branches (local jumps,
// tail-called externals, scheduler tails).

mod emit;
mod emit_alu;
mod emit_branch;

pub use emit::is_block_jit_supported;

use vixl::A64Reg;

// Two more block-body scratch registers on top of the assembler's (all caller-saved).
const SCRATCH3: A64Reg = A64Reg::X12;
const SCRATCH4: A64Reg = A64Reg::X13;
