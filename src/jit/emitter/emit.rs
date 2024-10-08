use crate::core::emu::get_regs_mut;
use crate::core::CpuType;
use crate::core::CpuType::ARM7;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::{BlockLabel, BlockReg};
use crate::jit::inst_thread_regs_handler::{register_restore_spsr, restore_thumb_after_restore_spsr, set_pc_arm_mode};
use crate::jit::jit_asm::{JitAsm, JitRuntimeData};
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::Cond;
use crate::IS_DEBUG;
use CpuType::ARM9;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit(&mut self, block_asm: &mut BlockAsm) {
        block_asm.guest_pc(self.jit_buf.current_pc);

        let op = self.jit_buf.current_inst().op;
        let cond = self.jit_buf.current_inst().cond;

        block_asm.start_cond_block(cond);
        match op {
            Op::B | Op::Bl => self.emit_branch_label(block_asm),
            Op::Bx => self.emit_bx(block_asm),
            Op::BlxReg => self.emit_blx(block_asm),
            Op::Blx => self.emit_blx_label(block_asm),
            Op::Mcr | Op::Mrc => self.emit_cp15(block_asm),
            Op::MsrRc | Op::MsrIc | Op::MsrRs | Op::MsrIs => self.emit_msr(block_asm),
            Op::MrsRc | Op::MrsRs => self.emit_mrs(block_asm),
            Op::Swi => self.emit_swi::<false>(block_asm),
            Op::Swpb | Op::Swp => self.emit_swp(block_asm),
            Op::UnkArm => unreachable!(),
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

        if self.jit_buf.current_inst().out_regs.is_reserved(Reg::PC) {
            block_asm.save_context();

            let restore_spsr = self.jit_buf.current_inst().out_regs.is_reserved(Reg::CPSR) && op.is_arm_alu();
            if restore_spsr {
                block_asm.call2(register_restore_spsr as *const (), get_regs_mut!(self.emu, CPU) as *mut _ as u32, self.emu as *mut _ as u32);
            }

            if CPU == ARM7 || (!op.is_single_mem_transfer() && !op.is_multiple_mem_transfer()) {
                if restore_spsr {
                    block_asm.call1(restore_thumb_after_restore_spsr as *const (), get_regs_mut!(self.emu, CPU) as *mut _ as u32);
                } else {
                    block_asm.call1(set_pc_arm_mode as *const (), get_regs_mut!(self.emu, CPU) as *mut _ as u32);
                }
            } else if restore_spsr {
                block_asm.call1(restore_thumb_after_restore_spsr as *const (), get_regs_mut!(self.emu, CPU) as *mut _ as u32);
            }

            if (op.is_mov() && self.jit_buf.current_inst().src_regs.is_reserved(Reg::LR) && !self.jit_buf.current_inst().out_regs.is_reserved(Reg::CPSR))
                || (op.is_multiple_mem_transfer() && *self.jit_buf.current_inst().operands()[0].as_reg_no_shift().unwrap() == Reg::SP)
                || (op.is_single_mem_transfer() && self.jit_buf.current_inst().src_regs.is_reserved(Reg::SP))
            {
                let guest_pc_reg = block_asm.new_reg();
                block_asm.load_u32(guest_pc_reg, block_asm.thread_regs_addr_reg, Reg::PC as u32 * 4);
                self.emit_branch_return_stack_common(block_asm, guest_pc_reg);
                block_asm.free_reg(guest_pc_reg);
            }

            self.emit_branch_out_metadata(block_asm);
            block_asm.epilogue();
        }

        block_asm.end_cond_block();
    }

    fn emit_count_cycles(&mut self, block_asm: &mut BlockAsm, runtime_data_addr_reg: BlockReg, result_accumulated_cycles_reg: BlockReg) {
        let pre_cycle_count_sum_reg = block_asm.new_reg();
        block_asm.load_u16(pre_cycle_count_sum_reg, runtime_data_addr_reg, JitRuntimeData::get_pre_cycle_count_sum_offset() as u32);

        let accumulated_cycles_reg = block_asm.new_reg();
        block_asm.load_u16(accumulated_cycles_reg, runtime_data_addr_reg, JitRuntimeData::get_accumulated_cycles_offset() as u32);

        // +2 for branching
        block_asm.add(
            result_accumulated_cycles_reg,
            accumulated_cycles_reg,
            self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32 + 2,
        );
        block_asm.sub(result_accumulated_cycles_reg, result_accumulated_cycles_reg, pre_cycle_count_sum_reg);

        block_asm.store_u32(result_accumulated_cycles_reg, runtime_data_addr_reg, JitRuntimeData::get_accumulated_cycles_offset() as u32);

        block_asm.free_reg(accumulated_cycles_reg);
        block_asm.free_reg(pre_cycle_count_sum_reg);
    }

    fn _emit_branch_out_metadata(&mut self, block_asm: &mut BlockAsm, count_cycles: bool, set_idle_loop: bool) {
        let runtime_data_addr_reg = block_asm.new_reg();
        block_asm.mov(runtime_data_addr_reg, self.runtime_data.get_addr() as u32);

        if IS_DEBUG {
            let pc_reg = block_asm.new_reg();
            block_asm.mov(pc_reg, self.jit_buf.current_pc);
            block_asm.store_u32(pc_reg, runtime_data_addr_reg, JitRuntimeData::get_out_pc_offset() as u32);
            block_asm.free_reg(pc_reg);
        }

        if count_cycles {
            let result_accumulated_cycles_reg = block_asm.new_reg();
            self.emit_count_cycles(block_asm, runtime_data_addr_reg, result_accumulated_cycles_reg);
            block_asm.free_reg(result_accumulated_cycles_reg);
        }

        if set_idle_loop {
            let idle_loop_reg = block_asm.new_reg();
            block_asm.mov(idle_loop_reg, 1);
            block_asm.store_u8(idle_loop_reg, runtime_data_addr_reg, JitRuntimeData::get_idle_loop_offset() as u32);
            block_asm.free_reg(idle_loop_reg);
        }

        block_asm.free_reg(runtime_data_addr_reg);
    }

    pub fn emit_branch_out_metadata(&mut self, block_asm: &mut BlockAsm) {
        self._emit_branch_out_metadata(block_asm, true, false)
    }

    pub fn emit_branch_out_metadata_no_count_cycles(&mut self, block_asm: &mut BlockAsm) {
        self._emit_branch_out_metadata(block_asm, false, false)
    }

    pub fn emit_branch_out_metadata_with_idle_loop(&mut self, block_asm: &mut BlockAsm) {
        self._emit_branch_out_metadata(block_asm, true, true)
    }

    pub fn emit_flush_cycles<ContinueFn: Fn(&mut Self, &mut BlockAsm, BlockReg, BlockLabel), BreakoutFn: Fn(&mut Self, &mut BlockAsm)>(
        &mut self,
        block_asm: &mut BlockAsm,
        target_pre_cycle_count_sum: Option<u16>,
        add_continue_label: bool,
        continue_fn: ContinueFn,
        breakout_fn: BreakoutFn,
    ) {
        let runtime_data_addr_reg = block_asm.new_reg();
        block_asm.mov(runtime_data_addr_reg, self.runtime_data.get_addr() as u32);

        let result_accumulated_cycles_reg = block_asm.new_reg();
        self.emit_count_cycles(block_asm, runtime_data_addr_reg, result_accumulated_cycles_reg);

        const MAX_LOOP_CYCLE_COUNT: u32 = 127;
        block_asm.cmp(
            result_accumulated_cycles_reg,
            match CPU {
                ARM9 => MAX_LOOP_CYCLE_COUNT * 2,
                ARM7 => MAX_LOOP_CYCLE_COUNT,
            },
        );

        let continue_label = if add_continue_label { Some(block_asm.new_label()) } else { None };
        let breakout_label = block_asm.new_label();
        block_asm.branch(breakout_label, Cond::HS);

        if let Some(target_pre_cycle_count_sum) = target_pre_cycle_count_sum {
            let target_pre_cycle_count_sum_reg = block_asm.new_reg();
            block_asm.mov(target_pre_cycle_count_sum_reg, target_pre_cycle_count_sum as u32);
            block_asm.store_u16(target_pre_cycle_count_sum_reg, runtime_data_addr_reg, JitRuntimeData::get_pre_cycle_count_sum_offset() as u32);
            block_asm.free_reg(target_pre_cycle_count_sum_reg);
        }
        continue_fn(self, block_asm, runtime_data_addr_reg, breakout_label);
        if add_continue_label {
            block_asm.branch(continue_label.unwrap(), Cond::AL);
        }

        block_asm.label(breakout_label);
        breakout_fn(self, block_asm);

        if add_continue_label {
            block_asm.label(continue_label.unwrap());
        }

        block_asm.free_reg(result_accumulated_cycles_reg);
        block_asm.free_reg(runtime_data_addr_reg);
    }
}
