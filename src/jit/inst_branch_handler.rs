use crate::core::emu::{get_cm_mut, get_common_mut, get_cpu_regs, get_jit, get_regs, get_regs_mut};
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::jit_asm::{align_guest_pc, JitAsm, RETURN_STACK_SIZE, STACK_DEPTH_LIMIT};
use crate::jit::jit_asm_common_funs::{exit_guest_context, get_max_loop_cycle_count, JitAsmCommonFuns};
use crate::jit::jit_memory::JitEntry;
use crate::logging::debug_println;
use crate::{get_jit_asm_ptr, CURRENT_RUNNING_CPU, DEBUG_LOG, IS_DEBUG};
use std::cmp::min;
use std::intrinsics::likely;
use std::mem;

pub extern "C" fn run_scheduler<const CPU: CpuType, const ARM7_HLE: bool>(asm: *mut JitAsm<CPU>) {
    let asm = unsafe { asm.as_mut_unchecked() };
    let cycles = if ARM7_HLE {
        (asm.runtime_data.accumulated_cycles + 1) >> 1
    } else {
        let arm9_cycles = (asm.runtime_data.accumulated_cycles + 1) >> 1;
        let jit_asm_arm7 = unsafe { get_jit_asm_ptr::<{ ARM7 }>().as_mut_unchecked() };
        let arm7_cycles = if likely(!get_cpu_regs!(jit_asm_arm7.emu, ARM7).is_halted() && !jit_asm_arm7.runtime_data.is_idle_loop()) {
            unsafe { CURRENT_RUNNING_CPU = ARM7 };
            jit_asm_arm7.execute()
        } else {
            0
        };
        unsafe { CURRENT_RUNNING_CPU = ARM9 };
        min(arm9_cycles.wrapping_sub(1), arm7_cycles.wrapping_sub(1)).wrapping_add(1)
    };
    asm.runtime_data.accumulated_cycles = 0;

    let cm = get_cm_mut!(asm.emu);
    cm.add_cycles(cycles);
    if cm.check_events(asm.emu) && !ARM7_HLE {
        let jit_asm_arm7 = unsafe { get_jit_asm_ptr::<{ ARM7 }>().as_mut_unchecked() };
        jit_asm_arm7.runtime_data.clear_idle_loop();
    }
    get_common_mut!(asm.emu).gpu.gpu_3d_regs.run_cmds(cm.get_cycles(), asm.emu);
}

fn flush_cycles<const CPU: CpuType>(asm: &mut JitAsm<CPU>, total_cycles: u16, current_pc: u32) {
    asm.runtime_data.accumulated_cycles += total_cycles + 2 - asm.runtime_data.pre_cycle_count_sum;
    debug_println!("{CPU:?} flush cycles {} at {current_pc:x}", asm.runtime_data.accumulated_cycles);
}

fn check_scheduler<const CPU: CpuType>(asm: &mut JitAsm<CPU>, current_pc: u32) {
    if asm.runtime_data.accumulated_cycles >= get_max_loop_cycle_count::<CPU>() as u16 {
        match CPU {
            ARM9 => {
                let pc_og = get_regs!(asm.emu, ARM9).pc;
                if asm.emu.settings.arm7_hle() {
                    run_scheduler::<CPU, true>(asm as _);
                } else {
                    run_scheduler::<CPU, false>(asm as _);
                }

                // Handle interrupts
                if get_regs!(asm.emu, ARM9).pc != pc_og {
                    debug_println!("{CPU:?} exit guest flush cycles");
                    if IS_DEBUG {
                        asm.runtime_data.set_branch_out_pc(current_pc);
                    }
                    unsafe { exit_guest_context!(asm) };
                }
            }
            ARM7 => {
                debug_println!("{CPU:?} exit guest flush cycles");
                if IS_DEBUG {
                    asm.runtime_data.set_branch_out_pc(current_pc);
                }
                unsafe { exit_guest_context!(asm) };
            }
        }
    }
}

