use crate::emu::CpuType;
use crate::jit::jit_asm::JitAsm;
use crate::jit::MemoryAmount;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_ldr_thumb(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        self.emit_single_transfer::<true, false>(buf_index, pc, true, false, MemoryAmount::from(op));
    }

    pub fn emit_str_thumb(&mut self, buf_index: usize, pc: u32) {
        let op = self.jit_buf.instructions[buf_index].op;
        self.emit_single_transfer::<true, true>(buf_index, pc, true, false, MemoryAmount::from(op));
    }
}
