use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::inst_info::Operand;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::{Cond, Op, ShiftType};

impl JitAsm {
    pub fn emit_alu_common(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let operands = inst_info.operands();
        let op0 = *operands[0].as_reg_no_shift().unwrap();
        let (op1, op2) = if operands.len() == 3 {
            (*operands[1].as_reg_no_shift().unwrap(), &operands[2])
        } else {
            (op0, &operands[1])
        };

        let opcode = match op2 {
            Operand::Reg { reg, .. } => match inst_info.op {
                Op::AddRegT => AluShiftImm::adds_al(op0, op1, *reg),
                Op::BicDpT => AluShiftImm::bics_al(op0, op1, *reg),
                Op::CmpDpT => AluShiftImm::cmp_al(op0, *reg),
                Op::SubRegT => AluShiftImm::subs_al(op0, op1, *reg),
                Op::TstDpT => AluShiftImm::tst_al(op0, *reg),
                Op::OrrDpT => AluShiftImm::orrs_al(op0, op1, *reg),
                _ => todo!("{:?}", inst_info),
            },
            Operand::Imm(imm) => match inst_info.op {
                Op::AddImm3T | Op::AddImm8T => AluImm::adds_al(op0, op1, *imm as u8),
                Op::AddSpT => AluImm::add(op0, op1, (*imm / 4) as u8, 15, Cond::AL), // imm in steps of 4, ror by 15 * 2
                Op::AsrImmT => AluShiftImm::movs(op0, op1, ShiftType::ASR, *imm as u8, Cond::AL),
                Op::CmpImm8T => AluImm::cmp_al(op0, *imm as u8),
                Op::LslImmT => AluShiftImm::movs(op0, op1, ShiftType::LSL, *imm as u8, Cond::AL),
                Op::LsrImmT => AluShiftImm::movs(op0, op1, ShiftType::LSR, *imm as u8, Cond::AL),
                Op::MovImm8T => AluImm::movs_al(op0, *imm as u8),
                Op::SubImm8T => AluImm::subs_al(op0, op1, *imm as u8),
                _ => todo!("{:?}", inst_info),
            },
            _ => panic!(),
        };

        self.jit_buf.emit_opcodes.push(opcode)
    }

    pub fn emit_add_sp_imm_thumb(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let imm = inst_info.opcode & 0x7F;
        let sub = inst_info.opcode & (1 << 7) != 0;
        // imm in steps of 4, ror by 15 * 2
        let opcode = if sub {
            AluImm::sub(Reg::SP, Reg::SP, imm as u8, 15, Cond::AL)
        } else {
            AluImm::add(Reg::SP, Reg::SP, imm as u8, 15, Cond::AL)
        };

        self.jit_buf.emit_opcodes.push(opcode);
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
}
