use crate::jit::jit_asm::JitAsm;

/// Executes a single instruction the interpreter tables mark as Fallback (swi, mcr/mrc, swp,
/// dsp muls, ...) on hosts without a jit backend. Stage-1 stub; filled in with the portable
/// handler bodies next.
pub fn interpret_fallback(_asm: &mut JitAsm, guest_pc: u32, thumb: bool) {
    todo!("interpreter fallback op at {guest_pc:x} thumb {thumb}")
}
