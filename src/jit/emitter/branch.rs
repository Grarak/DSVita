use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::transfer_assembler::{Bx, LdrStrImm};
use crate::jit::jit::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::{Cond, Op};

impl JitAsm {
    pub fn emit_blx(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.opcode_buf[buf_index];

        self.jit_buf
            .extend_from_slice(&self.thread_regs.borrow().emit_save_regs());

        match inst_info.op {
            Op::BlxReg => {
                let (reg, _) = inst_info.operands()[0].as_reg().unwrap();
                self.jit_buf
                    .extend_from_slice(&self.thread_regs.borrow().emit_set_reg(
                        Reg::PC,
                        *reg,
                        Reg::R0,
                    ));
            }
            _ => todo!(),
        }

        let original_sp_addr = self.original_regs.get_sp_addr();
        let addr_diff = self.original_regs.addr_diff();

        self.jit_buf
            .extend_from_slice(&AluImm::mov32(Reg::R0, original_sp_addr));
        self.jit_buf.push(LdrStrImm::ldr_al(Reg::SP, Reg::R0));
        self.jit_buf
            .push(LdrStrImm::ldr_offset_al(Reg::LR, Reg::R0, addr_diff as u16));
        self.jit_buf.push(Bx::bx(Reg::LR, Cond::AL));
    }
}
