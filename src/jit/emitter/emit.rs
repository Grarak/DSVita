use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::block_asm::{BlockAsm, CPSR_TMP_REG};
use crate::jit::emitter::map_fun_cpu;
use crate::jit::inst_branch_handler::branch_any_reg;
use crate::jit::inst_thread_regs_handler::{register_restore_spsr, restore_thumb_after_restore_spsr, set_pc_arm_mode, set_pc_thumb_mode};
use crate::jit::jit_asm::{debug_after_exec_op, JitAsm, JitCondIndirectBranch, JitRuntimeData};
use crate::jit::op::Op;
use crate::jit::reg::{reg_reserve, Reg};
use crate::jit::Cond;
use crate::logging::debug_println;
use crate::settings::Arm7Emu;
use crate::{DEBUG_LOG, IS_DEBUG};
use std::ptr;
use vixl::{
    BranchHint_kFar, BranchHint_kNear, FlagsUpdate_DontCare, FlagsUpdate_LeaveFlags, Label, MasmAdd5, MasmB2, MasmB3, MasmBkpt1, MasmBx1, MasmLdr2, MasmLdrh2, MasmMov4, MasmStr2, MasmStrh2, MasmSub5,
};

impl JitAsm<'_> {
    pub fn emit(&mut self, block_asm: &mut BlockAsm, thumb: bool) {
        block_asm.guest_inst_offsets.reserve(self.jit_buf.insts.len() - 1);

        for i in 0..self.analyzer.basic_blocks.len() {
            let basic_block = &self.analyzer.basic_blocks[i];
            let metadata = self.analyzer.insts_metadata[basic_block.start_index];
            if metadata.local_branch_entry() {
                block_asm.guest_basic_block_labels[i] = Some(Label::new());
            }
            let required_guest_regs = basic_block.get_inputs() - Reg::PC;
            block_asm.init_guest_regs_mapping(required_guest_regs, basic_block.output_regs, i);
        }

        for i in 0..self.analyzer.basic_blocks.len() {
            let cursor_start = block_asm.get_cursor_offset() as usize;
            self.jit_buf.debug_info.record_basic_block_offset(i, cursor_start);
            let required_guest_regs = self.analyzer.basic_blocks[i].get_inputs() - Reg::PC;

            block_asm.init_guest_regs(required_guest_regs, i);
            if i == 0 {
                block_asm.reload_active_guest_regs_all();
            }

            if self.analyzer.insts_metadata[self.analyzer.basic_blocks[i].start_index].local_branch_entry() {
                block_asm.bind_basic_block(i);
            }

            self.emit_basic_block(i, block_asm, thumb);

            let block_size = block_asm.get_cursor_offset() as usize - cursor_start;
            self.jit_buf.debug_info.record_basic_block(self.analyzer.basic_blocks[i].start_pc, cursor_start, block_size);
        }

        self.jit_buf.debug_info.record_inst_offset(self.jit_buf.insts.len(), block_asm.get_cursor_offset() as usize);
    }

    pub fn emit_epilogue(&mut self, block_asm: &mut BlockAsm) {
        for i in 0..self.jit_buf.forward_branches.len() {
            let basic_block_index = self.analyzer.get_basic_block_from_inst(self.jit_buf.forward_branches[i].inst_index);
            let cursor_start = block_asm.get_cursor_offset() as usize;
            self.emit_forward_branch(i, block_asm);
            let block_size = block_asm.get_cursor_offset() as usize - cursor_start;
            self.jit_buf
                .debug_info
                .record_basic_block(self.analyzer.basic_blocks[basic_block_index].start_pc, cursor_start, block_size);
        }
        for i in 0..self.jit_buf.run_scheduler_labels.len() {
            let basic_block_index = self.analyzer.get_basic_block_from_inst(self.jit_buf.run_scheduler_labels[i].inst_index);
            let cursor_start = block_asm.get_cursor_offset() as usize;
            self.emit_run_scheduler(i, block_asm);
            let block_size = block_asm.get_cursor_offset() as usize - cursor_start;
            self.jit_buf
                .debug_info
                .record_basic_block(self.analyzer.basic_blocks[basic_block_index].start_pc, cursor_start, block_size);
        }
        for i in 0..self.jit_buf.cond_indirect_branches.len() {
            let cursor_start = block_asm.get_cursor_offset() as usize;
            let basic_block_index = self.analyzer.get_basic_block_from_inst(self.jit_buf.cond_indirect_branches[i].inst_index);
            self.emit_cond_indirect_branch(i, block_asm);
            let block_size = block_asm.get_cursor_offset() as usize - cursor_start;
            self.jit_buf
                .debug_info
                .record_basic_block(self.analyzer.basic_blocks[basic_block_index].start_pc, cursor_start, block_size);
        }
    }

    fn emit_cond_indirect_branch(&mut self, cond_indirect_branch_index: usize, block_asm: &mut BlockAsm) {
        let cond_indirect_branch = &mut self.jit_buf.cond_indirect_branches[cond_indirect_branch_index];
        block_asm.bind(&mut cond_indirect_branch.bind_label);
        block_asm.current_pc = self.analyzer.get_pc_from_inst(cond_indirect_branch.inst_index);
        block_asm.dirty_guest_regs = cond_indirect_branch.dirty_guest_regs;
        block_asm.set_guest_regs_mapping(cond_indirect_branch.guest_regs_mapping);
        let inst_index = cond_indirect_branch.inst_index;
        self.handle_indirect_branch(inst_index, block_asm);
    }

    fn handle_indirect_branch(&mut self, inst_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];

        block_asm.save_dirty_guest_regs(true, inst.cond == Cond::AL);

        let restore_spsr = inst.out_regs.is_reserved(Reg::CPSR) && inst.op.is_alu();
        if restore_spsr {
            block_asm.call(map_fun_cpu!(self.cpu, register_restore_spsr));
        }

        if self.cpu == ARM7 || (!inst.op.is_single_mem_transfer() && !inst.op.is_multiple_mem_transfer()) {
            if restore_spsr {
                block_asm.call(map_fun_cpu!(self.cpu, restore_thumb_after_restore_spsr));
            } else {
                block_asm.call(map_fun_cpu!(self.cpu, set_pc_arm_mode));
            }
        } else if restore_spsr {
            block_asm.call(map_fun_cpu!(self.cpu, restore_thumb_after_restore_spsr));
        }

        if (inst.op.is_mov() && inst.src_regs.is_reserved(Reg::LR) && !inst.out_regs.is_reserved(Reg::CPSR))
            || (inst.op.is_multiple_mem_transfer() && inst.operands()[0].as_reg_no_shift().unwrap() == Reg::SP)
            || (inst.op.is_single_mem_transfer() && inst.src_regs.is_reserved(Reg::SP))
        {
            match inst.op {
                Op::Ldm(transfer) | Op::Stm(transfer) => {
                    if transfer.user() {
                        block_asm.call(map_fun_cpu!(self.cpu, register_restore_spsr));
                        if self.cpu == ARM7 {
                            block_asm.call(set_pc_arm_mode::<{ ARM7 }> as _);
                        }
                    }
                }
                _ => {}
            }

            let pc_reg = block_asm.get_guest_map(Reg::PC);
            self.emit_branch_return_stack(inst_index, pc_reg, block_asm);
        } else if self.cpu == ARM9 {
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &self.jit_buf.insts_cycle_counts[inst_index].into());
            if IS_DEBUG {
                let pc = block_asm.current_pc;
                block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &pc.into());
            }
            block_asm.restore_stack();
            block_asm.ldr2(
                Reg::R12,
                if self.emu.settings.arm7_emu() == Arm7Emu::Hle {
                    branch_any_reg::<true> as *const ()
                } else {
                    branch_any_reg::<false> as *const ()
                } as u32,
            );
            block_asm.bx1(Reg::R12);
        } else {
            self.emit_branch_out_metadata(inst_index, true, block_asm);
            block_asm.exit_guest_context(&mut self.runtime_data.host_sp);
        }
    }

    fn handle_indirect_branch_thumb(&mut self, inst_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];

        block_asm.save_dirty_guest_regs(true, inst.cond == Cond::AL);

        if self.cpu == ARM7 || !inst.op.is_multiple_mem_transfer() {
            block_asm.call(map_fun_cpu!(self.cpu, set_pc_thumb_mode));
        }

        // R9 can be used as a substitution for SP for branch prediction
        if (inst.op == Op::MovHT && inst.src_regs.is_reserved(Reg::LR))
            || (inst.op.is_multiple_mem_transfer() && matches!(inst.operands()[0].as_reg_no_shift().unwrap(), Reg::R9 | Reg::SP))
            || (inst.op.is_single_mem_transfer() && (inst.src_regs.is_reserved(Reg::R9) || inst.src_regs.is_reserved(Reg::SP)))
        {
            let pc_reg = block_asm.get_guest_map(Reg::PC);
            self.emit_branch_return_stack(inst_index, pc_reg, block_asm);
        } else if self.cpu == ARM9 {
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &self.jit_buf.insts_cycle_counts[inst_index].into());
            if IS_DEBUG {
                let pc = block_asm.current_pc;
                block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &pc.into());
            }
            block_asm.restore_stack();
            block_asm.ldr2(
                Reg::R12,
                if self.emu.settings.arm7_emu() == Arm7Emu::Hle {
                    branch_any_reg::<true> as *const ()
                } else {
                    branch_any_reg::<false> as *const ()
                } as u32,
            );
            block_asm.bx1(Reg::R12);
        } else {
            self.emit_branch_out_metadata(inst_index, true, block_asm);
            block_asm.exit_guest_context(&mut self.runtime_data.host_sp);
        }
    }

    fn emit_basic_block(&mut self, basic_block_index: usize, block_asm: &mut BlockAsm, thumb: bool) {
        let basic_block = &self.analyzer.basic_blocks[basic_block_index];
        let start_index = basic_block.start_index;
        let end_index = basic_block.end_index;
        let start_pc = basic_block.start_pc;
        let pc_shift = if thumb { 1 } else { 2 };

        debug_println!("{:?} emit basic block {basic_block_index}", self.cpu);

        for i in start_index..end_index + 1 {
            block_asm.current_pc = start_pc + (((i - start_index) as u32) << pc_shift);
            // if block_asm.current_pc == 0x20ce114 {
            //     unsafe { BLOCK_LOG = true };
            // }
            self.jit_buf.debug_info.record_inst_offset(i, block_asm.get_cursor_offset() as usize);

            let inst = &self.jit_buf.insts[i];

            debug_println!("{:x}: block {basic_block_index}: emit {inst:?}", block_asm.current_pc);

            // if block_asm.current_pc == 0x37fbd04 {
            //     block_asm.bkpt1(0);
            // }

            if inst.op.is_multiple_mem_transfer() {
                let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
                let op1 = inst.operands()[1].as_reg_list().unwrap();
                let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, i);
                block_asm.alloc_guest_regs(reg_reserve!(op0), op1 & Reg::PC, inst.cond, next_live_regs);
            } else if !inst.op.is_labelled_branch() || inst.out_regs.is_reserved(Reg::LR) {
                self.emit_guest_regs_alloc(i, basic_block_index, block_asm);
            }

            let inst = &self.jit_buf.insts[i];
            if i != 0 {
                block_asm.guest_offset(self.jit_buf.insts_cycle_counts[i] - inst.cycle as u16, self.cpu, self.emu);
            }

            let mut label = Label::new();
            let needs_cond_jump = !thumb
                && !matches!(inst.op, Op::B)
                && (inst.out_regs.is_reserved(Reg::PC) || (!matches!(inst.op, Op::Clz | Op::Qadd | Op::Qsub | Op::Qdadd | Op::Qdsub) && !inst.op.is_alu() && !inst.op.is_mul()));
            if inst.cond != Cond::AL && needs_cond_jump {
                block_asm.b3(!inst.cond, &mut label, BranchHint_kNear);
            }

            if thumb {
                match inst.op {
                    Op::BlSetupT => {}
                    Op::BlOffT | Op::BlxOffT => self.emit_bl_thumb(i, basic_block_index, block_asm),
                    Op::BxRegT => self.emit_bx(i, basic_block_index, block_asm),
                    Op::BT => self.emit_b_thumb(i, basic_block_index, &mut label, block_asm),
                    Op::BlxRegT => self.emit_blx_thumb(i, basic_block_index, block_asm),
                    Op::SwiT => self.emit_swi(i, basic_block_index, block_asm),
                    op if op.is_labelled_branch() && inst.cond != Cond::AL => self.emit_b_thumb(i, basic_block_index, &mut label, block_asm),
                    op if op.is_alu() => self.emit_alu_thumb(i, block_asm),
                    op if op.is_single_mem_transfer() => self.emit_single_transfer(i, basic_block_index, block_asm),
                    op if op.is_multiple_mem_transfer() => self.emit_multiple_transfer(i, basic_block_index, block_asm),
                    _ => todo!("{inst:?}"),
                }
            } else {
                match inst.op {
                    Op::Mcr | Op::Mrc => self.emit_cp15(i, basic_block_index, block_asm),
                    Op::MsrRc | Op::MsrIc | Op::MsrRs | Op::MsrIs => self.emit_msr(i, basic_block_index, block_asm),
                    Op::MrsRc | Op::MrsRs => self.emit_mrs(i, block_asm),
                    Op::BlxReg => self.emit_blx_reg(i, basic_block_index, block_asm),
                    Op::Bl => self.emit_bl(i, basic_block_index, block_asm),
                    Op::B => self.emit_b(i, basic_block_index, &mut label, block_asm),
                    Op::Bx => self.emit_bx(i, basic_block_index, block_asm),
                    Op::Blx => self.emit_blx(i, basic_block_index, block_asm),
                    Op::Swi => self.emit_swi(i, basic_block_index, block_asm),
                    Op::Swp | Op::Swpb => self.emit_swp(i, basic_block_index, block_asm),
                    Op::Clz => self.emit_clz(i, block_asm),
                    Op::Qadd | Op::Qsub | Op::Qdadd | Op::Qdsub => self.emit_q_op(i, block_asm),
                    op if op.is_mul() => self.emit_mul(i, block_asm),
                    op if op.is_alu() => self.emit_alu(i, block_asm),
                    op if op.is_single_mem_transfer() => self.emit_single_transfer(i, basic_block_index, block_asm),
                    op if op.is_multiple_mem_transfer() => self.emit_multiple_transfer(i, basic_block_index, block_asm),
                    _ => todo!("{inst:?}"),
                };
            }

            let inst = &self.jit_buf.insts[i];
            if !inst.op.is_branch() {
                block_asm.add_dirty_guest_regs(inst.out_regs);
            }

            if (inst.op.is_alu() || inst.op.is_single_mem_transfer() || inst.op.is_multiple_mem_transfer()) && inst.out_regs.is_reserved(Reg::PC) {
                if thumb {
                    self.handle_indirect_branch_thumb(i, block_asm);
                } else if inst.cond != Cond::AL {
                    let mut label = Label::new();
                    block_asm.b2(&mut label, BranchHint_kFar);
                    self.jit_buf
                        .cond_indirect_branches
                        .push(JitCondIndirectBranch::new(i, block_asm.dirty_guest_regs, block_asm.get_guest_regs_mapping(), label));
                } else {
                    self.handle_indirect_branch(i, block_asm);
                }
            }

            block_asm.bind(&mut label);

            if DEBUG_LOG {
                block_asm.save_dirty_guest_regs(false, false);
                block_asm.save_dirty_guest_cpsr(true);
                let current_pc = block_asm.current_pc;
                block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R0, &current_pc.into());
                block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &self.jit_buf.insts[i].opcode.into());
                block_asm.call(map_fun_cpu!(self.cpu, debug_after_exec_op));

                let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, i);
                block_asm.restore_tmp_regs(next_live_regs);
            }
        }

        if basic_block_index == self.analyzer.basic_blocks.len() - 1 {
            block_asm.save_dirty_guest_regs(true, true);
        } else {
            let basic_block = &self.analyzer.basic_blocks[basic_block_index + 1];
            let next_block_needs_cpsr = basic_block.get_inputs().is_reserved(Reg::CPSR);
            if block_asm.dirty_guest_regs.is_reserved(Reg::CPSR) {
                block_asm.store_guest_cpsr_reg(if next_block_needs_cpsr { FlagsUpdate_LeaveFlags } else { FlagsUpdate_DontCare }, CPSR_TMP_REG);
            }
            let load_cpsr = !block_asm.dirty_guest_regs.is_reserved(Reg::CPSR) && next_block_needs_cpsr;
            block_asm.relocate_for_basic_block(
                if load_cpsr || !next_block_needs_cpsr { FlagsUpdate_DontCare } else { FlagsUpdate_LeaveFlags },
                basic_block.output_regs,
                basic_block_index + 1,
            );
            if load_cpsr {
                block_asm.load_guest_cpsr_reg(CPSR_TMP_REG);
            }
        }
    }

    pub fn emit_count_cycles(&mut self, total_cycle_count: u16, block_asm: &mut BlockAsm) {
        block_asm.ldrh2(Reg::R1, &(Reg::R0, JitRuntimeData::get_pre_cycle_count_sum_offset() as i32).into());
        block_asm.ldrh2(Reg::R2, &(Reg::R0, JitRuntimeData::get_accumulated_cycles_offset() as i32).into());

        // +2 for branching
        block_asm.add5(FlagsUpdate_DontCare, Cond::AL, Reg::R2, Reg::R2, &(total_cycle_count as u32 + 2).into());
        block_asm.sub5(FlagsUpdate_DontCare, Cond::AL, Reg::R2, Reg::R2, &Reg::R1.into());

        block_asm.strh2(Reg::R2, &(Reg::R0, JitRuntimeData::get_accumulated_cycles_offset() as i32).into());
    }

    pub fn emit_guest_regs_alloc(&mut self, inst_index: usize, basic_block_index: usize, block_asm: &mut BlockAsm) {
        let inst = &self.jit_buf.insts[inst_index];
        let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, inst_index);
        block_asm.alloc_guest_inst(inst, next_live_regs);
    }

    pub fn emit_branch_out_metadata(&mut self, inst_index: usize, count_cycles: bool, block_asm: &mut BlockAsm) {
        let needs_runtime_data = IS_DEBUG || count_cycles;
        if needs_runtime_data {
            block_asm.ldr2(Reg::R0, ptr::addr_of_mut!(self.runtime_data) as u32);
        }
        if IS_DEBUG {
            let pc = block_asm.current_pc;
            block_asm.mov4(FlagsUpdate_DontCare, Cond::AL, Reg::R1, &pc.into());
            block_asm.str2(Reg::R1, &(Reg::R0, JitRuntimeData::get_branch_out_pc_offset() as i32).into());
        }

        if count_cycles {
            self.emit_count_cycles(self.jit_buf.insts_cycle_counts[inst_index], block_asm);
        }
    }
}