pub unsafe extern "C" fn call_jit_fun<const CPU: CpuType>(asm: &mut JitAsm<CPU>, target_pc: u32) {
    get_regs_mut!(asm.emu, CPU).set_thumb(target_pc & 1 == 1);

    let jit_entry = get_jit!(asm.emu).get_jit_start_addr(align_guest_pc(target_pc));
    let jit_entry: extern "C" fn() = mem::transmute(jit_entry);
    jit_entry();
}

fn pre_branch<const CPU: CpuType, const HAS_LR_RETURN: bool>(asm: &mut JitAsm<CPU>, total_cycles: u16, lr: u32, current_pc: u32) {
    flush_cycles(asm, total_cycles, current_pc);

    if CPU == ARM9 && asm.runtime_data.stack_depth() >= STACK_DEPTH_LIMIT {
        if IS_DEBUG {
            asm.runtime_data.set_branch_out_pc(current_pc);
        }
        if DEBUG_LOG {
            JitAsmCommonFuns::<CPU>::debug_stack_depth_too_big(asm.runtime_data.stack_depth(), current_pc);
        }
        unsafe { exit_guest_context!(asm) };
    }

    check_scheduler(asm, current_pc);

    asm.runtime_data.pre_cycle_count_sum = 0;
    if HAS_LR_RETURN {
        unsafe { *asm.runtime_data.return_stack.get_unchecked_mut(asm.runtime_data.return_stack_ptr as usize) = lr };
        asm.runtime_data.return_stack_ptr += 1;
        asm.runtime_data.return_stack_ptr &= RETURN_STACK_SIZE as u8 - 1;

        if DEBUG_LOG {
            JitAsmCommonFuns::<CPU>::debug_push_return_stack(current_pc, lr, asm.runtime_data.return_stack_ptr);
        }
    }
}

pub unsafe extern "C" fn branch_reg<const CPU: CpuType, const HAS_LR_RETURN: bool>(total_cycles: u16, target_pc: u32, lr: u32, current_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();

    pre_branch::<CPU, HAS_LR_RETURN>(asm, total_cycles, lr, current_pc);

    if DEBUG_LOG {
        JitAsmCommonFuns::<CPU>::debug_branch_reg(current_pc, target_pc);
    }

    if CPU == ARM9 {
        asm.runtime_data.increment_stack_depth();
    }
    call_jit_fun(asm, target_pc);
    if CPU == ARM9 {
        asm.runtime_data.decrement_stack_depth();
    }
    if HAS_LR_RETURN {
        asm.runtime_data.pre_cycle_count_sum = total_cycles;
    }
}

pub unsafe extern "C" fn branch_imm<const CPU: CpuType, const THUMB: bool, const HAS_LR_RETURN: bool>(total_cycles: u16, target_entry: *const JitEntry, lr: u32, current_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();

    pre_branch::<CPU, HAS_LR_RETURN>(asm, total_cycles, lr, current_pc);

    if CPU == ARM9 {
        asm.runtime_data.increment_stack_depth();
    }
    get_regs_mut!(asm.emu, CPU).set_thumb(THUMB);
    let entry = (*target_entry).0;
    let entry: extern "C" fn() = mem::transmute(entry);
    entry();
    if CPU == ARM9 {
        asm.runtime_data.decrement_stack_depth();
    }
    if HAS_LR_RETURN {
        asm.runtime_data.pre_cycle_count_sum = total_cycles;
    }
}

pub unsafe extern "C" fn branch_lr<const CPU: CpuType>(total_cycles: u16, target_pc: u32, current_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();

    flush_cycles(asm, total_cycles, current_pc);
    check_scheduler(asm, current_pc);

    if IS_DEBUG {
        asm.runtime_data.set_branch_out_pc(current_pc);
    }

    asm.runtime_data.return_stack_ptr = asm.runtime_data.return_stack_ptr.wrapping_sub(1);
    asm.runtime_data.return_stack_ptr &= RETURN_STACK_SIZE as u8 - 1;
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
        exit_guest_context!(asm);
    }
}
