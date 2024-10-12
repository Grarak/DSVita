use crate::core::emu::{get_jit, get_jit_mut, get_regs_mut};
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::{BlockLabel, BlockOperand, BlockReg};
use crate::jit::jit_asm::{JitAsm, JitRuntimeData, RETURN_STACK_SIZE};
use crate::jit::reg::Reg;
use crate::jit::{jit_memory_map, Cond, ShiftType};
use crate::{DEBUG_LOG, IS_DEBUG};
use std::ptr;

pub struct JitAsmCommonFuns<const CPU: CpuType> {
    branch_return_stack: usize,
    branch_reg: usize,
}

impl<const CPU: CpuType> Default for JitAsmCommonFuns<CPU> {
    fn default() -> Self {
        JitAsmCommonFuns {
            branch_return_stack: 0,
            branch_reg: 0,
        }
    }
}

impl<const CPU: CpuType> JitAsmCommonFuns<CPU> {
    pub fn new(asm: &mut JitAsm<CPU>) -> Self {
        let mut create_function = |fun: fn(&mut BlockAsm, &mut JitAsm<CPU>)| {
            let mut block_asm = asm.new_block_asm(true);
            fun(&mut block_asm, asm);
            block_asm.emit_opcodes(0, false);
            let opcodes = block_asm.finalize(0);
            get_jit_mut!(asm.emu).insert_common_fun_block(opcodes) as usize - get_jit!(asm.emu).get_start_entry()
        };
        JitAsmCommonFuns {
            branch_return_stack: create_function(Self::emit_branch_return_stack),
            branch_reg: create_function(Self::emit_branch_reg),
        }
    }

    pub fn emit_call_jit_addr(block_asm: &mut BlockAsm, asm: &mut JitAsm<CPU>, target_pc_reg: BlockReg, has_return: bool) {
        Self::emit_set_cpsr_thumb_bit(block_asm, asm, target_pc_reg);

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

        if has_return {
            block_asm.call1(entry_fn_reg, 0);
        } else {
            block_asm.call1_no_return(entry_fn_reg, 0);
        }

        block_asm.free_reg(entry_fn_reg);
        block_asm.free_reg(map_entry_base_ptr_reg);
        block_asm.free_reg(map_index_reg);
        block_asm.free_reg(map_ptr_reg);
        block_asm.free_reg(aligned_target_reg);
    }

