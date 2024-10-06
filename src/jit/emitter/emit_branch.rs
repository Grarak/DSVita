use crate::core::emu::get_jit;
use crate::core::CpuType;
use crate::core::CpuType::ARM9;
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::{BlockOperand, BlockReg};
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm::{JitAsm, JitRuntimeData, RETURN_STACK_SIZE};
use crate::jit::op::Op;
use crate::jit::reg::{reg_reserve, Reg, RegReserve};
use crate::jit::{jit_memory_map, Cond, ShiftType};
use crate::DEBUG_LOG;

pub enum JitBranchInfo {
    Idle,
    Local(usize),
    None,
}

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
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

            if (out_regs & regs_disallowed_to_write).len() != 0 {
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
            let target_pc_reg = block_asm.new_reg();
            block_asm.mov(target_pc_reg, target_pc);
            self.emit_branch_reg_common(block_asm, target_pc_reg, true);
            block_asm.free_reg(target_pc_reg);
        } else {
            self.emit_branch_label_common::<false>(block_asm, target_pc, inst_info.cond);
        }
    }

    pub fn emit_branch_label_common<const THUMB: bool>(&mut self, block_asm: &mut BlockAsm, target_pc: u32, cond: Cond) {
        match Self::analyze_branch_label::<THUMB>(&self.jit_buf.insts, self.jit_buf.current_index, cond, self.jit_buf.current_pc, target_pc) {
            JitBranchInfo::Local(target_index) => {
                let target_pre_cycle_count_sum = self.jit_buf.insts_cycle_counts[target_index] - self.jit_buf.insts[target_index].cycle as u16;

                let backed_up_cpsr_reg = block_asm.new_reg();
                block_asm.mrs_cpsr(backed_up_cpsr_reg);

                self.emit_flush_cycles(
                    block_asm,
                    Some(target_pre_cycle_count_sum),
                    false,
                    |asm, block_asm, _, _| {
                        if DEBUG_LOG {
                            block_asm.call2(Self::debug_branch_label as *const (), asm.jit_buf.current_pc, target_pc);
                        }
                        block_asm.msr_cpsr(backed_up_cpsr_reg);
                        block_asm.guest_branch(Cond::AL, target_pc & !1);
                    },
                    |asm, block_asm| {
                        block_asm.msr_cpsr(backed_up_cpsr_reg);

                        block_asm.mov(Reg::PC, target_pc);
                        block_asm.save_context();
                        asm.emit_branch_out_metadata_no_count_cycles(block_asm);
                        block_asm.epilogue();
                    },
                );

                block_asm.free_reg(backed_up_cpsr_reg);
            }
            JitBranchInfo::Idle => {
                block_asm.mov(Reg::PC, target_pc);
                block_asm.save_context();
                self.emit_branch_out_metadata_with_idle_loop(block_asm);
                block_asm.epilogue();
            }
            JitBranchInfo::None => {
                let target_pc_reg = block_asm.new_reg();
                block_asm.mov(target_pc_reg, target_pc);
                self.emit_branch_reg_common(block_asm, target_pc_reg, false);
                block_asm.free_reg(target_pc_reg);
            }
        }
    }

    pub fn emit_bx(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();
        let target_pc_reg = *inst_info.operands()[0].as_reg_no_shift().unwrap();

        block_asm.mov(Reg::PC, target_pc_reg);
        block_asm.save_context();

        if target_pc_reg == Reg::LR {
            self.emit_branch_return_stack_common(block_asm, target_pc_reg.into());
        } else {
            self.emit_branch_out_metadata(block_asm);
            block_asm.epilogue();
        }
    }

    pub fn emit_branch_return_stack_common(&mut self, block_asm: &mut BlockAsm, target_pc_reg: BlockReg) {
        self.emit_flush_cycles(
            block_asm,
            None,
            false,
            |asm, block_asm, runtime_data_addr_reg, breakout_label| {
                let return_stack_ptr_reg = block_asm.new_reg();

                // block_asm.bkpt(1);

                block_asm.load_u8(return_stack_ptr_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_ptr_offset() as u32);
                block_asm.cmp(return_stack_ptr_reg, 0);
                block_asm.branch(breakout_label, Cond::EQ);

                block_asm.sub(return_stack_ptr_reg, return_stack_ptr_reg, 1);

                let return_stack_reg = block_asm.new_reg();
                block_asm.add(return_stack_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_offset() as u32);

                let desired_lr_reg = block_asm.new_reg();
                block_asm.load_u32(desired_lr_reg, return_stack_reg, (return_stack_ptr_reg.into(), ShiftType::Lsl, BlockOperand::from(2)));

                let aligned_target_pc_reg = block_asm.new_reg();
                let thumb_bit_mask_reg = block_asm.new_reg();
                block_asm.and(thumb_bit_mask_reg, target_pc_reg, 1);
                Self::emit_align_guest_pc(block_asm, target_pc_reg, aligned_target_pc_reg);
                block_asm.orr(aligned_target_pc_reg, aligned_target_pc_reg, thumb_bit_mask_reg);

                block_asm.cmp(desired_lr_reg, aligned_target_pc_reg);
                block_asm.branch(breakout_label, Cond::NE);

                block_asm.store_u8(return_stack_ptr_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_ptr_offset() as u32);

                Self::emit_set_cpsr_thumb_bit(block_asm, aligned_target_pc_reg);

                if DEBUG_LOG {
                    block_asm.call2(Self::debug_branch_lr as *const (), asm.jit_buf.current_pc, aligned_target_pc_reg);
                }

                block_asm.epilogue_previous_block();

                block_asm.free_reg(thumb_bit_mask_reg);
                block_asm.free_reg(aligned_target_pc_reg);
                block_asm.free_reg(desired_lr_reg);
                block_asm.free_reg(return_stack_reg);
                block_asm.free_reg(return_stack_ptr_reg);
            },
            |asm, block_asm| {
                asm.emit_branch_out_metadata(block_asm);
                block_asm.epilogue();
            },
        )
    }

    pub fn emit_blx(&mut self, block_asm: &mut BlockAsm) {
        let inst_info = self.jit_buf.current_inst();
        let target_pc_reg = *inst_info.operands()[0].as_reg_no_shift().unwrap();

        block_asm.mov(Reg::LR, self.jit_buf.current_pc + 4);
        self.emit_branch_reg_common(block_asm, target_pc_reg.into(), true);
    }

    fn emit_return_stack_write_desired_lr(&mut self, block_asm: &mut BlockAsm, runtime_data_addr_reg: BlockReg) {
        let return_stack_ptr_reg = block_asm.new_reg();

        block_asm.load_u8(return_stack_ptr_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_ptr_offset() as u32);
        block_asm.and(return_stack_ptr_reg, return_stack_ptr_reg, RETURN_STACK_SIZE as u32 - 1);

        let return_stack_reg = block_asm.new_reg();
        block_asm.add(return_stack_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_offset() as u32);
        block_asm.store_u32(Reg::LR, return_stack_reg, (return_stack_ptr_reg.into(), ShiftType::Lsl, BlockOperand::from(2)));

        if DEBUG_LOG {
            block_asm.call3(Self::debug_push_return_stack as *const (), self.jit_buf.current_pc, Reg::LR, return_stack_ptr_reg);
        }

        block_asm.add(return_stack_ptr_reg, return_stack_ptr_reg, 1);
        block_asm.store_u8(return_stack_ptr_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_ptr_offset() as u32);

        block_asm.free_reg(return_stack_reg);
        block_asm.free_reg(return_stack_ptr_reg);
    }

    fn emit_return_write_pre_cycle_count_sum(&mut self, block_asm: &mut BlockAsm, runtime_data_addr_reg: BlockReg) {
        let total_cycles = self.jit_buf.insts_cycle_counts[self.jit_buf.current_index];
        let total_cycles_reg = block_asm.new_reg();
        block_asm.mov(total_cycles_reg, total_cycles as u32);
        block_asm.store_u16(total_cycles_reg, runtime_data_addr_reg, JitRuntimeData::get_pre_cycle_count_sum_offset() as u32);
        block_asm.free_reg(total_cycles_reg);
    }

    pub fn emit_branch_reg_common(&mut self, block_asm: &mut BlockAsm, target_pc_reg: BlockReg, has_lr_return: bool) {
        block_asm.mov(Reg::PC, target_pc_reg);
        block_asm.save_context();

        self.emit_flush_cycles(
            block_asm,
            Some(0),
            true,
            |asm, block_asm, runtime_data_addr_reg, _| {
                if has_lr_return {
                    asm.emit_return_stack_write_desired_lr(block_asm, runtime_data_addr_reg);
                }

                Self::emit_set_cpsr_thumb_bit(block_asm, target_pc_reg);

                if DEBUG_LOG {
                    // block_asm.call2(Self::debug_branch_reg as *const (), asm.jit_buf.current_pc, target_pc_reg);
                }

                let aligned_target_reg = block_asm.new_reg();
                Self::emit_align_guest_pc(block_asm, target_pc_reg, aligned_target_reg);

                let map_ptr = get_jit!(asm.emu).jit_memory_map.get_map_ptr::<CPU>();

                let map_ptr_reg = block_asm.new_reg();
                let map_index_reg = block_asm.new_reg();
                let map_entry_base_ptr_reg = block_asm.new_reg();

                block_asm.mov(map_ptr_reg, map_ptr as u32);
                block_asm.mov(map_index_reg, (aligned_target_reg.into(), ShiftType::Lsr, BlockOperand::from(jit_memory_map::BLOCK_SHIFT as u32 + 1)));
                block_asm.load_u32(map_entry_base_ptr_reg, map_ptr_reg, (map_index_reg.into(), ShiftType::Lsl, BlockOperand::from(2)));
                let block_size_mask_reg = map_index_reg;
                block_asm.mov(block_size_mask_reg, (jit_memory_map::BLOCK_SIZE as u32 - 1) << 2);
                block_asm.and(aligned_target_reg, block_size_mask_reg, (aligned_target_reg.into(), ShiftType::Lsl, BlockOperand::from(1)));

                let entry_fn_reg = block_asm.new_reg();
                block_asm.load_u32(entry_fn_reg, map_entry_base_ptr_reg, aligned_target_reg);

                block_asm.call1(entry_fn_reg, 0);
                if has_lr_return {
                    asm.emit_return_write_pre_cycle_count_sum(block_asm, runtime_data_addr_reg);
                    block_asm.restore_reg(Reg::CPSR);
                } else {
                    block_asm.epilogue_previous_block();
                }

                block_asm.free_reg(entry_fn_reg);
                block_asm.free_reg(map_entry_base_ptr_reg);
                block_asm.free_reg(map_index_reg);
                block_asm.free_reg(map_ptr_reg);
                block_asm.free_reg(aligned_target_reg);
            },
            |asm, block_asm| {
                asm.emit_branch_out_metadata_no_count_cycles(block_asm);
                block_asm.epilogue();
            },
        );
    }

    pub fn emit_blx_label(&mut self, block_asm: &mut BlockAsm) {
        if CPU != ARM9 {
            return;
        }

        let relative_pc = *self.jit_buf.current_inst().operands()[0].as_imm().unwrap() as i32 + 8;
        let target_pc = (self.jit_buf.current_pc as i32 + relative_pc) as u32;

        let target_pc_reg = block_asm.new_reg();
        block_asm.mov(target_pc_reg, target_pc | 1);

        block_asm.mov(Reg::LR, self.jit_buf.current_pc + 4);
        self.emit_branch_reg_common(block_asm, target_pc_reg, true);

        block_asm.free_reg(target_pc_reg);
    }

    fn emit_set_cpsr_thumb_bit(block_asm: &mut BlockAsm, guest_pc_reg: BlockReg) {
        let cpsr_reg = block_asm.new_reg();
        block_asm.load_u32(cpsr_reg, block_asm.thread_regs_addr_reg, Reg::CPSR as u32 * 4);
        block_asm.bfi(cpsr_reg, guest_pc_reg, 5, 1);
        block_asm.store_u32(cpsr_reg, block_asm.thread_regs_addr_reg, Reg::CPSR as u32 * 4);
        block_asm.free_reg(cpsr_reg);
    }

    fn emit_align_guest_pc(block_asm: &mut BlockAsm, guest_pc_reg: BlockReg, aligned_guest_pc_reg: BlockReg) {
        // Align pc to 2 or 4
        // let thumb = (guest_pc & 1) == 1;
        // let addr_mask = !(1 | ((!thumb as u32) << 1));
        // let aligned_guest_pc = guest_pc & addr_mask;

        let addr_mask_reg = block_asm.new_reg();
        block_asm.mvn(addr_mask_reg, 3);
        block_asm.orr(addr_mask_reg, addr_mask_reg, (guest_pc_reg.into(), ShiftType::Lsl, BlockOperand::from(1)));
        block_asm.and(aligned_guest_pc_reg, guest_pc_reg, addr_mask_reg);
        block_asm.free_reg(addr_mask_reg);
    }

    extern "C" fn debug_push_return_stack(current_pc: u32, lr_pc: u32, stack_size: u8) {
        println!("{CPU:?} push {lr_pc:x} to return stack with size {stack_size} at {current_pc:x}")
    }

    extern "C" fn debug_branch_label(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} branch label from {current_pc:x} to {target_pc:x}")
    }

    extern "C" fn debug_branch_reg(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} branch reg from {current_pc:x} to {target_pc:x}")
    }

    extern "C" fn debug_branch_lr(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} branch lr from {current_pc:x} to {target_pc:x}")
    }
}
