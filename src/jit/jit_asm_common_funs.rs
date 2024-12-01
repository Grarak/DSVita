use crate::core::emu::{get_jit, get_regs_mut};
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::block_asm::BlockAsm;
use crate::jit::assembler::{BlockLabel, BlockOperand, BlockReg};
use crate::jit::jit_asm::{JitAsm, JitRuntimeData};
use crate::jit::reg::Reg;
use crate::jit::{jit_memory_map, Cond, ShiftType};
use crate::IS_DEBUG;
use std::ptr;

const MAX_LOOP_CYCLE_COUNT: u32 = 255;

pub const fn get_max_loop_cycle_count<const CPU: CpuType>() -> u32 {
    match CPU {
        ARM9 => MAX_LOOP_CYCLE_COUNT * 2,
        ARM7 => MAX_LOOP_CYCLE_COUNT,
    }
}

macro_rules! exit_guest_context {
    ($asm:expr) => {{
        // r4-r12,pc since we need an even amount of registers for 8 byte alignment, in case the compiler decides to use neon instructions
        std::arch::asm!(
            "mov sp, {}",
            "pop {{r4-r12,pc}}",
            in(reg) $asm.runtime_data.host_sp
        );
        std::hint::unreachable_unchecked();
    }};
}

use crate::jit::inst_branch_handler::{branch_lr, branch_reg_with_lr_return};
pub(crate) use exit_guest_context;

pub struct JitAsmCommonFuns<const CPU: CpuType> {}

impl<const CPU: CpuType> Default for JitAsmCommonFuns<CPU> {
    fn default() -> Self {
        JitAsmCommonFuns {}
    }
}

impl<const CPU: CpuType> JitAsmCommonFuns<CPU> {
    pub fn new(asm: &mut JitAsm<CPU>) -> Self {
        JitAsmCommonFuns {}
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
        const ADDR_SHIFT: u8 = jit_memory_map::BLOCK_SHIFT as u8 + 1;
        block_asm.ubfx(map_index_reg, aligned_target_reg, ADDR_SHIFT, 28 - ADDR_SHIFT);
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

    pub fn emit_call_branch_return_stack(&self, block_asm: &mut BlockAsm, total_cycles: u16, target_pc_reg: BlockReg, current_pc: u32) {
        if IS_DEBUG {
            block_asm.call4(branch_lr::<CPU, true> as *const (), total_cycles as u32, target_pc_reg, 0, current_pc);
        } else {
            block_asm.call3(branch_lr::<CPU, true> as *const (), total_cycles as u32, target_pc_reg, 0);
        }
    }

    pub fn emit_call_branch_reg(&self, block_asm: &mut BlockAsm, total_cycles: u16, lr_reg: BlockReg, target_pc_reg: BlockReg, current_pc: u32) {
        if IS_DEBUG {
            block_asm.call4(branch_reg_with_lr_return::<CPU> as *const (), total_cycles as u32, lr_reg, target_pc_reg, current_pc);
        } else {
            block_asm.call3(branch_reg_with_lr_return::<CPU> as *const (), total_cycles as u32, lr_reg, target_pc_reg);
        }
    }

    pub extern "C" fn debug_push_return_stack(current_pc: u32, lr_pc: u32, stack_size: u8) {
        println!("{CPU:?} push {lr_pc:x} to return stack with size {stack_size} at {current_pc:x}")
    }

    pub extern "C" fn debug_branch_reg(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} branch reg from {current_pc:x} to {target_pc:x}")
    }

    pub extern "C" fn debug_branch_lr(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} branch lr from {current_pc:x} to {target_pc:x}")
    }

    pub extern "C" fn debug_branch_lr_failed(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} failed to branch lr from {current_pc:x} to {target_pc:x}")
    }

    pub extern "C" fn debug_return_stack_empty(current_pc: u32, target_pc: u32) {
        println!("{CPU:?} empty return stack {current_pc:x} to {target_pc:x}")
    }
}
