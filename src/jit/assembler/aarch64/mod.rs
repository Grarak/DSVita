// The aarch64 assembler backend, laid out like assembler/arm32: block_asm.rs owns
// A64BlockAsm (frame contract, entry-pc dispatch stub, guest-register access, the
// emitted exit-guest-context twin) and the block-body register conventions.

mod block_asm;

pub use block_asm::*;
