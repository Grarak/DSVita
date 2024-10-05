use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::Cond;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_b_thumb(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();

        let relative_pc = *inst_info.operands()[0].as_imm().unwrap() as i32 + 4;
        let target_pc = (self.jit_buf.current_pc as i32 + relative_pc) as u32;

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
            _ => unreachable!(),
        };

        block_asm.start_cond_block(cond);
        self.emit_branch_label_common::<true>(block_asm, target_pc, cond, false);
        block_asm.end_cond_block();
    }

    pub fn emit_bl_setup_thumb(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();

        let op0 = *inst_info.operands()[0].as_imm().unwrap() as i32;
        let lr = (self.jit_buf.current_pc as i32 + 4 + op0) as u32;

        block_asm.mov(Reg::LR, lr);
    }

    pub fn emit_bl_thumb(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();

        let op0 = *inst_info.operands()[0].as_imm().unwrap();
        let lr = self.jit_buf.current_pc + 2;

        let target_pc_reg = block_asm.new_reg();
        block_asm.add(target_pc_reg, Reg::LR, op0);

        block_asm.mov(Reg::LR, lr | 1);

        if inst_info.op == Op::BlxOffT {
            block_asm.bic(target_pc_reg, target_pc_reg, 0x1);
        } else {
            block_asm.orr(target_pc_reg, target_pc_reg, 0x1);
        }

        self.emit_branch_reg_common(block_asm, target_pc_reg, true);
        block_asm.free_reg(target_pc_reg);
    }

    pub fn emit_bx_thumb(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();

        let op0 = *inst_info.operands()[0].as_reg_no_shift().unwrap();

        if inst_info.op == Op::BlxRegT {
            block_asm.mov(Reg::LR, self.jit_buf.current_pc + 3);
        }
        block_asm.mov(Reg::PC, op0);
        block_asm.save_context();
        self.emit_branch_out_metadata(block_asm);
        block_asm.epilogue();
    }
}
