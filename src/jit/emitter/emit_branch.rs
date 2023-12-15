use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::{Cond, Op};
use std::ptr;

impl JitAsm {
    pub fn emit_b(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let mut opcodes = Vec::<u32>::new();

        opcodes.extend(&self.thread_regs.borrow().save_regs_opcodes);

        let imm = inst_info.operands()[0].as_imm().unwrap();
        let new_pc = (pc as i32 + 8 + *imm as i32) as u32;

        if inst_info.op == Op::B {
            let inst_index_diff = (new_pc as i32 - pc as i32) / 4;
            let index_to_branch = buf_index as i32 + inst_index_diff;
            if index_to_branch >= 0 && (index_to_branch as usize) < self.jit_buf.instructions.len()
            {
                if inst_index_diff < 0 {
                    let jit_addr_offset = if index_to_branch != 0 {
                        self.jit_buf.jit_addr_mapping[&new_pc] / 4
                    } else {
                        0
                    };
                    let relative_pc =
                        (self.jit_buf.emit_opcodes.len() as u16 - jit_addr_offset) as i32;
                    self.jit_buf
                        .emit_opcodes
                        .push(B::b(-relative_pc - 2, inst_info.cond));
                } else {
                    self.jit_buf
                        .post_branch_mapping
                        .push((self.jit_buf.emit_opcodes.len() as u16 * 4, new_pc));
                    self.jit_buf.emit_opcodes.push(B::b(0, inst_info.cond));
                }

                return;
            }
        }

        opcodes.extend(&AluImm::mov32(Reg::R0, new_pc));
        opcodes.extend(
            &self
                .thread_regs
                .borrow()
                .emit_set_reg(Reg::PC, Reg::R0, Reg::LR),
        );

        opcodes.extend(&AluImm::mov32(Reg::R0, pc));
        opcodes.extend(&AluImm::mov32(Reg::LR, ptr::addr_of_mut!(self.guest_branch_out_pc) as u32));
        opcodes.push(LdrStrImm::str_al(Reg::R0, Reg::LR));

        if inst_info.op == Op::Bl {
            opcodes.push(AluImm::add_al(Reg::R0, Reg::R0, 4));
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
                .push(B::b(opcodes.len() as i32 - 1, !inst_info.cond));
        }

        self.jit_buf.emit_opcodes.extend(&opcodes);
    }

    pub fn emit_bx(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let mut opcodes = Vec::<u32>::new();

        opcodes.extend(&self.thread_regs.borrow().save_regs_opcodes);

        let reg = inst_info.operands()[0].as_reg_no_shift().unwrap();
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

        opcodes.extend(&AluImm::mov32(Reg::R0, pc));
        opcodes.extend(&AluImm::mov32(Reg::LR, ptr::addr_of_mut!(self.guest_branch_out_pc) as u32));
        opcodes.push(LdrStrImm::str_al(Reg::R0, Reg::LR));

        if inst_info.op == Op::BlxReg {
            opcodes.push(AluImm::add_al(Reg::R0, Reg::R0, 4));
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
                .push(B::b(opcodes.len() as i32 - 1, !inst_info.cond));
        }

        self.jit_buf.emit_opcodes.extend(&opcodes);
    }
}
