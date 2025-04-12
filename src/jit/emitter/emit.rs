use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::jit_asm::JitAsm;

impl<const CPU: CpuType> JitAsm<'_, CPU> {
    pub fn emit(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();
        match inst_info.op {
            op if op.is_alu() => self.emit_alu(block_asm),
            _ => {}
        }
    }
}
