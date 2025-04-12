use crate::core::CpuType;
use crate::jit::analyzer::asm_analyzer::AsmAnalyzer;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::jit_asm::JitAsm;

impl<const CPU: CpuType> JitAsm<'_, CPU> {
    pub fn emit_thumb(&mut self, asm_analyzer: &AsmAnalyzer, block_asm: &mut BlockAsm) {}
}
