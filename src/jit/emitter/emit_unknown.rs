use crate::emu::CpuType;
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::transfer_assembler::{LdrStrImm, LdrStrImmSBHD};
use crate::jit::inst_exception_handler::bios_uninterrupt;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::DEBUG_LOG_BRANCH_OUT;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_unknown(&mut self, buf_index: usize, pc: u32) {
        let opcode = self.jit_buf.instructions[buf_index].opcode;
        if opcode == 0xEC000000 {
            let opcodes = &mut self.jit_buf.emit_opcodes;
            opcodes.extend(&self.restore_host_opcodes);
            opcodes.extend(AluImm::mov32(Reg::R0, self.emu as *mut _ as _));
            Self::emit_host_blx(bios_uninterrupt::<CPU> as *const () as _, opcodes);

            opcodes.extend(self.runtime_data.emit_get_branch_out_addr(Reg::R1));
            opcodes.push(AluImm::mov16_al(
                Reg::R2,
                self.jit_buf.insts_cycle_counts[buf_index],
            ));

            if DEBUG_LOG_BRANCH_OUT {
                opcodes.extend(AluImm::mov32(Reg::R0, pc));
                opcodes.push(LdrStrImm::str_al(Reg::R0, Reg::R1));
            }
            opcodes.push(LdrStrImmSBHD::strh_al(Reg::R2, Reg::R1, 4));

            Self::emit_host_bx(self.breakout_skip_save_regs_addr, opcodes);
        }
    }
}
