use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::inst_exception_handler::bios_uninterrupt;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use std::ptr;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_unknown(&mut self, buf_index: usize, pc: u32) {
        let opcode = self.jit_buf.instructions[buf_index].opcode;
        if (opcode & 0xE000000) == 0xA000000 {
            todo!()
        }

        if opcode == 0xEC000000 {
            self.jit_buf.emit_opcodes.extend(&self.restore_host_opcodes);
            self.jit_buf
                .emit_opcodes
                .extend(AluImm::mov32(Reg::R0, self.hle as *mut _ as _));
            Self::emit_host_blx(
                bios_uninterrupt::<CPU> as *const () as _,
                &mut self.jit_buf.emit_opcodes,
            );

            self.jit_buf.emit_opcodes.extend(AluImm::mov32(Reg::R0, pc));
            self.jit_buf.emit_opcodes.extend(AluImm::mov32(
                Reg::R1,
                ptr::addr_of_mut!(self.guest_branch_out_pc) as u32,
            ));
            self.jit_buf
                .emit_opcodes
                .push(LdrStrImm::str_al(Reg::R0, Reg::R1));

            Self::emit_host_bx(
                self.breakout_skip_save_regs_addr,
                &mut self.jit_buf.emit_opcodes,
            );
        }
    }
}