    fn emit_set_cpsr_thumb_bit(block_asm: &mut BlockAsm, asm: &mut JitAsm<CPU>, guest_pc_reg: BlockReg) {
        let thread_regs_addr_reg = block_asm.new_reg();
        block_asm.mov(thread_regs_addr_reg, get_regs_mut!(asm.emu, CPU).get_reg_mut_ptr() as u32);
        let cpsr_reg = block_asm.new_reg();
        block_asm.load_u32(cpsr_reg, thread_regs_addr_reg, Reg::CPSR as u32 * 4);
        block_asm.bfi(cpsr_reg, guest_pc_reg, 5, 1);
        block_asm.store_u32(cpsr_reg, thread_regs_addr_reg, Reg::CPSR as u32 * 4);
        block_asm.free_reg(cpsr_reg);
        block_asm.free_reg(thread_regs_addr_reg);
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

    fn emit_count_cycles(block_asm: &mut BlockAsm, total_cycles_reg: BlockReg, runtime_data_addr_reg: BlockReg, result_accumulated_cycles_reg: BlockReg) {
        let pre_cycle_count_sum_reg = block_asm.new_reg();
        block_asm.load_u16(pre_cycle_count_sum_reg, runtime_data_addr_reg, JitRuntimeData::get_pre_cycle_count_sum_offset() as u32);

        let accumulated_cycles_reg = block_asm.new_reg();
        block_asm.load_u16(accumulated_cycles_reg, runtime_data_addr_reg, JitRuntimeData::get_accumulated_cycles_offset() as u32);

        // +2 for branching
        block_asm.add(result_accumulated_cycles_reg, accumulated_cycles_reg, total_cycles_reg);
        block_asm.add(result_accumulated_cycles_reg, result_accumulated_cycles_reg, 2);
        block_asm.sub(result_accumulated_cycles_reg, result_accumulated_cycles_reg, pre_cycle_count_sum_reg);

        block_asm.store_u16(result_accumulated_cycles_reg, runtime_data_addr_reg, JitRuntimeData::get_accumulated_cycles_offset() as u32);

        block_asm.free_reg(accumulated_cycles_reg);
        block_asm.free_reg(pre_cycle_count_sum_reg);
    }

    pub fn emit_flush_cycles<ContinueFn: Fn(&mut JitAsm<CPU>, &mut BlockAsm, BlockReg, BlockLabel), BreakoutFn: Fn(&mut JitAsm<CPU>, &mut BlockAsm, BlockReg)>(
        asm: &mut JitAsm<CPU>,
        block_asm: &mut BlockAsm,
        total_cycles_reg: BlockReg,
        target_pre_cycle_count_sum_reg: BlockReg,
        add_continue_label: bool,
        continue_fn: ContinueFn,
        breakout_fn: BreakoutFn,
    ) {
        let runtime_data_addr_reg = block_asm.new_reg();
        block_asm.mov(runtime_data_addr_reg, ptr::addr_of_mut!(asm.runtime_data) as u32);

        let result_accumulated_cycles_reg = block_asm.new_reg();
        Self::emit_count_cycles(block_asm, total_cycles_reg, runtime_data_addr_reg, result_accumulated_cycles_reg);

        const MAX_LOOP_CYCLE_COUNT: u32 = 255;
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
        block_asm.store_u16(target_pre_cycle_count_sum_reg, runtime_data_addr_reg, JitRuntimeData::get_pre_cycle_count_sum_offset() as u32);

        continue_fn(asm, block_asm, runtime_data_addr_reg, breakout_label);
        if add_continue_label {
            block_asm.branch(continue_label.unwrap(), Cond::AL);
        }

        block_asm.label(breakout_label);
        breakout_fn(asm, block_asm, runtime_data_addr_reg);

        if add_continue_label {
            block_asm.label(continue_label.unwrap());
        }

        block_asm.free_reg(result_accumulated_cycles_reg);
        block_asm.free_reg(runtime_data_addr_reg);
    }

    fn emit_branch_return_stack(block_asm: &mut BlockAsm, asm: &mut JitAsm<CPU>) {
        // args: total_cycles, target_pc, current_pc

        let total_cycles_reg = block_asm.new_reg();
        block_asm.mov(total_cycles_reg, BlockReg::Fixed(Reg::R0));

        let target_pc_reg = block_asm.new_reg();
        block_asm.mov(target_pc_reg, BlockReg::Fixed(Reg::R1));

        let current_pc_reg = block_asm.new_reg();
        if IS_DEBUG {
            block_asm.mov(current_pc_reg, BlockReg::Fixed(Reg::R2));
        }

        let target_pre_cycle_count_sum_reg = block_asm.new_reg();
        block_asm.mov(target_pre_cycle_count_sum_reg, 0);

        Self::emit_flush_cycles(
            asm,
            block_asm,
            total_cycles_reg,
            target_pre_cycle_count_sum_reg,
            false,
            |asm, block_asm, runtime_data_addr_reg, breakout_label| {
                let aligned_target_pc_reg = block_asm.new_reg();
                let thumb_bit_mask_reg = block_asm.new_reg();
                block_asm.and(thumb_bit_mask_reg, target_pc_reg, 1);
                Self::emit_align_guest_pc(block_asm, target_pc_reg, aligned_target_pc_reg);
                block_asm.orr(aligned_target_pc_reg, aligned_target_pc_reg, thumb_bit_mask_reg);

                let no_return_label = block_asm.new_label();
                let return_stack_ptr_reg = block_asm.new_reg();

                block_asm.load_u8(return_stack_ptr_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_ptr_offset() as u32);
                block_asm.cmp(return_stack_ptr_reg, 0);
                if DEBUG_LOG {
                    block_asm.start_cond_block(Cond::EQ);
                    block_asm.call2(Self::debug_return_stack_empty as *const (), current_pc_reg, aligned_target_pc_reg);
                    block_asm.branch(no_return_label, Cond::AL);
                    block_asm.end_cond_block();
                } else {
                    block_asm.branch(no_return_label, Cond::EQ);
                }

                block_asm.sub(return_stack_ptr_reg, return_stack_ptr_reg, 1);

                let return_stack_reg = block_asm.new_reg();
                block_asm.add(return_stack_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_offset() as u32);

                let desired_lr_reg = block_asm.new_reg();
                block_asm.load_u32(desired_lr_reg, return_stack_reg, (return_stack_ptr_reg.into(), ShiftType::Lsl, BlockOperand::from(2)));

                let no_return_store_label = block_asm.new_label();
                block_asm.cmp(desired_lr_reg, aligned_target_pc_reg);
                block_asm.branch(no_return_store_label, Cond::NE);

                block_asm.store_u8(return_stack_ptr_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_ptr_offset() as u32);

                Self::emit_set_cpsr_thumb_bit(block_asm, asm, aligned_target_pc_reg);
                if DEBUG_LOG {
                    block_asm.call2(Self::debug_branch_lr as *const (), current_pc_reg, aligned_target_pc_reg);
                }
                block_asm.epilogue_previous_block();

                block_asm.label(no_return_store_label);
                if DEBUG_LOG {
                    block_asm.call2(Self::debug_branch_lr_failed as *const (), current_pc_reg, aligned_target_pc_reg);
                }
                block_asm.mov(return_stack_ptr_reg, 0);
                block_asm.store_u8(return_stack_ptr_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_ptr_offset() as u32);

                block_asm.label(no_return_label);
                Self::emit_call_jit_addr(block_asm, asm, aligned_target_pc_reg, false);

                block_asm.free_reg(aligned_target_pc_reg);
                block_asm.free_reg(desired_lr_reg);
                block_asm.free_reg(return_stack_reg);
                block_asm.free_reg(return_stack_ptr_reg);
                block_asm.free_reg(thumb_bit_mask_reg);
            },
            |_, block_asm, runtime_data_addr_reg| {
                if IS_DEBUG {
                    block_asm.store_u32(current_pc_reg, runtime_data_addr_reg, JitRuntimeData::get_out_pc_offset() as u32);
                }
                block_asm.epilogue();
            },
        );

        block_asm.free_reg(current_pc_reg);
        block_asm.free_reg(target_pc_reg);
        block_asm.free_reg(total_cycles_reg);
    }

    fn emit_return_stack_write_desired_lr(block_asm: &mut BlockAsm, runtime_data_addr_reg: BlockReg, lr_reg: BlockReg, current_pc_reg: BlockReg) {
        let return_stack_ptr_reg = block_asm.new_reg();

        block_asm.load_u8(return_stack_ptr_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_ptr_offset() as u32);
        block_asm.and(return_stack_ptr_reg, return_stack_ptr_reg, RETURN_STACK_SIZE as u32 - 1);

        let return_stack_reg = block_asm.new_reg();
        block_asm.add(return_stack_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_offset() as u32);
        block_asm.store_u32(lr_reg, return_stack_reg, (return_stack_ptr_reg.into(), ShiftType::Lsl, BlockOperand::from(2)));

        if DEBUG_LOG {
            block_asm.call3(Self::debug_push_return_stack as *const (), current_pc_reg, lr_reg, return_stack_ptr_reg);
        }

        block_asm.add(return_stack_ptr_reg, return_stack_ptr_reg, 1);
        block_asm.store_u8(return_stack_ptr_reg, runtime_data_addr_reg, JitRuntimeData::get_return_stack_ptr_offset() as u32);

        block_asm.free_reg(return_stack_reg);
        block_asm.free_reg(return_stack_ptr_reg);
    }

    fn emit_branch_reg(block_asm: &mut BlockAsm, asm: &mut JitAsm<CPU>) {
        // args: total_cycles, lr_reg, target_pc, current_pc

        let total_cycles_reg = block_asm.new_reg();
        block_asm.mov(total_cycles_reg, BlockReg::Fixed(Reg::R0));

        let lr_reg = block_asm.new_reg();
        block_asm.mov(lr_reg, BlockReg::Fixed(Reg::R1));

        let target_pc_reg = block_asm.new_reg();
        block_asm.mov(target_pc_reg, BlockReg::Fixed(Reg::R2));

        let current_pc_reg = block_asm.new_reg();
        if IS_DEBUG {
            block_asm.mov(current_pc_reg, BlockReg::Fixed(Reg::R3));
        }

        let target_pre_cycle_count_sum_reg = block_asm.new_reg();
        block_asm.mov(target_pre_cycle_count_sum_reg, 0);

        Self::emit_flush_cycles(
            asm,
            block_asm,
            total_cycles_reg,
            target_pre_cycle_count_sum_reg,
            false,
            |asm, block_asm, runtime_data_addr_reg, _| {
                Self::emit_return_stack_write_desired_lr(block_asm, runtime_data_addr_reg, lr_reg, current_pc_reg);
                if DEBUG_LOG {
                    block_asm.call2(Self::debug_branch_reg as *const (), current_pc_reg, target_pc_reg);
                }

                JitAsmCommonFuns::emit_call_jit_addr(block_asm, asm, target_pc_reg, true);
                block_asm.store_u16(total_cycles_reg, runtime_data_addr_reg, JitRuntimeData::get_pre_cycle_count_sum_offset() as u32);
                block_asm.epilogue_previous_block();
            },
            |_, block_asm, runtime_data_addr_reg| {
                if IS_DEBUG {
                    block_asm.store_u32(current_pc_reg, runtime_data_addr_reg, JitRuntimeData::get_out_pc_offset() as u32);
                }
                block_asm.epilogue();
            },
        );

        block_asm.free_reg(target_pre_cycle_count_sum_reg);
        block_asm.free_reg(current_pc_reg);
        block_asm.free_reg(target_pc_reg);
        block_asm.free_reg(lr_reg);
        block_asm.free_reg(total_cycles_reg);
    }

    pub fn emit_call_branch_return_stack(&self, block_asm: &mut BlockAsm, total_cycles: u16, target_pc_reg: BlockReg, current_pc: u32) {
        if IS_DEBUG {
            block_asm.call3_common(self.branch_return_stack, total_cycles as u32, target_pc_reg, current_pc);
        } else {
            block_asm.call2_common(self.branch_return_stack, total_cycles as u32, target_pc_reg);
        }
    }

    pub fn emit_call_branch_reg(&self, block_asm: &mut BlockAsm, total_cycles: u16, lr_reg: BlockReg, target_pc_reg: BlockReg, current_pc: u32) {
        if IS_DEBUG {
            block_asm.call4_common(self.branch_reg, total_cycles as u32, lr_reg, target_pc_reg, current_pc);
        } else {
            block_asm.call3_common(self.branch_reg, total_cycles as u32, lr_reg, target_pc_reg);
        }
    }

    extern "C" fn debug_push_return_stack(current_pc: u32, lr_pc: u32, stack_size: u8) {
        println!("{CPU:?} push {lr_pc:x} to return stack with size {stack_size} at {current_pc:x}")
    }

    extern "C" fn debug_branch_reg(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} branch reg from {current_pc:x} to {target_pc:x}")
    }

    extern "C" fn debug_branch_lr(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} branch lr from {current_pc:x} to {target_pc:x}")
    }

    extern "C" fn debug_branch_lr_failed(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} failed to branch lr from {current_pc:x} to {target_pc:x}")
    }

    extern "C" fn debug_return_stack_empty(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} empty return stack {current_pc:x} to {target_pc:x}")
    }
}
