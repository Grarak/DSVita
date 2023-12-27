use crate::hle::thread_regs::register_set_cpsr;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::inst_info::Operand;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Cond;

impl JitAsm {
    pub fn emit_msr_cprs(&mut self, buf_index: usize, _: u32) {
        self.emit_call_host_func(
            |asm| {
                let inst_info = &asm.jit_buf.instructions[buf_index];

                if inst_info.cond != Cond::AL {
                    todo!()
                }

                match &inst_info.operands()[0] {
                    Operand::Reg { reg, .. } => {
                        if *reg != Reg::R1 {
                            asm.jit_buf
                                .emit_opcodes
                                .push(AluShiftImm::mov_al(Reg::R1, *reg));
                        }
                    }
                    Operand::Imm(imm) => {
                        asm.jit_buf
                            .emit_opcodes
                            .extend(&AluImm::mov32(Reg::R1, *imm));
                    }
                    _ => panic!(),
                }
            },
            &[Some(self.thread_regs.as_ptr() as _), None],
            register_set_cpsr as _,
        );
    }

    pub fn emit_mrs_cprs(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        if inst_info.cond != Cond::AL {
            todo!()
        }

        let op0 = inst_info.operands()[0].as_reg_no_shift().unwrap();

        self.jit_buf
            .emit_opcodes
            .extend(self.thread_regs.borrow().emit_get_reg(*op0, Reg::CPSR));
    }
}
