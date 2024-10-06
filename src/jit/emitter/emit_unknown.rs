use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::BlockReg;
use crate::jit::inst_exception_handler::bios_uninterrupt;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::jit::Cond;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_unknown(&mut self, block_asm: &mut BlockAsm) {
        // 0xEC000000 magic number to call finish bios hle interrupt
        if self.jit_buf.current_inst().opcode == 0xEC000000 {
            block_asm.save_context();
            // Returns true if CPU is unhalted
            block_asm.call(bios_uninterrupt::<CPU> as *const ());

            let breakout_label = block_asm.new_label();
            block_asm.cmp(BlockReg::Fixed(Reg::R0), 0);
            block_asm.branch(breakout_label, Cond::EQ);

            let target_pc_reg = block_asm.new_reg();
            block_asm.load_u32(target_pc_reg, block_asm.thread_regs_addr_reg, Reg::PC as u32 * 4);
            self.emit_branch_reg_common(block_asm, target_pc_reg, false);
            block_asm.free_reg(target_pc_reg);

            block_asm.label(breakout_label);
            self.emit_branch_out_metadata(block_asm);
            block_asm.epilogue();
        }
    }
}
