use crate::hle::cp15_context::Cp15Context;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg};
use crate::jit::assembler::arm::branch_assembler::Bx;
use crate::jit::jit::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::{Cond, Op};
use std::ops::DerefMut;
use std::ptr;

impl JitAsm {
    pub fn emit_cp15(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.opcode_buf[buf_index];

        let (rd, _) = &inst_info.operands()[0].as_reg().unwrap();
        let cn = (inst_info.opcode >> 16) & 0xF;
        let cm = inst_info.opcode & 0xF;
        let cp = (inst_info.opcode >> 5) & 0x7;

        let cp15_reg = (cn << 16) | (cm << 8) | cp;
        let cp15_context_addr = self.cp15_context.borrow_mut().deref_mut() as *mut _ as u32;

        self.jit_buf.extend_from_slice(&self.restore_host_opcodes);

        if inst_info.op == Op::Mcr && **rd != Reg::R2 {
            self.jit_buf.push(AluReg::mov_al(Reg::R2, **rd));
        }

        self.jit_buf
            .extend_from_slice(&AluImm::mov32(Reg::R0, cp15_context_addr));

        match inst_info.op {
            Op::Mcr => {
                self.jit_buf
                    .extend_from_slice(&AluImm::mov32(Reg::R1, cp15_reg));

                let cp15_write_addr = Cp15Context::write as *const () as u32;
                self.jit_buf
                    .extend_from_slice(&AluImm::mov32(Reg::LR, cp15_write_addr));
            }
            Op::Mrc => {
                self.jit_buf
                    .extend_from_slice(&AluImm::mov32(Reg::R1, cp15_reg));
                let reg_addr =
                    ptr::addr_of_mut!(self.thread_regs.borrow_mut().gp_regs[**rd as usize]) as u32;
                self.jit_buf
                    .extend_from_slice(&AluImm::mov32(Reg::R2, reg_addr));

                let cp15_read_addr = Cp15Context::read as *const () as u32;
                self.jit_buf
                    .extend_from_slice(&AluImm::mov32(Reg::LR, cp15_read_addr));
            }
            _ => panic!(),
        }

        self.jit_buf.push(Bx::blx(Reg::LR, Cond::AL));

        self.jit_buf.extend_from_slice(&self.restore_guest_opcodes);
    }
}
