use crate::core::CpuType::ARM9;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::inst_cp15_handler::{cp15_read, cp15_write};
use crate::jit::inst_cpu_regs_handler::cpu_regs_halt;
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::Cond;
use vixl::{FlagsUpdate_DontCare, MasmLdr2, MasmMov4};

impl JitAsm<'_> {
    pub fn emit_cp15(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        if self.cpu != ARM9 {
            return;
        }

        let inst = &self.jit_buf.insts[inst_index];

        let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
        let op0_mapped = block_asm.get_guest_map(op0);
        let cn = (inst.opcode >> 16) & 0xF;
        let cm = inst.opcode & 0xF;
        let cp = (inst.opcode >> 5) & 0x7;

        let cp15_reg = (cn << 16) | (cm << 8) | cp;
        if cp15_reg == 0x070004 || cp15_reg == 0x070802 {
            block_asm.save_dirty_guest_regs(true, inst.cond == Cond::AL);
            let pc = block_asm.current_pc;
            block_asm.ldr2(Reg::R1, pc + 4);
            block_asm.store_guest_reg(Reg::R1, Reg::PC);
            block_asm.call(cpu_regs_halt as _);
            self.emit_branch_out_metadata(inst_index, true, block_asm);
            block_asm.exit_guest_context(&mut self.runtime_data.host_sp);
        } else {
            block_asm.save_dirty_guest_cpsr(inst.cond == Cond::AL);
            match inst.op {
                Op::Mcr => {
                    block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &cp15_reg.into());
                    block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &op0_mapped.into());
                    block_asm.call(cp15_write as _);
                }
                Op::Mrc => {
                    block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &cp15_reg.into());
                    block_asm.call(cp15_read as _);
                    block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, op0_mapped, &Reg::R0.into());
                }
                _ => unreachable!(),
            }

            let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);
            block_asm.restore_tmp_regs(next_live_regs);
        }
    }
}
