use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::inst_exception_handler::bios_uninterrupt;
use crate::jit::jit_asm::JitAsm;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_unknown(&mut self, block_asm: &mut BlockAsm) {
        // 0xEC000000 magic number to call finish bios hle interrupt
        if self.jit_buf.current_inst().opcode == 0xEC000000 {
            block_asm.save_context();
            block_asm.call1(bios_uninterrupt::<CPU> as _, self as *mut _ as u32);
            self.emit_branch_out_metadata(block_asm);
            block_asm.breakout();
        }
    }
}
