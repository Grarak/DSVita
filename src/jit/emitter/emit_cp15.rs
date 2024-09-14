use crate::core::CpuType;
use crate::core::CpuType::ARM9;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::BlockReg;
use crate::jit::inst_cp15_handler::{cp15_read, cp15_write};
use crate::jit::inst_cpu_regs_handler::cpu_regs_halt;
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::Reg;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_halt(&mut self, block_asm: &mut BlockAsm) {
        block_asm.mov(Reg::PC, self.jit_buf.current_pc + 4);
        block_asm.save_context();
        block_asm.call(cpu_regs_halt::<CPU> as *const ());

        self.emit_branch_out_metadata(block_asm);
        block_asm.breakout();
    }

    pub fn emit_cp15(&mut self, block_asm: &mut BlockAsm) {
        if CPU != ARM9 {
            return;
        }

        let inst_info = self.jit_buf.current_inst();
        let rd = *inst_info.operands()[0].as_reg_no_shift().unwrap();
        let cn = (inst_info.opcode >> 16) & 0xF;
        let cm = inst_info.opcode & 0xF;
        let cp = (inst_info.opcode >> 5) & 0x7;

        let cp15_reg = (cn << 16) | (cm << 8) | cp;
        if cp15_reg == 0x070004 || cp15_reg == 0x070802 {
            self.emit_halt(block_asm);
        } else {
            let backed_up_cpsr_reg = block_asm.new_reg();
            block_asm.mrs_cpsr(backed_up_cpsr_reg);
            match inst_info.op {
                Op::Mcr => block_asm.call2(cp15_write as *const (), cp15_reg, rd),
                Op::Mrc => {
                    block_asm.call1(cp15_read as *const (), cp15_reg);
                    block_asm.mov(rd, BlockReg::Fixed(Reg::R0));
                }
                _ => unreachable!(),
            }
            block_asm.msr_cpsr(backed_up_cpsr_reg);
            block_asm.free_reg(backed_up_cpsr_reg);
        }
    }
}
