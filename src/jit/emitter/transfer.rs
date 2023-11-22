use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::emitter::emit::get_writable_gp_regs;
use crate::jit::jit::JitAsm;
use crate::jit::reg::reg_reserve;
use crate::jit::{Op, Operand};
use bilge::prelude::u4;

impl JitAsm {
    pub fn emit_str(&mut self, buf_index: usize, _: u32) -> bool {
        let (opcode, op, inst_info) = &self.opcode_buf[buf_index];
        match op {
            Op::StrOfip => {
                let operands = inst_info.operands();
                assert_eq!(operands.len(), 3);

                let reg_op0 =
                    match &operands[0] {
                        Operand::Reg { reg, .. } => reg,
                        _ => panic!(),
                    };

                let reg_op1 =
                    match &operands[1] {
                        Operand::Reg { reg, .. } => reg,
                        _ => panic!(),
                    };

                let tmp_reg = match get_writable_gp_regs(
                    1,
                    reg_reserve!(*reg_op0, *reg_op1),
                    &self.opcode_buf[buf_index + 1..],
                ) {
                    Ok(regs) => regs[0],
                    Err(_) => todo!(),
                };

                self.jit_memory
                    .write(AluImm::mov16_al(tmp_reg, self.vm_mem_offset as u16));
                self.jit_memory
                    .write(AluImm::mov_t_al(tmp_reg, (self.vm_mem_offset >> 16) as u16));
                self.jit_memory
                    .write(AluShiftImm::add_al(tmp_reg, tmp_reg, *reg_op1));

                let mut modified_op = LdrStrImm::from(*opcode);
                modified_op.set_rn(u4::new(tmp_reg as u8));
                self.jit_memory.write(u32::from(modified_op));
            }
            _ => panic!(),
        };

        true
    }
}
