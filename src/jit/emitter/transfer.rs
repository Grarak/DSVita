use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::inst_info::Operand;
use crate::jit::jit::JitAsm;
use crate::jit::reg::reg_reserve;
use crate::jit::Op;
use bilge::prelude::u4;

impl JitAsm {
    pub fn emit_str(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.opcode_buf[buf_index];
        match inst_info.op {
            Op::StrOfip => {
                let operands = inst_info.operands();
                assert_eq!(operands.len(), 3);

                let reg_op0 = match &operands[0] {
                    Operand::Reg { reg, .. } => reg,
                    _ => panic!(),
                };

                let reg_op1 = match &operands[1] {
                    Operand::Reg { reg, .. } => reg,
                    _ => panic!(),
                };

                let tmp_regs = reg_reserve!(*reg_op0, *reg_op1)
                    .get_writable_gp_regs(1, &self.opcode_buf[buf_index + 1..]);

                assert!(tmp_regs.len() >= 1); // TODO

                let tmp_reg = tmp_regs.into_iter().next().unwrap();
                self.jit_buf
                    .extend_from_slice(&AluImm::mov32(tmp_reg, self.vm_mem_offset));
                self.jit_buf
                    .push(AluShiftImm::add_al(tmp_reg, tmp_reg, *reg_op1));

                let mut modified_op = LdrStrImm::from(inst_info.opcode);
                modified_op.set_rn(u4::new(tmp_reg as u8));
                self.jit_buf.push(u32::from(modified_op));
            }
            _ => panic!(),
        };
    }

    pub fn emit_ldr(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.opcode_buf[buf_index];

        let used_regs = inst_info.src_regs + inst_info.out_regs;
        let emulated_regs_count = used_regs.emulated_regs_count();
        if emulated_regs_count > 0 {
            self.handle_emulated_regs(buf_index, pc, |jit_asm, inst_info, reg_reserve| {
                let mut insts = Vec::<u32>::new();
                insts.resize(3, 0);

                let tmp_reg = reg_reserve.pop().unwrap();
                insts[..2].copy_from_slice(&AluImm::mov32(tmp_reg, jit_asm.vm_mem_offset));

                let (reg1, _) = inst_info.operands()[1].as_reg().unwrap();
                insts[2] = AluShiftImm::add_al(*reg1, tmp_reg, *reg1);
                insts
            });
        } else {
            self.jit_buf.push(inst_info.opcode);
        }
    }
}
