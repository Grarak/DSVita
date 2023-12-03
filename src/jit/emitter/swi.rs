use crate::hle::exception_handler;
use crate::hle::exception_handler::ExceptionVector;
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::jit::JitAsm;
use crate::jit::reg::Reg;
use std::ops::{Deref, DerefMut};

impl JitAsm {
    pub fn emit_swi(&mut self, buf_index: usize, _: u32) {
        let cp15_context_addr = self.cp15_context.borrow().deref() as *const _ as u32;
        let regs_addr = self.thread_regs.borrow_mut().deref_mut() as *mut _ as u32;
        let exception_addr = exception_handler::exception as *const () as u32;

        self.emit_call_host_func(
            |asm| {
                asm.jit_buf
                    .push(AluImm::mov_al(
                        Reg::R3,
                        ExceptionVector::SoftwareInterrupt as u8,
                    ));
            },
            &[
                Some(cp15_context_addr),
                Some(regs_addr),
                Some(self.opcode_buf[buf_index].opcode),
                None,
            ],
            exception_addr,
        );
    }
}
