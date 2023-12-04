use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::transfer_assembler::Msr;
use crate::jit::inst_info::Operand;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Cond;

impl JitAsm {
    pub fn emit_msr_cprs(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        if inst_info.cond != Cond::AL {
            todo!()
        }

        let mut reserved = inst_info
            .src_regs
            .create_push_pop_handler(2, &self.jit_buf.instructions[buf_index + 1..]);

        let tmp_addr = reserved.pop().unwrap();

        let op1 = match &inst_info.operands()[0] {
            Operand::Reg { reg, .. } => {
                if let Some(opcode) = reserved.emit_push_stack(Reg::LR) {
                    self.jit_buf.emit_opcodes.push(opcode);
                }

                *reg
            }
            Operand::Imm(imm) => {
                let reg = reserved.pop().unwrap();

                if let Some(opcode) = reserved.emit_push_stack(Reg::LR) {
                    self.jit_buf.emit_opcodes.push(opcode);
                }

                self.jit_buf.emit_opcodes.extend(&AluImm::mov32(reg, *imm));
                reg
            }
            _ => panic!(),
        };

        self.jit_buf
            .emit_opcodes
            .push(Msr::cpsr_flags(op1, Cond::AL));

        self.jit_buf
            .emit_opcodes
            .extend(
                &self
                    .thread_regs
                    .borrow()
                    .emit_set_reg(Reg::CPSR, op1, tmp_addr),
            );

        if let Some(opcode) = reserved.emit_pop_stack(Reg::LR) {
            self.jit_buf.emit_opcodes.push(opcode);
        }
    }

    pub fn emit_mrs_cprs(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        if inst_info.cond != Cond::AL {
            todo!()
        }

        let op0 = inst_info.operands()[0].as_reg_no_shift().unwrap();

        self.jit_buf
            .emit_opcodes
            .extend(&self.thread_regs.borrow().emit_get_reg(*op0, Reg::CPSR));
    }
}
