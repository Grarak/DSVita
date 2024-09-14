use crate::core::exception_handler::ExceptionVector;
use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::inst_exception_handler::exception_handler;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_swi<const THUMB: bool>(&mut self, block_asm: &mut BlockAsm) {
        block_asm.save_context();
        block_asm.call3(
            exception_handler::<CPU, THUMB> as *const (),
            self.jit_buf.current_inst().opcode,
            ExceptionVector::SoftwareInterrupt as u32,
            self.jit_buf.current_pc,
        );
        block_asm.restore_reg(Reg::R0);
        block_asm.restore_reg(Reg::R1);
        block_asm.restore_reg(Reg::R2);
        block_asm.restore_reg(Reg::R3);
        block_asm.restore_reg(Reg::R12);
        block_asm.restore_reg(Reg::CPSR);
    }
}
