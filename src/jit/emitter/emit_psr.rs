use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::vixl::vixl::FlagsUpdate_DontCare;
use crate::jit::assembler::vixl::MasmMov4;
use crate::jit::inst_info::Operand;
use crate::jit::inst_thread_regs_handler::{register_set_cpsr_checked, register_set_spsr_checked};
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;

impl<const CPU: CpuType> JitAsm<'_, CPU> {
    pub fn emit_msr(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];

        let flags = (inst.opcode >> 16) & 0xF;

        let func = match inst.op {
            Op::MsrRc | Op::MsrIc => register_set_cpsr_checked::<CPU> as *const (),
            Op::MsrRs | Op::MsrIs => register_set_spsr_checked::<CPU> as *const (),
            _ => unreachable!(),
        };

        block_asm.save_dirty_guest_regs(true, inst.cond == Cond::AL);
        match inst.operands()[0] {
            Operand::Reg { reg, shift: None } => {
                let reg = block_asm.get_guest_map(reg);
                block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &reg.into());
            }
            Operand::Imm(imm) => block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &imm.into()),
            _ => unreachable!(),
        }
        block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &flags.into());
        block_asm.call(func);

        let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);
        block_asm.restore_tmp_regs(next_live_regs);

        const REG_TO_RESTORE: RegReserve = reg_reserve!(Reg::R8, Reg::R9, Reg::R10, Reg::R11, Reg::R12, Reg::SP, Reg::LR);
        block_asm.unload_active_guest_regs(REG_TO_RESTORE);
    }
}
