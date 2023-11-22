use crate::jit::jit::JitAsm;

impl JitAsm {
    pub fn emit_blx(&mut self, buf_index: usize, pc: u32) -> bool {
        false
    }
}
