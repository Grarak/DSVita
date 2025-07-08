use crate::core::exception_handler::ExceptionVector;
use crate::core::CpuType;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::vixl::vixl::FlagsUpdate_DontCare;
use crate::jit::assembler::vixl::{MasmLdr2, MasmMov4};
use crate::jit::inst_exception_handler::software_interrupt_handler;
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;

impl<const CPU: CpuType> JitAsm<'_, CPU> {
    pub fn emit_swi(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];
        let thumb = block_asm.thumb;
        let comment = if thumb { inst.opcode } else { inst.opcode >> 16 } as u8;

        block_asm.save_dirty_guest_regs(true, inst.cond == Cond::AL);

        block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &comment.into());
        let pc = block_asm.current_pc;
        block_asm.ldr2(Reg::R1, pc | thumb as u32);
        block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R2, &self.jit_buf.insts_cycle_counts[inst_index].into());
        block_asm.call(software_interrupt_handler::<CPU> as _);

        let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);
        block_asm.restore_tmp_regs(next_live_regs);

        const REG_TO_RESTORE: RegReserve = reg_reserve!(Reg::R0, Reg::R1, Reg::R3);
        block_asm.reload_active_guest_regs(REG_TO_RESTORE);
    }
}
