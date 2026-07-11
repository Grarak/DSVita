// The aarch64 emitter, laid out like emitter/arm32: emit.rs
// drives the block loop and owns the support valve, emit_alu.rs lowers data-processing
// (the flags-in-memory strategy), emit_branch.rs lowers branches (local jumps,
// tail-called externals, scheduler tails).

pub(super) mod emit;
#[cfg(target_arch = "aarch64")]
mod emit_alu;
mod emit_branch;
mod emit_transfer;
mod thumb;

use vixl::A64Reg;

// Two more block-body scratch registers on top of the assembler's (all caller-saved).
const SCRATCH3: A64Reg = A64Reg::X12;
const SCRATCH4: A64Reg = A64Reg::X13;
