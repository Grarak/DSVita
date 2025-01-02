use crate::core::emu::get_jit;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::BlockReg;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm::{align_guest_pc, JitAsm, JitRuntimeData};
use crate::jit::jit_asm_common_funs::JitAsmCommonFuns;
use crate::jit::jit_memory::{DEFAULT_JIT_ENTRY_ARM7, DEFAULT_JIT_ENTRY_ARM9};
use crate::jit::op::Op;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::Cond;
use crate::DEBUG_LOG;
use std::ptr;

pub enum JitBranchInfo {
    Idle,
    Local(usize),
    None,
}

impl<const CPU: CpuType> JitAsm<'_, CPU> {
    // Taken from https://github.com/melonDS-emu/melonDS/blob/24c402af51fe9c0537582173fc48d1ad3daff459/src/ARMJIT.cpp#L352
    pub fn is_idle_loop(insts: &[InstInfo]) -> bool {
        let mut regs_written_to = RegReserve::new();
        let mut regs_disallowed_to_write = RegReserve::new();
        for (i, inst) in insts.iter().enumerate() {
            if (inst.is_branch() && i < insts.len() - 1)
                || matches!(inst.op, Op::Swi | Op::SwiT | Op::Mcr | Op::Mrc | Op::MrsRc | Op::MrsRs | Op::MsrIc | Op::MsrIs | Op::MsrRc | Op::MsrRs)
                || inst.op.mem_is_write()
            {
                return false;
            }

            let src_regs = inst.src_regs & !reg_reserve!(Reg::PC);
            let out_regs = inst.out_regs & !reg_reserve!(Reg::PC);
            regs_disallowed_to_write |= src_regs & !regs_written_to;

            if !(out_regs & regs_disallowed_to_write).is_empty() {
                return false;
            }
            regs_written_to |= out_regs;
        }
        true
    }

    pub fn analyze_branch_label<const THUMB: bool>(insts: &[InstInfo], branch_index: usize, cond: Cond, pc: u32, target_pc: u32) -> JitBranchInfo {
        let target_pc = target_pc & !1;
        if (THUMB || insts[branch_index].op != Op::Bl) && (cond as u8) < (Cond::AL as u8) && target_pc < pc {
            let diff = (pc - target_pc) >> if THUMB { 1 } else { 2 };
            if diff as usize <= branch_index {
                let jump_to_index = branch_index - diff as usize;
                if Self::is_idle_loop(&insts[jump_to_index..branch_index + 1]) {
                    return JitBranchInfo::Idle;
                }
            }
        }

        let relative_index = (target_pc as i32 - pc as i32) >> if THUMB { 1 } else { 2 };
        let target_index = branch_index as i32 + relative_index;
        if target_index >= 0 && (target_index as usize) < insts.len() {
            JitBranchInfo::Local(target_index as usize)
        } else {
            JitBranchInfo::None
        }
    }

    pub fn emit_branch_label(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();
        let op = inst_info.op;
        let relative_pc = *inst_info.operands()[0].as_imm().unwrap() as i32 + 8;
        let target_pc = (self.jit_buf.current_pc as i32 + relative_pc) as u32;

        if op == Op::Bl {
            block_asm.mov(Reg::LR, self.jit_buf.current_pc + 4);
            self.emit_branch_external_label(block_asm, target_pc, true, false);
        } else {
            self.emit_branch_label_common::<false>(block_asm, target_pc, inst_info.cond);
        }
    }

    pub fn emit_branch_label_common<const THUMB: bool>(&mut self, block_asm: &mut BlockAsm, target_pc: u32, cond: Cond) {
        match Self::analyze_branch_label::<THUMB>(&self.jit_buf.insts, self.jit_buf.current_index, cond, self.jit_buf.current_pc, target_pc) {
            JitBranchInfo::Local(target_index) => {
                let target_pre_cycle_count_sum = self.jit_buf.insts_cycle_counts[target_index] - self.jit_buf.insts[target_index].cycle as u16;

                let total_cycles_reg = block_asm.new_reg();
                block_asm.mov(total_cycles_reg, self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32);
                let target_pre_cycle_count_sum_reg = block_asm.new_reg();
                block_asm.mov(target_pre_cycle_count_sum_reg, target_pre_cycle_count_sum as u32);

                let backed_up_cpsr_reg = block_asm.new_reg();
                block_asm.mrs_cpsr(backed_up_cpsr_reg);

                JitAsmCommonFuns::emit_flush_cycles(
                    self,
                    block_asm,
                    total_cycles_reg,
                    target_pre_cycle_count_sum_reg,
                    false,
                    CPU == ARM9,
                    |_, _, _, _| {},
                    |asm, block_asm, _, _| {
                        if DEBUG_LOG {
                            block_asm.call2(Self::debug_branch_label as *const (), asm.jit_buf.current_pc, target_pc);
                        }
                        block_asm.msr_cpsr(backed_up_cpsr_reg);
                        block_asm.guest_branch(Cond::AL, target_pc & !1);
                    },
                    |_, block_asm| {
                        block_asm.msr_cpsr(backed_up_cpsr_reg);

                        block_asm.mov(Reg::PC, target_pc);
                        block_asm.save_context();
                    },
                    |asm, block_asm, _| {
                        if CPU == ARM7 {
                            block_asm.msr_cpsr(backed_up_cpsr_reg);

                            block_asm.mov(Reg::PC, target_pc);
                            block_asm.save_context();
                        }

                        asm.emit_branch_out_metadata_no_count_cycles(block_asm);
                        block_asm.epilogue();
                    },
                );

                block_asm.free_reg(backed_up_cpsr_reg);
                block_asm.free_reg(target_pre_cycle_count_sum_reg);
                block_asm.free_reg(total_cycles_reg);
            }
            JitBranchInfo::Idle => {
                block_asm.mov(Reg::PC, target_pc);
                block_asm.save_context();
                if DEBUG_LOG {
                    block_asm.call2(Self::debug_idle_loop as *const (), self.jit_buf.current_pc, target_pc);
                }
                self.emit_branch_out_metadata_with_idle_loop(block_asm);
                block_asm.epilogue();
            }
            JitBranchInfo::None => self.emit_branch_external_label(block_asm, target_pc, false, false),
        }
    }

    pub fn emit_bx(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();
        let target_pc_reg = *inst_info.operands()[0].as_reg_no_shift().unwrap();

        if target_pc_reg == Reg::LR {
            block_asm.mov(Reg::PC, target_pc_reg);
            block_asm.save_context();
            self.emit_branch_return_stack_common(block_asm, target_pc_reg.into());
        } else {
            self.emit_branch_reg_common(block_asm, target_pc_reg.into(), false, false);
        }
    }

    pub fn emit_branch_return_stack_common(&mut self, block_asm: &mut BlockAsm, target_pc_reg: BlockReg) {
        self.jit_common_funs
            .emit_call_branch_return_stack(block_asm, self.jit_buf.insts_cycle_counts[self.jit_buf.current_index], target_pc_reg, self.jit_buf.current_pc);
        block_asm.epilogue_previous_block();
    }

    pub fn emit_blx(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();
        let op0 = *inst_info.operands()[0].as_reg_no_shift().unwrap();

        let target_pc_reg = block_asm.new_reg();
        block_asm.mov(target_pc_reg, op0);

        block_asm.mov(Reg::LR, self.jit_buf.current_pc + 4);
        self.emit_branch_reg_common(block_asm, target_pc_reg, true, false);

        block_asm.free_reg(target_pc_reg);
    }

    pub fn emit_branch_external_label(&mut self, block_asm: &mut BlockAsm, target_pc: u32, has_lr_return: bool, thumb: bool) {
        let target_pc = align_guest_pc(target_pc) | (target_pc & 1);

        let target_jit_addr_entry = get_jit!(self.emu).jit_memory_map.get_jit_entry::<CPU>(target_pc);
        if !target_jit_addr_entry.is_null() {
            let target_jit_addr = unsafe { (*target_jit_addr_entry).0 };
            if target_jit_addr != DEFAULT_JIT_ENTRY_ARM9.0 && target_jit_addr != DEFAULT_JIT_ENTRY_ARM7.0 {
                let target_jit_addr_reg = block_asm.new_reg();
                block_asm.mov(target_jit_addr_reg, target_jit_addr as u32);
                block_asm.pli(target_jit_addr_reg, 0, true);
                block_asm.free_reg(target_jit_addr_reg);
            }
        }

        block_asm.mov(Reg::PC, target_pc);
        block_asm.save_context();

        let total_cycles_reg = block_asm.new_reg();
        block_asm.mov(total_cycles_reg, self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32);
        let target_pre_cycle_count_sum_reg = block_asm.new_reg();
        block_asm.mov(target_pre_cycle_count_sum_reg, 0);

        JitAsmCommonFuns::emit_flush_cycles(
            self,
            block_asm,
            total_cycles_reg,
            target_pre_cycle_count_sum_reg,
            true,
            CPU == ARM9,
            |asm, block_asm, runtime_data_addr_reg, breakout_label| {
                JitAsmCommonFuns::<CPU>::emit_check_stack_depth(block_asm, runtime_data_addr_reg, breakout_label, asm.jit_buf.current_pc);
            },
            |asm, block_asm, runtime_data_addr_reg, _| {
                if has_lr_return {
                    JitAsmCommonFuns::<CPU>::emit_return_stack_write_desired_lr(block_asm, runtime_data_addr_reg, Reg::LR.into(), asm.jit_buf.current_pc);
                }

                if DEBUG_LOG {
                    block_asm.call2(JitAsmCommonFuns::<CPU>::debug_branch_reg as *const (), asm.jit_buf.current_pc, target_pc);
                }

                JitAsmCommonFuns::<CPU>::emit_increment_stack_depth(block_asm, runtime_data_addr_reg);
                JitAsmCommonFuns::<CPU>::emit_call_jit_addr_imm(block_asm, asm, target_pc, true);
                JitAsmCommonFuns::<CPU>::emit_decrement_stack_depth(block_asm, runtime_data_addr_reg);

                if has_lr_return {
                    if asm.jit_buf.current_index == asm.jit_buf.insts.len() - 1 {
                        block_asm.mov(total_cycles_reg, 0);
                        block_asm.store_u16(total_cycles_reg, runtime_data_addr_reg, JitRuntimeData::get_pre_cycle_count_sum_offset() as u32);

                        JitAsmCommonFuns::<CPU>::emit_increment_stack_depth(block_asm, runtime_data_addr_reg);
                        JitAsmCommonFuns::<CPU>::emit_call_jit_addr_imm(block_asm, asm, if thumb { (asm.jit_buf.current_pc + 2) | 1 } else { asm.jit_buf.current_pc + 4 }, true);
                        JitAsmCommonFuns::<CPU>::emit_decrement_stack_depth(block_asm, runtime_data_addr_reg);

                        block_asm.epilogue_previous_block();
                    } else {
                        block_asm.store_u16(total_cycles_reg, runtime_data_addr_reg, JitRuntimeData::get_pre_cycle_count_sum_offset() as u32);
                        for reg in Reg::R0 as u8..=Reg::LR as u8 {
                            block_asm.restore_reg(Reg::from(reg));
                        }
                        block_asm.restore_reg(Reg::CPSR);
                    }
                } else {
                    block_asm.epilogue_previous_block();
                }
            },
            |_, _| {},
            |asm, block_asm, _| {
                asm.emit_branch_out_metadata_no_count_cycles(block_asm);
                block_asm.epilogue();
            },
        );

        block_asm.free_reg(target_pre_cycle_count_sum_reg);
        block_asm.free_reg(total_cycles_reg);
    }

    pub fn emit_branch_reg_common(&mut self, block_asm: &mut BlockAsm, target_pc_reg: BlockReg, has_lr_return: bool, thumb: bool) {
        block_asm.mov(Reg::PC, target_pc_reg);
        block_asm.save_context();

        if has_lr_return {
            self.jit_common_funs.emit_call_branch_reg(
                block_asm,
                self.jit_buf.insts_cycle_counts[self.jit_buf.current_index],
                Reg::LR.into(),
                target_pc_reg,
                self.jit_buf.current_pc,
            );
            if self.jit_buf.current_index == self.jit_buf.insts.len() - 1 {
                let total_cycles_reg = block_asm.new_reg();
                let runtime_data_addr_reg = block_asm.new_reg();

                block_asm.mov(total_cycles_reg, 0);
                block_asm.mov(runtime_data_addr_reg, ptr::addr_of_mut!(self.runtime_data) as u32);
                block_asm.store_u16(total_cycles_reg, runtime_data_addr_reg, JitRuntimeData::get_pre_cycle_count_sum_offset() as u32);

                JitAsmCommonFuns::<CPU>::emit_increment_stack_depth(block_asm, runtime_data_addr_reg);
                JitAsmCommonFuns::<CPU>::emit_call_jit_addr_imm(block_asm, self, if thumb { (self.jit_buf.current_pc + 2) | 1 } else { self.jit_buf.current_pc + 4 }, true);
                JitAsmCommonFuns::<CPU>::emit_decrement_stack_depth(block_asm, runtime_data_addr_reg);

                block_asm.epilogue_previous_block();

                block_asm.free_reg(runtime_data_addr_reg);
                block_asm.free_reg(total_cycles_reg);
            } else {
                for reg in Reg::R0 as u8..=Reg::LR as u8 {
                    block_asm.restore_reg(Reg::from(reg));
                }
                block_asm.restore_reg(Reg::CPSR);
            }
        } else {
            let total_cycles_reg = block_asm.new_reg();
            block_asm.mov(total_cycles_reg, self.jit_buf.insts_cycle_counts[self.jit_buf.current_index] as u32);
            let target_pre_cycle_count_sum_reg = block_asm.new_reg();
            block_asm.mov(target_pre_cycle_count_sum_reg, 0);

            JitAsmCommonFuns::emit_flush_cycles(
                self,
                block_asm,
                total_cycles_reg,
                target_pre_cycle_count_sum_reg,
                false,
                CPU == ARM9,
                |asm, block_asm, runtime_data_addr_reg, breakout_label| {
                    JitAsmCommonFuns::<CPU>::emit_check_stack_depth(block_asm, runtime_data_addr_reg, breakout_label, asm.jit_buf.current_pc);
                },
                |asm, block_asm, runtime_data_addr_reg, _| {
                    if DEBUG_LOG {
                        block_asm.call2(Self::debug_branch_reg as *const (), asm.jit_buf.current_pc, target_pc_reg);
                    }

                    JitAsmCommonFuns::<CPU>::emit_increment_stack_depth(block_asm, runtime_data_addr_reg);
                    JitAsmCommonFuns::emit_call_jit_addr(block_asm, asm, target_pc_reg, true);
                    JitAsmCommonFuns::<CPU>::emit_decrement_stack_depth(block_asm, runtime_data_addr_reg);

                    block_asm.epilogue_previous_block();
                },
                |_, _| {},
                |asm, block_asm, _| {
                    asm.emit_branch_out_metadata_no_count_cycles(block_asm);
                    block_asm.epilogue();
                },
            );
            block_asm.free_reg(target_pre_cycle_count_sum_reg);
            block_asm.free_reg(total_cycles_reg);
        }
    }

    pub fn emit_blx_label(&mut self, block_asm: &mut BlockAsm) {
        if CPU != ARM9 {
            return;
        }

        let relative_pc = *self.jit_buf.current_inst().operands()[0].as_imm().unwrap() as i32 + 8;
        let target_pc = (self.jit_buf.current_pc as i32 + relative_pc) as u32;

        block_asm.mov(Reg::LR, self.jit_buf.current_pc + 4);
        self.emit_branch_external_label(block_asm, target_pc | 1, true, false);
    }

    extern "C" fn debug_branch_label(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} branch label from {current_pc:x} to {target_pc:x}")
    }

    extern "C" fn debug_branch_reg(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} branch reg from {current_pc:x} to {target_pc:x}")
    }

    extern "C" fn debug_idle_loop(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} detected idle loop {current_pc:x} to {target_pc:x}")
    }
}
