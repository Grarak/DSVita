use crate::core::emu::{get_jit, get_regs_mut};
use crate::core::CpuType;
use crate::jit::jit_asm::RETURN_STACK_SIZE;
use crate::jit::jit_asm_common_funs::{exit_guest_context, JitAsmCommonFuns, MAX_LOOP_CYCLE_COUNT};
use crate::{get_jit_asm_ptr, DEBUG_LOG, IS_DEBUG};
use std::intrinsics::unlikely;
use std::mem;
use CpuType::{ARM7, ARM9};

pub unsafe extern "C" fn branch_reg<const CPU: CpuType>(total_cycles: u16, lr: u32, target_pc: u32, current_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    asm.runtime_data.accumulated_cycles += total_cycles + 2 - asm.runtime_data.pre_cycle_count_sum;
    let max_count = match CPU {
        ARM9 => MAX_LOOP_CYCLE_COUNT * 2,
        ARM7 => MAX_LOOP_CYCLE_COUNT,
    };

    if unlikely(asm.runtime_data.accumulated_cycles >= max_count as u16) {
        if IS_DEBUG {
            asm.runtime_data.branch_out_pc = current_pc;
        }
        exit_guest_context!(asm);
    } else {
        asm.runtime_data.pre_cycle_count_sum = 0;
        let return_stack_ptr = asm.runtime_data.return_stack_ptr & (RETURN_STACK_SIZE as u8 - 1);
        *asm.runtime_data.return_stack.get_unchecked_mut(return_stack_ptr as usize) = lr;
        asm.runtime_data.return_stack_ptr = return_stack_ptr + 1;

        if DEBUG_LOG {
            JitAsmCommonFuns::<CPU>::debug_push_return_stack(current_pc, lr, return_stack_ptr);
            JitAsmCommonFuns::<CPU>::debug_branch_reg(current_pc, target_pc);
        }

        let thumb = target_pc & 1 != 0;
        get_regs_mut!(asm.emu, CPU).set_thumb(thumb);
        let aligned_pc_mask = !(1 | ((!thumb as u32) << 1));
        let aligned_pc = target_pc & aligned_pc_mask;

        let jit_entry = get_jit!(asm.emu).get_jit_start_addr::<CPU>(aligned_pc);
        let jit_entry: extern "C" fn(bool) = unsafe { mem::transmute(jit_entry) };
        jit_entry(false);

        asm.runtime_data.pre_cycle_count_sum = total_cycles;
    }
}
