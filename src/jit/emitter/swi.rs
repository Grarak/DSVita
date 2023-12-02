use crate::hle::exception_handler;
use crate::hle::exception_handler::ExceptionVector;
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::branch_assembler::Bx;
use crate::jit::jit::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Cond;
use std::ops::{Deref, DerefMut};

impl JitAsm {
    pub fn emit_swi(&mut self, buf_index: usize, _: u32) {
        let exception_addr = exception_handler::exception as *const () as u32;

        self.jit_buf.extend_from_slice(&self.restore_host_opcodes);

        let cp15_context_addr = self.cp15_context.borrow().deref() as *const _ as u32;
        let regs_addr = self.thread_regs.borrow_mut().deref_mut() as *mut _ as u32;

        self.jit_buf
            .extend_from_slice(&AluImm::mov32(Reg::R0, cp15_context_addr));
        self.jit_buf
            .extend_from_slice(&AluImm::mov32(Reg::R1, regs_addr));
        self.jit_buf
            .extend_from_slice(&AluImm::mov32(Reg::R2, self.opcode_buf[buf_index].opcode));
        self.jit_buf
            .push(AluImm::mov_al(Reg::R3, ExceptionVector::SoftwareInterrupt as u8));

        self.jit_buf
            .extend_from_slice(&AluImm::mov32(Reg::LR, exception_addr));
        self.jit_buf.push(Bx::blx(Reg::LR, Cond::AL));

        self.jit_buf.extend_from_slice(&self.restore_guest_opcodes);
    }
}
