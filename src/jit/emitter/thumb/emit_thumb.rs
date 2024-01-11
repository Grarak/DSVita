use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::transfer_assembler::LdrStrImm;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Op;
use std::ptr;

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];
        let op = inst_info.op;
        let out_regs = inst_info.out_regs;

        let emit_func = match op {
            Op::AdcDpT
            | Op::AddImm3T
            | Op::AddImm8T
            | Op::AddRegT
            | Op::AddSpT
            | Op::AndDpT
            | Op::AsrImmT
            | Op::BicDpT
            | Op::CmpDpT
            | Op::CmpImm8T
            | Op::EorDpT
            | Op::LslImmT
            | Op::LslDpT
            | Op::LsrImmT
            | Op::MovImm8T
            | Op::MulDpT
            | Op::MvnDpT
            | Op::NegDpT
            | Op::RorDpT
            | Op::SbcDpT
            | Op::SubImm3T
            | Op::SubImm8T
            | Op::SubRegT
            | Op::TstDpT
            | Op::OrrDpT => JitAsm::emit_alu_common_thumb,
            Op::AddSpImmT => JitAsm::emit_add_sp_imm_thumb,
            Op::AddHT => JitAsm::emit_add_h_thumb,
            Op::CmpHT => JitAsm::emit_cmp_h_thumb,
            Op::MovHT => JitAsm::emit_movh_thumb,

            Op::BT
            | Op::BeqT
            | Op::BneT
            | Op::BcsT
            | Op::BccT
            | Op::BmiT
            | Op::BplT
            | Op::BvsT
            | Op::BvcT
            | Op::BhiT
            | Op::BlsT
            | Op::BgeT
            | Op::BltT
            | Op::BgtT
            | Op::BleT => JitAsm::emit_b_thumb,
            Op::BlSetupT => JitAsm::emit_bl_setup_thumb,
            Op::BlOffT => JitAsm::emit_bl_thumb,
            Op::BxRegT | Op::BlxRegT => JitAsm::emit_bx_thumb,

            Op::LdrshRegT
            | Op::LdrbRegT
            | Op::LdrbImm5T
            | Op::LdrhRegT
            | Op::LdrRegT
            | Op::LdrhImm5T
            | Op::LdrImm5T
            | Op::LdrPcT
            | Op::LdrSpT => JitAsm::emit_ldr_thumb,
            Op::StrbRegT
            | Op::StrbImm5T
            | Op::StrhImm5T
            | Op::StrhRegT
            | Op::StrRegT
            | Op::StrImm5T
            | Op::StrSpT => JitAsm::emit_str_thumb,
            Op::LdmiaT | Op::PopT | Op::PopPcT => JitAsm::emit_ldm_thumb,
            Op::StmiaT | Op::PushT | Op::PushLrT => JitAsm::emit_stm_thumb,

            Op::SwiT => JitAsm::emit_swi_thumb,
            _ => todo!("{:?}", inst_info),
        };

        emit_func(self, buf_index, pc);

        if out_regs.is_reserved(Reg::CPSR) {
            self.handle_cpsr(Reg::R8, Reg::R9);
        }

        if out_regs.is_reserved(Reg::PC) {
            self.jit_buf
                .emit_opcodes
                .extend(&self.thread_regs.borrow().save_regs_thumb_opcodes);

            self.jit_buf
                .emit_opcodes
                .extend(&AluImm::mov32(Reg::R0, pc));
            self.jit_buf.emit_opcodes.extend(AluImm::mov32(
                Reg::LR,
                ptr::addr_of_mut!(self.guest_branch_out_pc) as u32,
            ));

            if CPU == CpuType::ARM7 || op != Op::PopPcT {
                let thread_regs = self.thread_regs.borrow();
                self.jit_buf
                    .emit_opcodes
                    .extend(thread_regs.emit_get_reg(Reg::R1, Reg::PC));
                self.jit_buf
                    .emit_opcodes
                    .push(AluImm::orr_al(Reg::R1, Reg::R1, 1));
                self.jit_buf.emit_opcodes.extend(thread_regs.emit_set_reg(
                    Reg::PC,
                    Reg::R1,
                    Reg::R2,
                ));
            }

            self.jit_buf
                .emit_opcodes
                .push(LdrStrImm::str_al(Reg::R0, Reg::LR));

            Self::emit_host_bx(
                self.breakout_skip_save_regs_thumb_addr,
                &mut self.jit_buf.emit_opcodes,
            );
        }
    }
}
