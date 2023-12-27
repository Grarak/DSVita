use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, Op};
use std::ptr;

impl JitAsm {
    pub fn emit_b_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let imm = *inst_info.operands()[0].as_imm().unwrap() as i32;
        let new_pc = (pc as i32 + 4 + imm) as u32;

        let cond = match inst_info.op {
            Op::BT => Cond::AL,
            Op::BeqT => Cond::EQ,
            Op::BneT => Cond::NE,
            Op::BcsT => Cond::HS,
            Op::BccT => Cond::LO,
            Op::BmiT => Cond::MI,
            Op::BplT => Cond::PL,
            Op::BvsT => Cond::VS,
            Op::BvcT => Cond::VC,
            Op::BhiT => Cond::HI,
            Op::BlsT => Cond::LS,
            Op::BgeT => Cond::GE,
            Op::BltT => Cond::LT,
            Op::BgtT => Cond::GT,
            Op::BleT => Cond::LE,
            _ => panic!(),
        };

        let mut opcodes = Vec::<u32>::new();

        opcodes.extend(AluImm::mov32(Reg::R8, pc));
        opcodes.extend(AluImm::mov32(Reg::R9, ptr::addr_of_mut!(self.guest_branch_out_pc) as u32));
        opcodes.extend(AluImm::mov32(Reg::R10, new_pc | 1));

        opcodes.push(LdrStrImm::str_al(Reg::R8, Reg::R9));

        opcodes.extend(
            self.thread_regs
                .borrow()
                .emit_set_reg(Reg::PC, Reg::R10, Reg::R11),
        );

        JitAsm::emit_host_bx(self.breakout_thumb_addr, &mut opcodes);

        if cond != Cond::AL {
            if new_pc < pc {
                self.jit_buf
                    .emit_opcodes
                    .push(B::b(opcodes.len() as i32 - 1, Cond::AL));
            } else {
                self.jit_buf
                    .emit_opcodes
                    .push(B::b(opcodes.len() as i32 - 1, !cond));
            }
        }

        self.jit_buf.emit_opcodes.extend(&opcodes);

        if cond != Cond::AL && new_pc < pc {
            self.jit_buf
                .emit_opcodes
                .push(B::b(-(opcodes.len() as i32) - 2, cond));
        }
    }

    pub fn emit_bl_setup_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let op0 = *inst_info.operands()[0].as_imm().unwrap() as i32;
        let lr = (pc as i32 + 4 + op0) as u32;

        self.jit_buf.emit_opcodes.extend(AluImm::mov32(Reg::R8, lr));
        self.jit_buf
            .emit_opcodes
            .extend(
                self.thread_regs
                    .borrow()
                    .emit_set_reg(Reg::LR, Reg::R8, Reg::R9),
            );
    }

    pub fn emit_bl_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let op0 = inst_info.operands()[0].as_imm().unwrap();
        let lr = (pc + 2) | 1;

        self.jit_buf
            .emit_opcodes
            .extend(AluImm::mov32(Reg::R10, pc));
        self.jit_buf
            .emit_opcodes
            .extend(AluImm::mov32(Reg::R11, ptr::addr_of_mut!(self.guest_branch_out_pc) as u32));

        let thread_regs = self.thread_regs.borrow();
        self.jit_buf
            .emit_opcodes
            .extend(thread_regs.emit_get_reg(Reg::R8, Reg::LR));

        self.jit_buf
            .emit_opcodes
            .push(LdrStrImm::str_al(Reg::R10, Reg::R11));

        self.jit_buf
            .emit_opcodes
            .extend(AluImm::mov32(Reg::R9, *op0 | 1));

        self.jit_buf
            .emit_opcodes
            .extend(AluImm::mov32(Reg::R10, lr));

        self.jit_buf
            .emit_opcodes
            .push(AluShiftImm::add_al(Reg::R8, Reg::R8, Reg::R9));

        self.jit_buf
            .emit_opcodes
            .extend(thread_regs.emit_set_reg(Reg::LR, Reg::R10, Reg::R11));

        self.jit_buf
            .emit_opcodes
            .extend(thread_regs.emit_set_reg(Reg::PC, Reg::R8, Reg::R9));

        JitAsm::emit_host_bx(self.breakout_thumb_addr, &mut self.jit_buf.emit_opcodes);
    }

    pub fn emit_bx_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let op0 = inst_info.operands()[0].as_reg_no_shift().unwrap();

        let mut reg_reserve = !(RegReserve::gp_thumb() + *op0).get_gp_regs();
        let tmp_reg = reg_reserve.pop().unwrap();
        let tmp_reg2 = reg_reserve.pop().unwrap();

        self.jit_buf.emit_opcodes.extend(AluImm::mov32(tmp_reg, pc));
        self.jit_buf
            .emit_opcodes
            .extend(AluImm::mov32(tmp_reg2, ptr::addr_of_mut!(self.guest_branch_out_pc) as u32));
        self.jit_buf
            .emit_opcodes
            .push(LdrStrImm::str_al(tmp_reg, tmp_reg2));

        if op0.is_emulated() {
            let thread_regs = self.thread_regs.borrow();
            if *op0 == Reg::PC {
                self.jit_buf
                    .emit_opcodes
                    .push(AluImm::add_al(tmp_reg, tmp_reg, 4));
            } else {
                self.jit_buf
                    .emit_opcodes
                    .extend(thread_regs.emit_get_reg(tmp_reg, *op0));
            }
            self.jit_buf
                .emit_opcodes
                .extend(thread_regs.emit_set_reg(Reg::PC, tmp_reg, tmp_reg2));
        } else {
            self.jit_buf
                .emit_opcodes
                .extend(
                    self.thread_regs
                        .borrow()
                        .emit_set_reg(Reg::PC, *op0, tmp_reg),
                );
        }

        JitAsm::emit_host_bx(self.breakout_thumb_addr, &mut self.jit_buf.emit_opcodes);
    }
}
