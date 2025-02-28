use crate::core::emu::{get_cm_mut, get_common_mut, get_cpu_regs, get_jit, get_regs, get_regs_mut};
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::jit_asm::{align_guest_pc, JitAsm, MAX_STACK_DEPTH_SIZE};
use crate::jit::jit_asm_common_funs::{exit_guest_context, get_max_loop_cycle_count, JitAsmCommonFuns};
use crate::jit::jit_memory::JitEntry;
use crate::logging::debug_println;
use crate::settings::Arm7Emu;
use crate::{get_jit_asm_ptr, CURRENT_RUNNING_CPU, DEBUG_LOG, IS_DEBUG};
use std::arch::naked_asm;
use std::cmp::min;
use std::intrinsics::{likely, unlikely};
use std::mem;

pub extern "C" fn run_scheduler<const ARM7_HLE: bool>(asm: *mut JitAsm<{ ARM9 }>, current_pc: u32) {
    let asm = unsafe { asm.as_mut_unchecked() };
    debug_println!("{ARM9:?} run scheduler at {current_pc:x} target pc {:x}", get_regs!(asm.emu, ARM9).pc);

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
        jit_asm_arm7.runtime_data.set_idle_loop(false);
    }
    get_common_mut!(asm.emu).gpu.gpu_3d_regs.run_cmds(cm.get_cycles(), asm.emu);
}

fn run_scheduler_idle_loop<const ARM7_HLE: bool>(asm: &mut JitAsm<{ ARM9 }>) {
    let cm = get_cm_mut!(asm.emu);

    if ARM7_HLE {
        cm.jump_to_next_event();
    } else {
        let jit_asm_arm7 = unsafe { get_jit_asm_ptr::<{ ARM7 }>().as_mut_unchecked() };
        if likely(!get_cpu_regs!(jit_asm_arm7.emu, ARM7).is_halted() && !jit_asm_arm7.runtime_data.is_idle_loop()) {
            unsafe { CURRENT_RUNNING_CPU = ARM7 };
            cm.add_cycles(jit_asm_arm7.execute());
        } else {
            cm.jump_to_next_event();
        };
        unsafe { CURRENT_RUNNING_CPU = ARM9 };
    }

    if cm.check_events(asm.emu) && !ARM7_HLE {
        let jit_asm_arm7 = unsafe { get_jit_asm_ptr::<{ ARM7 }>().as_mut_unchecked() };
        jit_asm_arm7.runtime_data.set_idle_loop(false);
    }
    get_common_mut!(asm.emu).gpu.gpu_3d_regs.run_cmds(cm.get_cycles(), asm.emu);
}

#[naked]
unsafe extern "C" fn call_interrupt(_: *const fn(), _: *mut usize) {
    #[rustfmt::skip]
    naked_asm!(
        "push {{r4-r12,lr}}",
        "str sp, [r1]",
        "blx r0",
        "pop {{r4-r12,pc}}",
    );
}

#[inline(always)]
fn check_stack_depth(asm: &mut JitAsm<{ ARM9 }>, current_pc: u32) {
    let sp_depth_size = asm.runtime_data.get_sp_depth_size();
    if unlikely(sp_depth_size >= MAX_STACK_DEPTH_SIZE) {
        if IS_DEBUG {
            asm.runtime_data.set_branch_out_pc(current_pc);
        }
        if DEBUG_LOG {
            JitAsmCommonFuns::<{ ARM9 }>::debug_stack_depth_too_big(sp_depth_size, current_pc);
        }
        unsafe { exit_guest_context!(asm) };
    }
}

pub extern "C" fn handle_interrupt(asm: *mut JitAsm<{ ARM9 }>, target_pc: u32, current_pc: u32) {
    let asm = unsafe { asm.as_mut_unchecked() };
    check_stack_depth(asm, current_pc);

    let regs = get_regs!(asm.emu, ARM9);

    debug_println!("handle interrupt at {current_pc:x} return to {target_pc:x}");

    asm.runtime_data.pre_cycle_count_sum = 0;
    asm.runtime_data.set_in_interrupt(true);
    asm.runtime_data.push_return_stack(target_pc);
    get_regs_mut!(asm.emu, ARM9).set_thumb(regs.pc & 1 == 1);
    let jit_entry = get_jit!(asm.emu).get_jit_start_addr(align_guest_pc(regs.pc));
    unsafe { call_interrupt(jit_entry as _, &mut asm.runtime_data.interrupt_sp) };
    debug_println!("return from interrupt");
    asm.runtime_data.set_in_interrupt(false);
}

fn flush_cycles<const CPU: CpuType>(asm: &mut JitAsm<CPU>, total_cycles: u16, current_pc: u32) {
    asm.runtime_data.accumulated_cycles += total_cycles + 2 - asm.runtime_data.pre_cycle_count_sum;
    debug_println!("{CPU:?} flush cycles {} at {current_pc:x}", asm.runtime_data.accumulated_cycles);
}

