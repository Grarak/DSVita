use crate::hle::bios;
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::branch_assembler::Bx;
use crate::jit::jit::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Cond;
use std::ops::DerefMut;

impl JitAsm {
    pub fn emit_swi(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.opcode_buf[buf_index];

        let comment = ((inst_info.opcode >> 16) & 0xFF) as u8;
        let swi_addr = bios::swi as *const () as u32;

        self.jit_buf.extend_from_slice(&self.restore_host_opcodes);

        let regs_addr = self.thread_regs.borrow_mut().deref_mut() as *mut _ as u32;

        self.jit_buf.push(AluImm::mov_al(Reg::R0, comment));
        self.jit_buf
            .extend_from_slice(&AluImm::mov32(Reg::R1, regs_addr));

        self.jit_buf
            .extend_from_slice(&AluImm::mov32(Reg::LR, swi_addr));
        self.jit_buf.push(Bx::blx(Reg::LR, Cond::AL));

        self.jit_buf.extend_from_slice(&self.restore_guest_opcodes);
    }
}
