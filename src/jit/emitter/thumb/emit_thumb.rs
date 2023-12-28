use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Op;

impl JitAsm {
    pub fn emit_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];
        let out_regs = inst_info.out_regs;

        let emit_func = match inst_info.op {
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
            | Op::LslImmT
            | Op::LslDpT
            | Op::LsrImmT
            | Op::MovImm8T
            | Op::MulDpT
            | Op::NegDpT
            | Op::RorDpT
            | Op::SubImm8T
            | Op::SubRegT
            | Op::TstDpT
            | Op::OrrDpT => JitAsm::emit_alu_common_thumb,
            Op::AddSpImmT => JitAsm::emit_add_sp_imm_thumb,
            Op::AddHT => JitAsm::emit_add_h_thumb,
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
            Op::BxRegT => JitAsm::emit_bx_thumb,

            Op::LdrshRegT
            | Op::LdrbImm5T
            | Op::LdrhImm5T
            | Op::LdrImm5T
            | Op::LdrPcT
            | Op::LdrSpT => JitAsm::emit_ldr_thumb,
            Op::StrbImm5T
            | Op::StrhImm5T
            | Op::StrhRegT
            | Op::StrRegT
            | Op::StrImm5T
            | Op::StrSpT => JitAsm::emit_str_thumb,
            Op::LdmiaT | Op::PopT => JitAsm::emit_ldm_thumb,
            Op::PushLrT => JitAsm::emit_stm_thumb,

            Op::SwiT => JitAsm::emit_swi_thumb,
            _ => todo!("{:?}", inst_info),
        };

        emit_func(self, buf_index, pc);

        if out_regs.is_reserved(Reg::CPSR) {
            self.handle_cpsr(Reg::R8, Reg::R9);
        }

        if out_regs.is_reserved(Reg::PC) {
            todo!()
        }
    }
}