#[inline(always)]
fn check_scheduler<const CPU: CpuType>(asm: &mut JitAsm<CPU>, current_pc: u32) {
    if unlikely(asm.runtime_data.accumulated_cycles >= get_max_loop_cycle_count::<CPU>() as u16) {
        match CPU {
            ARM9 => {
                let pc_og = get_regs!(asm.emu, ARM9).pc;
                if asm.emu.settings.arm7_hle() == Arm7Emu::Hle {
                    run_scheduler::<true>(unsafe { mem::transmute(asm as *mut JitAsm<CPU>) }, current_pc);
                } else {
                    run_scheduler::<false>(unsafe { mem::transmute(asm as *mut JitAsm<CPU>) }, current_pc);
                }

                if unlikely(get_regs!(asm.emu, ARM9).pc != pc_og) {
                    handle_interrupt(unsafe { mem::transmute(asm as *mut JitAsm<CPU>) }, pc_og, current_pc);
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

#[inline(always)]
pub extern "C" fn pre_branch<const CPU: CpuType, const HAS_LR_RETURN: bool>(asm: &mut JitAsm<CPU>, total_cycles: u16, lr: u32, current_pc: u32) {
    flush_cycles(asm, total_cycles, current_pc);

    if CPU == ARM9 && HAS_LR_RETURN {
        check_stack_depth(unsafe { mem::transmute(asm as *mut JitAsm<CPU>) }, current_pc);
    }

    check_scheduler(asm, current_pc);

    asm.runtime_data.pre_cycle_count_sum = 0;
    if HAS_LR_RETURN {
        asm.runtime_data.push_return_stack(lr);
        if DEBUG_LOG {
            JitAsmCommonFuns::<CPU>::debug_push_return_stack(current_pc, lr, asm.runtime_data.get_return_stack_ptr());
        }
    }
}

pub extern "C" fn handle_idle_loop<const ARM7_HLE: bool>(asm: *mut JitAsm<{ ARM9 }>, target_pre_cycle_count_sum: u16, current_pc: u32) {
    let asm = unsafe { asm.as_mut_unchecked() };

    let pc_og = get_regs!(asm.emu, ARM9).pc;
    run_scheduler_idle_loop::<ARM7_HLE>(asm);
    if unlikely(get_regs!(asm.emu, ARM9).pc != pc_og) {
        handle_interrupt(asm, pc_og, current_pc);
    }

    asm.runtime_data.accumulated_cycles = 0;
    asm.runtime_data.pre_cycle_count_sum = target_pre_cycle_count_sum;
}

pub unsafe extern "C" fn branch_reg<const CPU: CpuType, const HAS_LR_RETURN: bool>(total_cycles: u16, target_pc: u32, lr: u32, current_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();

    pre_branch::<CPU, HAS_LR_RETURN>(asm, total_cycles, lr, current_pc);

    if DEBUG_LOG {
        JitAsmCommonFuns::<CPU>::debug_branch_reg(current_pc, target_pc);
    }

    call_jit_fun(asm, target_pc);
    if HAS_LR_RETURN {
        asm.runtime_data.pre_cycle_count_sum = total_cycles;
    }
}

pub unsafe extern "C" fn branch_imm<const CPU: CpuType, const THUMB: bool>(total_cycles: u16, target_entry: *const JitEntry, lr: u32, current_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    pre_branch::<CPU, true>(asm, total_cycles, lr, current_pc);

    if DEBUG_LOG {
        JitAsmCommonFuns::<CPU>::debug_branch_imm(current_pc, get_regs!(asm.emu, CPU).pc);
    }

    get_regs_mut!(asm.emu, CPU).set_thumb(THUMB);
    let entry = (*target_entry).0;
    let entry: extern "C" fn() = mem::transmute(entry);
    entry();

    asm.runtime_data.pre_cycle_count_sum = total_cycles;
}

pub unsafe extern "C" fn branch_lr<const CPU: CpuType>(total_cycles: u16, target_pc: u32, current_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();

    flush_cycles(asm, total_cycles, current_pc);
    check_scheduler(asm, current_pc);

    if IS_DEBUG {
        asm.runtime_data.set_branch_out_pc(current_pc);
    }

    let desired_lr = asm.runtime_data.pop_return_stack();
    if likely(desired_lr == target_pc) {
        get_regs_mut!(asm.emu, CPU).set_thumb(target_pc & 1 == 1);
        if DEBUG_LOG {
            JitAsmCommonFuns::<CPU>::debug_branch_lr(current_pc, target_pc);
        }
    } else {
        if DEBUG_LOG {
            JitAsmCommonFuns::<CPU>::debug_branch_lr_failed(current_pc, target_pc, desired_lr);
        }
        if CPU == ARM9 && unlikely(asm.runtime_data.is_in_interrupt()) {
            let sp_depth_size = asm.runtime_data.get_sp_depth_size();
            if likely(sp_depth_size < MAX_STACK_DEPTH_SIZE) {
                asm.runtime_data.pre_cycle_count_sum = 0;
                asm.runtime_data.push_return_stack(desired_lr);
                unsafe { call_jit_fun(asm, target_pc) };
            } else if DEBUG_LOG {
                JitAsmCommonFuns::<CPU>::debug_stack_depth_too_big(sp_depth_size, current_pc);
            }
        }
        exit_guest_context!(asm);
    }
}

pub unsafe extern "C" fn branch_any_reg(total_cycles: u16, current_pc: u32) {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();

    flush_cycles(asm, total_cycles, current_pc);
    check_stack_depth(asm, current_pc);
    check_scheduler(asm, current_pc);

    asm.runtime_data.pre_cycle_count_sum = 0;
    call_jit_fun(asm, get_regs!(asm.emu, ARM9).pc);
    exit_guest_context!(asm);
}
