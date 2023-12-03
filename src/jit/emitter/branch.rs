use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::jit::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::{Cond, Op};

impl JitAsm {
    pub fn emit_b(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.opcode_buf[buf_index];

        let mut opcodes = Vec::<u32>::new();

        let (imm, _) = inst_info.operands()[0].as_imm().unwrap();
        let new_pc = (pc as i32 + 8 + *imm as i32) as u32;

        opcodes.extend_from_slice(&self.restore_host_opcodes);
        opcodes.extend_from_slice(&AluImm::mov32(Reg::R0, new_pc));
        opcodes.extend_from_slice(&self.thread_regs.borrow().emit_set_reg(
            Reg::PC,
            Reg::R0,
            Reg::LR,
        ));

        if inst_info.op == Op::Bl {
            self.jit_buf
                .extend_from_slice(&AluImm::mov32(Reg::R0, pc + 4));
            self.jit_buf
                .extend_from_slice(&self.thread_regs.borrow().emit_set_reg(
                    Reg::LR,
                    Reg::R0,
                    Reg::LR,
                ));
        }

        JitAsm::emit_host_bx(self.breakout_skip_save_regs_addr, &mut opcodes);

        if inst_info.cond != Cond::AL {
            self.jit_buf
                .push(B::b(opcodes.len() as i32, !inst_info.cond));
        }

        self.jit_buf.extend_from_slice(&opcodes);
    }

    pub fn emit_bx(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.opcode_buf[buf_index];

        let (reg, _) = inst_info.operands()[0].as_reg().unwrap();

        self.jit_buf.extend_from_slice(&self.restore_host_opcodes);

        if *reg == Reg::LR {
            self.jit_buf
                .extend_from_slice(&self.thread_regs.borrow().emit_get_reg(Reg::R0, Reg::LR));
            self.jit_buf
                .extend_from_slice(&self.thread_regs.borrow().emit_set_reg(
                    Reg::PC,
                    Reg::R0,
                    Reg::LR,
                ));
        } else {
            self.jit_buf
                .extend_from_slice(
                    &self
                        .thread_regs
                        .borrow()
                        .emit_set_reg(Reg::PC, *reg, Reg::LR),
                );
        }

        if inst_info.op == Op::BlxReg {
            self.jit_buf
                .extend_from_slice(&AluImm::mov32(Reg::R0, pc + 4));
            self.jit_buf
                .extend_from_slice(&self.thread_regs.borrow().emit_set_reg(
                    Reg::LR,
                    Reg::R0,
                    Reg::LR,
                ));
        }

        JitAsm::emit_host_bx(self.breakout_skip_save_regs_addr, &mut self.jit_buf);
    }
}
