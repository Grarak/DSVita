use crate::jit::assembler::block_asm::{BlockAsm, CPSR_TMP_REG, GUEST_REGS_PTR_REG};
use crate::jit::emitter::map_fun_cpu;
use crate::jit::inst_info::Operand;
use crate::jit::inst_thread_regs_handler::{register_set_cpsr_checked, register_set_spsr_checked};
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;
use vixl::{FlagsUpdate_DontCare, MaskedSpecialRegisterType_CPSR_f, MasmAnd3, MasmLdrh2, MasmMov4, MasmMrs2, MasmMsr2, MasmOrr3, SpecialRegisterType_CPSR};

impl JitAsm<'_> {
    pub fn emit_msr(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];

        let flags = (inst.opcode >> 16) & 0xF;

        let func = match inst.op {
            Op::MsrRc | Op::MsrIc => map_fun_cpu!(self.cpu, register_set_cpsr_checked),
            Op::MsrRs | Op::MsrIs => map_fun_cpu!(self.cpu, register_set_spsr_checked),
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

        block_asm.msr2(MaskedSpecialRegisterType_CPSR_f.into(), &Reg::R0.into());

        let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);
        block_asm.restore_tmp_regs(next_live_regs);

        const REG_TO_RESTORE: RegReserve = reg_reserve!(Reg::R8, Reg::R9, Reg::R10, Reg::R11, Reg::R12, Reg::SP, Reg::LR);
        block_asm.reload_active_guest_regs(REG_TO_RESTORE);
    }

    pub fn emit_mrs(&mut self, inst_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];
        let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
        let op0_mapped = block_asm.get_guest_map(op0);

        match inst.op {
            Op::MrsRc => {
                block_asm.ldrh2(Reg::R1, &(GUEST_REGS_PTR_REG, Reg::CPSR as i32 * 4).into());
                block_asm.mrs2(CPSR_TMP_REG, SpecialRegisterType_CPSR.into());
                block_asm.and3(CPSR_TMP_REG, CPSR_TMP_REG, &0xF8000000u32.into());
                block_asm.orr3(op0_mapped, Reg::R1, &CPSR_TMP_REG.into());
            }
            Op::MrsRs => block_asm.load_guest_reg(op0_mapped, Reg::SPSR),
            _ => unreachable!(),
        }
    }
}
