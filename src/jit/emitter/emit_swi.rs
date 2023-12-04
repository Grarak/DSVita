use crate::hle::exception_handler::{exception_handler, ExceptionVector};
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Cond;

impl JitAsm {
    pub fn emit_swi(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        if inst_info.cond != Cond::AL {
            todo!()
        }

        let cp15_context_addr = self.cp15_context.as_ptr() as u32;
        let regs_addr = self.thread_regs.as_ptr() as u32;
        let exception_addr = exception_handler as *const () as u32;

        self.emit_call_host_func(
            |asm| {
                asm.jit_buf.emit_opcodes.push(
                    AluImm::mov_al(Reg::R3, ExceptionVector::SoftwareInterrupt as u8)
                );
            },
            &[
                Some(cp15_context_addr),
                Some(regs_addr),
                Some(inst_info.opcode),
                None,
            ],
            exception_addr,
        );
    }
}
