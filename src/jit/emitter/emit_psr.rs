use crate::hle::thread_regs::{register_set_cpsr_checked, register_set_spsr_checked};
use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::branch_assembler::B;
use crate::jit::inst_info::Operand;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::{Cond, Op};

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit_msr(&mut self, buf_index: usize, _: u32) {
        let op = self.jit_buf.instructions[buf_index].op;

        let opcodes = self.emit_call_host_func(
            |asm, opcodes| {
                let inst_info = &asm.jit_buf.instructions[buf_index];

                if inst_info.cond != Cond::AL {
                    todo!()
                }

                match &inst_info.operands()[0] {
                    Operand::Reg { reg, .. } => {
                        if *reg != Reg::R1 {
                            opcodes.push(AluShiftImm::mov_al(Reg::R1, *reg));
                        }
                    }
                    Operand::Imm(imm) => todo!(),
                    _ => panic!(),
                }

                let flags = (inst_info.opcode >> 16) & 0xF;
                opcodes.push(AluImm::mov_al(Reg::R2, flags as u8));
            },
            |_, _, _| {},
            &[Some(self.thread_regs.as_ptr() as _), None, None],
            match op {
                Op::MsrRc => register_set_cpsr_checked::<CPU> as _,
                Op::MsrRs => register_set_spsr_checked::<CPU> as _,
                _ => todo!(),
            },
        );
        self.jit_buf.emit_opcodes.extend(opcodes);
    }

    pub fn emit_mrs(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let mut opcodes = Vec::new();

        let op0 = inst_info.operands()[0].as_reg_no_shift().unwrap();
        opcodes.extend(self.thread_regs.borrow().emit_get_reg(
            *op0,
            match inst_info.op {
                Op::MrsRc => Reg::CPSR,
                Op::MrsRs => Reg::SPSR,
                _ => todo!(),
            },
        ));

        if inst_info.cond != Cond::AL {
            self.jit_buf
                .emit_opcodes
                .push(B::b(opcodes.len() as i32 - 1, !inst_info.cond));
        }
        self.jit_buf.emit_opcodes.extend(opcodes);
    }
}
