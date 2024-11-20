use crate::core::emu::{get_jit, get_regs_mut};
use crate::core::CpuType;
use crate::jit::jit_asm::{JitAsm, RETURN_STACK_SIZE};
use crate::jit::jit_asm_common_funs::{exit_guest_context, get_max_loop_cycle_count, JitAsmCommonFuns};
use crate::{get_jit_asm_ptr, DEBUG_LOG, IS_DEBUG};
use std::hint::unreachable_unchecked;
use std::intrinsics::{likely, unlikely};
use std::mem;

unsafe extern "C" fn flush_cycles<const CPU: CpuType>(asm: &mut JitAsm<CPU>, total_cycles: u16, current_pc: u32) {
    asm.runtime_data.accumulated_cycles += total_cycles + 2 - asm.runtime_data.pre_cycle_count_sum;

    if unlikely(asm.runtime_data.accumulated_cycles >= get_max_loop_cycle_count::<CPU>() as u16) {
        if IS_DEBUG {
            asm.runtime_data.branch_out_pc = current_pc;
        }
        exit_guest_context!(asm);
    }
}

#[inline(always)]
unsafe extern "C" fn call_jit_fun<const CPU: CpuType>(asm: &mut JitAsm<CPU>, target_pc: u32) {
    let thumb = target_pc & 1 != 0;
    get_regs_mut!(asm.emu, CPU).set_thumb(thumb);
    let aligned_pc_mask = !(1 | ((!thumb as u32) << 1));
    let aligned_pc = target_pc & aligned_pc_mask;

    let jit_entry = get_jit!(asm.emu).get_jit_start_addr::<CPU>(aligned_pc);
    let jit_entry: extern "C" fn(bool) = mem::transmute(jit_entry);
    jit_entry(false);
}

pub unsafe extern "C" fn branch_reg_with_lr_return<const CPU: CpuType>(total_cycles: u16, lr: u32, target_pc: u32, current_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();

    flush_cycles(asm, total_cycles, current_pc);

    asm.runtime_data.pre_cycle_count_sum = 0;
    let return_stack_ptr = asm.runtime_data.return_stack_ptr & (RETURN_STACK_SIZE as u8 - 1);
    *asm.runtime_data.return_stack.get_unchecked_mut(return_stack_ptr as usize) = lr;
    asm.runtime_data.return_stack_ptr = return_stack_ptr + 1;

    if DEBUG_LOG {
        JitAsmCommonFuns::<CPU>::debug_push_return_stack(current_pc, lr, return_stack_ptr);
        JitAsmCommonFuns::<CPU>::debug_branch_reg(current_pc, target_pc);
    }

    call_jit_fun(asm, target_pc);
    asm.runtime_data.pre_cycle_count_sum = total_cycles;
}

pub unsafe extern "C" fn branch_lr<const CPU: CpuType>(total_cycles: u16, target_pc: u32, current_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();

    flush_cycles(asm, total_cycles, current_pc);

    if unlikely(asm.runtime_data.return_stack_ptr == 0) {
        if DEBUG_LOG {
            JitAsmCommonFuns::<CPU>::debug_return_stack_empty(current_pc, target_pc);
        }
        asm.runtime_data.pre_cycle_count_sum = 0;
        call_jit_fun(asm, target_pc);
        unsafe { unreachable_unchecked() };
    } else {
        asm.runtime_data.return_stack_ptr -= 1;
        let desired_lr = *asm.runtime_data.return_stack.get_unchecked(asm.runtime_data.return_stack_ptr as usize);
        if likely(desired_lr == target_pc) {
            get_regs_mut!(asm.emu, CPU).set_thumb(target_pc & 1 == 1);
            if DEBUG_LOG {
                JitAsmCommonFuns::<CPU>::debug_branch_lr(current_pc, target_pc);
            }
        } else {
            if DEBUG_LOG {
                JitAsmCommonFuns::<CPU>::debug_branch_lr_failed(current_pc, target_pc);
            }
            asm.runtime_data.pre_cycle_count_sum = 0;
            asm.runtime_data.return_stack_ptr = 0;
            call_jit_fun(asm, target_pc);
            unsafe { unreachable_unchecked() };
        }
    }
}
