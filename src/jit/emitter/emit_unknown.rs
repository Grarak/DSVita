use crate::hle::CpuType;
use crate::jit::jit_asm::JitAsm;

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit_unknown(&mut self, buf_index: usize, _: u32) {
        let opcode = self.jit_buf.instructions[buf_index].opcode;
        if (opcode & 0xE000000) == 0xA000000 {
            todo!()
        }

        if opcode == 0xFF000000 {
            todo!();
        }
    }
}
