use crate::core::CpuType;
use crate::core::CpuType::ARM7;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::inst_threag_regs_handler::{register_restore_spsr, restore_thumb_after_restore_spsr, set_pc_arm_mode};
use crate::jit::jit_asm::{JitAsm, JitRuntimeData};
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::{Cond, MemoryAmount};
use crate::DEBUG_LOG_BRANCH_OUT;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit(&mut self, block_asm: &mut BlockAsm) {
        block_asm.guest_pc(self.jit_buf.current_pc);

        let op = self.jit_buf.current_inst().op;
        let cond = self.jit_buf.current_inst().cond;

        block_asm.start_cond_block(cond);
        match op {
            Op::B | Op::Bl => self.emit_branch_label(block_asm),
            Op::Bx | Op::BlxReg => self.emit_branch_reg(block_asm),
            Op::Blx => self.emit_blx_label(block_asm),
            Op::Mcr | Op::Mrc => self.emit_cp15(block_asm),
            Op::MsrRc | Op::MsrIc | Op::MsrRs | Op::MsrIs => self.emit_msr(block_asm),
            Op::MrsRc | Op::MrsRs => self.emit_mrs(block_asm),
            Op::Swi => self.emit_swi::<false>(block_asm),
            Op::Swpb | Op::Swp => self.emit_swp(block_asm),
            Op::UnkArm => self.emit_unknown(block_asm),
            op if op.is_single_mem_transfer() => {
                if op.mem_is_write() {
                    self.emit_single_write(block_asm)
                } else {
                    self.emit_single_read(block_asm)
                }
            }
            op if op.is_multiple_mem_transfer() => self.emit_multiple_transfer::<false>(block_asm),
            _ => {
                block_asm.generic_guest_inst(self.jit_buf.current_inst_mut());
            }
        }

        if self.jit_buf.current_inst().out_regs.is_reserved(Reg::PC) && !op.is_multiple_mem_transfer() {
            block_asm.save_context();

            let restore_spsr = self.jit_buf.current_inst().out_regs.is_reserved(Reg::CPSR) && op.is_arm_alu();
            if restore_spsr {
                block_asm.call(register_restore_spsr::<CPU> as *const ());
            }

            if CPU == ARM7 || !op.is_single_mem_transfer() {
                if restore_spsr {
                    block_asm.call(restore_thumb_after_restore_spsr::<CPU> as *const ());
                } else {
                    block_asm.call(set_pc_arm_mode::<CPU> as *const ())
                }
            } else if restore_spsr {
                block_asm.call(restore_thumb_after_restore_spsr::<CPU> as *const ());
            }

            self.emit_branch_out_metadata(block_asm);
            block_asm.epilogue();
        }

        block_asm.end_cond_block();
    }

    fn _emit_branch_out_metadata(&mut self, block_asm: &mut BlockAsm, set_idle_loop: bool) {
        let runtime_data_addr_reg = block_asm.new_reg();
        block_asm.mov(runtime_data_addr_reg, self.runtime_data.get_addr() as u32);

        let total_cycles_reg = block_asm.new_reg();
        block_asm.mov(total_cycles_reg, self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32);

        if DEBUG_LOG_BRANCH_OUT {
            let pc_reg = block_asm.new_reg();
            block_asm.mov(pc_reg, self.jit_buf.current_pc);
            block_asm.transfer_write(pc_reg, runtime_data_addr_reg, JitRuntimeData::get_out_pc_offset() as u32, false, MemoryAmount::Word);

            block_asm.free_reg(pc_reg);
        }
        block_asm.transfer_write(total_cycles_reg, runtime_data_addr_reg, JitRuntimeData::get_out_total_cycles_offset() as u32, false, MemoryAmount::Word);
        if set_idle_loop {
            let idle_loop_reg = block_asm.new_reg();
            block_asm.mov(idle_loop_reg, 1);
            block_asm.transfer_write(idle_loop_reg, runtime_data_addr_reg, JitRuntimeData::get_idle_loop_offset() as u32, false, MemoryAmount::Byte);

            block_asm.free_reg(idle_loop_reg);
        }

        block_asm.free_reg(total_cycles_reg);
        block_asm.free_reg(runtime_data_addr_reg);
    }

    pub fn emit_branch_out_metadata(&mut self, block_asm: &mut BlockAsm) {
        self._emit_branch_out_metadata(block_asm, false)
    }

    pub fn emit_branch_out_metadata_with_idle_loop(&mut self, block_asm: &mut BlockAsm) {
        self._emit_branch_out_metadata(block_asm, true)
    }

    pub fn emit_flush_cycles<ContinueFn: Fn(&mut Self, &mut BlockAsm), BreakoutFn: Fn(&mut Self, &mut BlockAsm)>(
        &mut self,
        block_asm: &mut BlockAsm,
        target_pre_cycle_count_sum: u16,
        continue_fn: ContinueFn,
        breakout_fn: BreakoutFn,
    ) {
        let runtime_data_addr_reg = block_asm.new_reg();
        block_asm.mov(runtime_data_addr_reg, self.runtime_data.get_addr() as u32);

        let accumulated_cycles_reg = block_asm.new_reg();
        block_asm.transfer_read(
            accumulated_cycles_reg,
            runtime_data_addr_reg,
            JitRuntimeData::get_accumulated_cycles_offset() as u32,
            false,
            MemoryAmount::Half,
        );

        let pre_cycle_count_sum_reg = block_asm.new_reg();
        block_asm.transfer_read(
            pre_cycle_count_sum_reg,
            runtime_data_addr_reg,
            JitRuntimeData::get_pre_cycle_count_sum_offset() as u32,
            false,
            MemoryAmount::Half,
        );

        let total_cycles_reg = block_asm.new_reg();
        // +2 for branching
        block_asm.add(total_cycles_reg, accumulated_cycles_reg, self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32 + 2);
        block_asm.sub(total_cycles_reg, total_cycles_reg, pre_cycle_count_sum_reg);

        const MAX_LOOP_CYCLE_COUNT: u32 = 200;
        block_asm.cmp(total_cycles_reg, MAX_LOOP_CYCLE_COUNT - 1);

        let breakout_label = block_asm.new_label();
        block_asm.branch(breakout_label, Cond::HI);

        block_asm.transfer_write(
            total_cycles_reg,
            runtime_data_addr_reg,
            JitRuntimeData::get_accumulated_cycles_offset() as u32,
            false,
            MemoryAmount::Half,
        );

        let target_pre_cycle_count_sum_reg = block_asm.new_reg();
        block_asm.mov(target_pre_cycle_count_sum_reg, target_pre_cycle_count_sum as u32);
        block_asm.transfer_write(
            target_pre_cycle_count_sum_reg,
            runtime_data_addr_reg,
            JitRuntimeData::get_pre_cycle_count_sum_offset() as u32,
            false,
            MemoryAmount::Half,
        );
        continue_fn(self, block_asm);

        block_asm.label(breakout_label);
        breakout_fn(self, block_asm);

        block_asm.free_reg(target_pre_cycle_count_sum_reg);
        block_asm.free_reg(total_cycles_reg);
        block_asm.free_reg(pre_cycle_count_sum_reg);
        block_asm.free_reg(accumulated_cycles_reg);
        block_asm.free_reg(runtime_data_addr_reg);
    }
}
