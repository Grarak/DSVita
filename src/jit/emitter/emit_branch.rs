use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::{Cond, Op};

impl JitAsm {
    pub fn emit_b(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let mut opcodes = Vec::<u32>::new();

        let imm = inst_info.operands()[0].as_imm().unwrap();
        let new_pc = (pc as i32 + 8 + *imm as i32) as u32;

        opcodes.extend(&self.thread_regs.borrow().save_regs_opcodes);
        opcodes.extend(&AluImm::mov32(Reg::R0, new_pc));
        opcodes.extend(
            &self
                .thread_regs
                .borrow()
                .emit_set_reg(Reg::PC, Reg::R0, Reg::LR),
        );

        if inst_info.op == Op::Bl {
            opcodes.extend(&AluImm::mov32(Reg::R0, pc + 4));
            opcodes.extend(
                &self
                    .thread_regs
                    .borrow()
                    .emit_set_reg(Reg::LR, Reg::R0, Reg::LR),
            );
        }

        JitAsm::emit_host_bx(self.breakout_skip_save_regs_addr, &mut opcodes);

        if inst_info.cond != Cond::AL {
            self.jit_buf
                .emit_opcodes
                .push(B::b(opcodes.len() as i32, !inst_info.cond));
        }

        self.jit_buf.emit_opcodes.extend(&opcodes);
    }

    pub fn emit_bx(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let mut opcodes = Vec::<u32>::new();

        let reg = inst_info.operands()[0].as_reg_no_shift().unwrap();

        opcodes.extend(&self.thread_regs.borrow().save_regs_opcodes);

        if *reg == Reg::LR {
            opcodes.extend(&self.thread_regs.borrow().emit_get_reg(Reg::R0, Reg::LR));
            opcodes.extend(
                &self
                    .thread_regs
                    .borrow()
                    .emit_set_reg(Reg::PC, Reg::R0, Reg::LR),
            );
        } else {
            opcodes.extend(
                &self
                    .thread_regs
                    .borrow()
                    .emit_set_reg(Reg::PC, *reg, Reg::LR),
            );
        }

        if inst_info.op == Op::BlxReg {
            opcodes.extend(&AluImm::mov32(Reg::R0, pc + 4));
            opcodes.extend(
                &self
                    .thread_regs
                    .borrow()
                    .emit_set_reg(Reg::LR, Reg::R0, Reg::LR),
            );
        }

        JitAsm::emit_host_bx(self.breakout_skip_save_regs_addr, &mut opcodes);

        if inst_info.cond != Cond::AL {
            self.jit_buf
                .emit_opcodes
                .push(B::b(opcodes.len() as i32, !inst_info.cond));
        }

        self.jit_buf.emit_opcodes.extend(&opcodes);
    }
}
