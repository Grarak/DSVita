use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::jit_asm::JitAsm;

impl<const CPU: CpuType> JitAsm<'_, CPU> {
    pub fn emit_thumb(&mut self, block_asm: &mut BlockAsm) {}
}
