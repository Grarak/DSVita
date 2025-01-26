use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::{BlockOperand, BlockReg};
use crate::jit::inst_info::Operand;
use crate::jit::inst_thread_regs_handler::{register_set_cpsr_checked, register_set_spsr_checked};
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::Reg;

impl<const CPU: CpuType> JitAsm<'_, CPU> {
    pub fn emit_msr(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();

        let flags = (inst_info.opcode >> 16) & 0xF;

        let func = match inst_info.op {
            Op::MsrRc | Op::MsrIc => register_set_cpsr_checked::<CPU> as *const (),
            Op::MsrRs | Op::MsrIs => register_set_spsr_checked::<CPU> as *const (),
            _ => unreachable!(),
        };

        block_asm.save_context();
        block_asm.call2(
            func,
            match inst_info.operands()[0] {
                Operand::Reg { reg, shift: None } => BlockOperand::from(reg),
                Operand::Imm(imm) => BlockOperand::from(imm),
                _ => unreachable!(),
            },
            flags,
        );
        block_asm.msr_cpsr(BlockReg::Fixed(Reg::R0));
        block_asm.restore_reg(Reg::R8);
        block_asm.restore_reg(Reg::R9);
        block_asm.restore_reg(Reg::R10);
        block_asm.restore_reg(Reg::R11);
        block_asm.restore_reg(Reg::R12);
        block_asm.restore_reg(Reg::SP);
        block_asm.restore_reg(Reg::LR);
    }

    pub fn emit_mrs(&mut self, block_asm: &mut BlockAsm) {
        let op = self.jit_buf.current_inst().op;
        let op0 = self.jit_buf.current_inst().operands()[0].as_reg_no_shift().unwrap();
        block_asm.mov(
            *op0,
            match op {
                Op::MrsRc => Reg::CPSR,
                Op::MrsRs => Reg::SPSR,
                _ => todo!(),
            },
        );
    }
}
