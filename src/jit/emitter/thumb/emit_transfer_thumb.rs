use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::jit_asm::JitAsm;
use crate::jit::MemoryAmount;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_ldr_thumb(&mut self, block_asm: &mut BlockAsm) {
        self.emit_single_transfer::<false>(block_asm, true, false, MemoryAmount::from(self.jit_buf.current_inst().op), true);
    }

    pub fn emit_str_thumb(&mut self, block_asm: &mut BlockAsm) {
        self.emit_single_transfer::<true>(block_asm, true, false, MemoryAmount::from(self.jit_buf.current_inst().op), true);
    }
}
