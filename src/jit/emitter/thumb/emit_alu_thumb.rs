use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::inst_info::Operand;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::RegReserve;
use crate::jit::{Cond, ShiftType};

impl JitAsm {
    pub fn emit_add_thumb(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let operands = inst_info.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();

        let opcode = match operands[1] {
            Operand::Reg { .. } => todo!(),
            Operand::Imm(imm) => AluImm::adds_al(*op0, *op0, imm as u8),
            _ => panic!(),
        };

        self.jit_buf.emit_opcodes.push(opcode)
    }

    pub fn emit_asr_thumb(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let operands = inst_info.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();
        let op1 = operands[1].as_reg_no_shift().unwrap();

        let opcode = match operands[2] {
            Operand::Reg { .. } => todo!(),
            Operand::Imm(imm) => AluShiftImm::movs(*op0, *op1, ShiftType::ASR, imm as u8, Cond::AL),
            _ => panic!(),
        };

        self.jit_buf.emit_opcodes.push(opcode)
    }

    pub fn emit_mov_thumb(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let operands = inst_info.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();

        let opcode = match operands[1] {
            Operand::Reg { .. } => todo!(),
            Operand::Imm(imm) => AluImm::movs_al(*op0, imm as u8),
            _ => panic!(),
        };

        self.jit_buf.emit_opcodes.push(opcode)
    }

    pub fn emit_movh_thumb(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let operands = inst_info.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();
        let op2 = operands[1].as_reg_no_shift().unwrap();

        if *op0 == *op2 {
            return;
        }

        if op2.is_high_gp_reg() {
            self.jit_buf
                .emit_opcodes
                .extend(self.thread_regs.borrow().emit_get_reg(*op2, *op2));
        }

        if op0.is_high_gp_reg() {
            let tmp_reg = (RegReserve::gp_thumb() + *op2).next_free().unwrap();
            self.jit_buf
                .emit_opcodes
                .extend(self.thread_regs.borrow().emit_set_reg(*op0, *op2, tmp_reg));
        } else {
            self.jit_buf
                .emit_opcodes
                .push(AluShiftImm::mov_al(*op0, *op2));
        }
    }

    pub fn emit_sub_thumb(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let operands = inst_info.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();
        let op1 = operands[1].as_reg_no_shift().unwrap();

        let opcode = match operands[2] {
            Operand::Reg { reg, .. } => AluShiftImm::subs_al(*op0, *op1, reg),
            Operand::Imm(imm) => AluImm::subs_al(*op0, *op1, imm as u8),
            _ => panic!(),
        };

        self.jit_buf.emit_opcodes.push(opcode)
    }
}
