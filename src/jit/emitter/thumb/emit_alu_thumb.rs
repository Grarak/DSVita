use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::BlockOperand;
use crate::jit::inst_info::Operand;
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::ShiftType;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_alu_common_thumb(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();
        let operands = inst_info.operands();
        let op0 = *operands[0].as_reg_no_shift().unwrap();
        let (op1, op2) = if operands.len() == 3 {
            (*operands[1].as_reg_no_shift().unwrap(), &operands[2])
        } else {
            (op0, &operands[1])
        };

        match op2 {
            Operand::Reg { reg, shift: None } => match inst_info.op {
                Op::AdcDpT => block_asm.adcs_guest_thumb_pc_aligned(op0, op1, *reg),
                Op::AddRegT => block_asm.adds_guest_thumb_pc_aligned(op0, op1, *reg),
                Op::AndDpT => block_asm.ands_guest_thumb_pc_aligned(op0, op1, *reg),
                Op::AsrDpT => block_asm.movs_guest_thumb_pc_aligned(op0, (op0, ShiftType::Asr, *reg)),
                Op::BicDpT => block_asm.bics_guest_thumb_pc_aligned(op0, op1, *reg),
                Op::CmpDpT => block_asm.cmp_guest_thumb_pc_aligned(op0, *reg),
                Op::CmnDpT => block_asm.cmn_guest_thumb_pc_aligned(op0, *reg),
                Op::EorDpT => block_asm.eors_guest_thumb_pc_aligned(op0, op1, *reg),
                Op::LslDpT => block_asm.movs_guest_thumb_pc_aligned(op0, (op0, ShiftType::Lsl, *reg)),
                Op::LsrDpT => block_asm.movs_guest_thumb_pc_aligned(op0, (op0, ShiftType::Lsr, *reg)),
                Op::MulDpT => block_asm.muls_guest_thumb_pc_aligned(op0, op0, *reg),
                Op::MvnDpT => block_asm.mvns_guest_thumb_pc_aligned(op0, *reg),
                Op::NegDpT => block_asm.rsbs_guest_thumb_pc_aligned(op0, *reg, 0),
                Op::RorDpT => block_asm.movs_guest_thumb_pc_aligned(op0, (op0, ShiftType::Ror, *reg)),
                Op::SbcDpT => block_asm.sbcs_guest_thumb_pc_aligned(op0, op1, *reg),
                Op::SubRegT => block_asm.subs_guest_thumb_pc_aligned(op0, op1, *reg),
                Op::TstDpT => block_asm.tst_guest_thumb_pc_aligned(op0, *reg),
                Op::OrrDpT => block_asm.orrs_guest_thumb_pc_aligned(op0, op1, *reg),
                _ => todo!("{:?}", inst_info),
            },
            Operand::Imm(imm) => match inst_info.op {
                Op::AddImm3T | Op::AddImm8T => block_asm.adds_guest_thumb_pc_aligned(op0, op1, *imm),
                Op::AddPcT | Op::AddSpT => block_asm.add_guest_thumb_pc_aligned(op0, op1, (*imm, ShiftType::Ror, 15)), // imm in steps of 4, ror by 15 * 2
                Op::AsrImmT => block_asm.movs_guest_thumb_pc_aligned(op0, (op1.into(), ShiftType::Asr, BlockOperand::from(*imm))),
                Op::CmpImm8T => block_asm.cmp_guest_thumb_pc_aligned(op0, *imm),
                Op::LslImmT => block_asm.movs_guest_thumb_pc_aligned(op0, (op1.into(), ShiftType::Lsl, BlockOperand::from(*imm))),
                Op::LsrImmT => block_asm.movs_guest_thumb_pc_aligned(op0, (op1.into(), ShiftType::Lsr, BlockOperand::from(*imm))),
                Op::MovImm8T => block_asm.movs_guest_thumb_pc_aligned(op0, *imm),
                Op::SubImm3T | Op::SubImm8T => block_asm.subs_guest_thumb_pc_aligned(op0, op1, *imm),
                _ => todo!("{:?}", inst_info),
            },
            _ => unreachable!(),
        }
    }

    pub fn emit_add_sp_imm_thumb(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();

        let imm = *inst_info.operands()[1].as_imm().unwrap();
        let sub = inst_info.opcode & (1 << 7) != 0;
        // imm in steps of 4, ror by 15 * 2
        if sub {
            block_asm.sub(Reg::SP, Reg::SP, (imm, ShiftType::Ror, 15));
        } else {
            block_asm.add(Reg::SP, Reg::SP, (imm, ShiftType::Ror, 15));
        }
    }

    pub fn emit_add_h_thumb(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();

        let operands = inst_info.operands();
        let op0 = *operands[0].as_reg_no_shift().unwrap();
        let op2 = *operands[1].as_reg_no_shift().unwrap();

        block_asm.add(op0, op0, op2);
    }

    pub fn emit_cmp_h_thumb(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();

        let operands = inst_info.operands();
        let op1 = *operands[0].as_reg_no_shift().unwrap();
        let op2 = *operands[1].as_reg_no_shift().unwrap();

        block_asm.cmp_guest(op1, op2);
    }

    pub fn emit_movh_thumb(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();

        let operands = inst_info.operands();
        let op0 = *operands[0].as_reg_no_shift().unwrap();
        let op2 = *operands[1].as_reg_no_shift().unwrap();

        block_asm.mov(op0, op2);
    }
}
