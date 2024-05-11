use crate::emu::emu::{get_cm_mut, get_regs, get_regs_mut};
use crate::emu::CpuType;
use crate::jit::assembler::arm::alu_assembler::{AluImm, AluShiftImm};
use crate::jit::inst_info::Operand;
use crate::jit::inst_threag_regs_handler::{register_set_cpsr_checked, register_set_spsr_checked};
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Op;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_msr(&mut self, buf_index: usize, _: u32) {
        let regs_addr = get_regs_mut!(self.emu, CPU) as *mut _ as _;
        let cm_addr = get_cm_mut!(self.emu) as *mut _ as _;
        let op = self.jit_buf.instructions[buf_index].op;

        self.jit_buf.emit_opcodes.extend(self.emit_call_host_func(
            |asm, opcodes| {
                let inst_info = &asm.jit_buf.instructions[buf_index];

                match &inst_info.operands()[0] {
                    Operand::Reg { reg, .. } => {
                        if *reg != Reg::R2 {
                            opcodes.push(AluShiftImm::mov_al(Reg::R2, *reg));
                        }
                    }
                    Operand::Imm(imm) => {
                        opcodes.extend(AluImm::mov32(Reg::R2, *imm));
                    }
                    _ => unreachable!(),
                }

                let flags = (inst_info.opcode >> 16) & 0xF;
                opcodes.push(AluImm::mov_al(Reg::R3, flags as u8));
            },
            &[Some(regs_addr), Some(cm_addr), None, None],
            match op {
                Op::MsrRc | Op::MsrIc => register_set_cpsr_checked as _,
                Op::MsrRs | Op::MsrIs => register_set_spsr_checked as _,
                _ => unreachable!(),
            },
        ));
    }

    pub fn emit_mrs(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let opcodes = &mut self.jit_buf.emit_opcodes;

        let op0 = inst_info.operands()[0].as_reg_no_shift().unwrap();
        opcodes.extend(get_regs!(self.emu, CPU).emit_get_reg(
            *op0,
            match inst_info.op {
                Op::MrsRc => Reg::CPSR,
                Op::MrsRs => Reg::SPSR,
                _ => todo!(),
            },
        ));
    }
}
