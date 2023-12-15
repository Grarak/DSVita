use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Op;

impl JitAsm {
    pub fn emit_thumb(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];
        let out_regs = inst_info.out_regs;

        let emit_func = match inst_info.op {
            Op::AddImm8T => JitAsm::emit_add_thumb,
            Op::AsrImmT => JitAsm::emit_asr_thumb,
            Op::MovImm8T => JitAsm::emit_mov_thumb,
            Op::MovHT => JitAsm::emit_movh_thumb,
            Op::SubRegT => JitAsm::emit_sub_thumb,

            Op::BeqT => JitAsm::emit_b_thumb,
            Op::BlSetupT => JitAsm::emit_bl_setup_thumb,
            Op::BlOffT => JitAsm::emit_bl_thumb,
            Op::BxRegT => JitAsm::emit_bx_thumb,

            Op::LdrPcT => JitAsm::emit_ldr_thumb,
            Op::LdmiaT | Op::PopT => JitAsm::emit_ldm_thumb,
            Op::PushLrT => JitAsm::emit_stm_thumb,
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
