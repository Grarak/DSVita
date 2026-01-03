use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::jit_asm::{align_guest_pc, call_jit_entry, JitAsm, MAX_STACK_DEPTH_SIZE};
use crate::jit::jit_asm_common_funs::{exit_guest_context, JitAsmCommonFuns};
use crate::logging::debug_println;
use crate::settings::Arm7Emu;
use crate::{get_jit_asm_ptr, BRANCH_LOG, CURRENT_RUNNING_CPU, IS_DEBUG};
use std::cmp::min;
use std::intrinsics::{likely, unlikely};
use std::mem;

pub extern "C" fn run_scheduler<const ARM7_HLE: bool>(asm: *mut JitAsm, current_pc: u32) {
    let asm = unsafe { asm.as_mut_unchecked() };
    debug_println!("{ARM9:?} run scheduler at {current_pc:x} target pc {:x}", ARM9.thread_regs().pc);

    let cycles = if ARM7_HLE {
        (asm.runtime_data.accumulated_cycles + 1) >> 1
    } else {
        let arm9_cycles = (asm.runtime_data.accumulated_cycles + 1) >> 1;
        let jit_asm_arm7 = unsafe { get_jit_asm_ptr::<{ ARM7 }>().as_mut_unchecked() };
        let arm7_cycles = if !asm.emu.cpu_is_halted(ARM7) && !jit_asm_arm7.runtime_data.is_idle_loop() {
            unsafe { CURRENT_RUNNING_CPU = ARM7 };
            jit_asm_arm7.execute::<{ ARM7 }>()
        } else {
            0
        };
        unsafe { CURRENT_RUNNING_CPU = ARM9 };
        min(arm9_cycles.wrapping_sub(1), arm7_cycles.wrapping_sub(1)).wrapping_add(1)
    };
    asm.runtime_data.accumulated_cycles = 0;

    asm.emu.cm.add_cycles(cycles);
    if asm.emu.cm_check_events() && !ARM7_HLE {
        let jit_asm_arm7 = unsafe { get_jit_asm_ptr::<{ ARM7 }>().as_mut_unchecked() };
        jit_asm_arm7.runtime_data.set_idle_loop(false);
    }
    asm.emu.regs_3d_run_cmds(asm.emu.cm.get_cycles());
    asm.emu.breakout_imm = false;
}

fn run_scheduler_idle_loop<const ARM7_HLE: bool>(asm: &mut JitAsm) {
    if ARM7_HLE {
        asm.emu.cm.jump_to_next_event();
    } else {
        let jit_asm_arm7 = unsafe { get_jit_asm_ptr::<{ ARM7 }>().as_mut_unchecked() };
        if !asm.emu.cpu_is_halted(ARM7) && !jit_asm_arm7.runtime_data.is_idle_loop() {
            unsafe { CURRENT_RUNNING_CPU = ARM7 };
            asm.emu.cm.add_cycles(jit_asm_arm7.execute::<{ ARM7 }>());
        } else {
            asm.emu.cm.jump_to_next_event();
        };
        unsafe { CURRENT_RUNNING_CPU = ARM9 };
    }

    if asm.emu.cm_check_events() && !ARM7_HLE {
        let jit_asm_arm7 = unsafe { get_jit_asm_ptr::<{ ARM7 }>().as_mut_unchecked() };
        jit_asm_arm7.runtime_data.set_idle_loop(false);
    }
    asm.emu.regs_3d_run_cmds(asm.emu.cm.get_cycles());
}

#[inline(always)]
fn check_stack_depth(asm: &mut JitAsm, current_pc: u32) {
    let sp_depth_size = asm.runtime_data.get_sp_depth_size();
    if unlikely(sp_depth_size >= MAX_STACK_DEPTH_SIZE) {
        if IS_DEBUG {
            asm.runtime_data.set_branch_out_pc(current_pc);
        }
        if BRANCH_LOG {
            JitAsmCommonFuns::<{ ARM9 }>::debug_stack_depth_too_big(sp_depth_size, current_pc);
        }
        unsafe { exit_guest_context!(asm) };
    }
}

#[inline(never)]
pub extern "C" fn handle_interrupt(asm: *mut JitAsm, target_pc: u32, current_pc: u32) {
    let asm = unsafe { asm.as_mut_unchecked() };
    check_stack_depth(asm, current_pc);

    let pc = ARM9.thread_regs().pc;

    debug_println!("handle interrupt at {current_pc:x} return to {target_pc:x}");

    asm.runtime_data.pre_cycle_count_sum = 0;
    asm.runtime_data.set_in_interrupt(true);
    asm.runtime_data.push_return_stack(target_pc);
    let thumb = pc & 1 == 1;
    let pc = align_guest_pc(pc);
    asm.emu.thread_set_thumb(ARM9, thumb);
    let jit_entry = asm.emu.jit.get_jit_start_addr(pc);
    unsafe { call_jit_entry(pc | (thumb as u32), jit_entry as _, &mut asm.runtime_data.interrupt_sp) };
    debug_println!("return from interrupt");
    asm.runtime_data.set_in_interrupt(false);
}

fn flush_cycles<const CPU: CpuType>(asm: &mut JitAsm, total_cycles: u16, current_pc: u32) {
    asm.runtime_data.accumulated_cycles += total_cycles + 2 - asm.runtime_data.pre_cycle_count_sum;
    debug_println!("{CPU:?} flush cycles {} at {current_pc:x}", asm.runtime_data.accumulated_cycles);
}

#[inline(always)]
pub fn check_scheduler<const CPU: CpuType, const ARM7_HLE: bool>(asm: &mut JitAsm, current_pc: u32) {
    if unlikely(asm.runtime_data.accumulated_cycles >= CPU.max_loop_cycle_count() as u16) {
        match CPU {
            ARM9 => {
                let pc_og = ARM9.thread_regs().pc;
                run_scheduler::<ARM7_HLE>(asm, current_pc);

                if unlikely(ARM9.thread_regs().pc != pc_og) {
                    handle_interrupt(asm, pc_og, current_pc);
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

#[inline(always)]
pub unsafe fn call_jit_fun<const CPU: CpuType>(asm: &mut JitAsm, target_pc: u32) {
    let thumb = target_pc & 1 == 1;
    let target_pc = align_guest_pc(target_pc);
    asm.emu.thread_set_thumb(CPU, thumb);

    let jit_entry = asm.emu.jit.get_jit_start_addr(target_pc);
    let jit_entry: extern "C" fn(u32) = mem::transmute(jit_entry);
    jit_entry(target_pc | (thumb as u32));
}

#[inline(always)]
pub extern "C" fn pre_branch<const CPU: CpuType, const HAS_LR_RETURN: bool, const ARM7_HLE: bool>(asm: &mut JitAsm, total_cycles: u16, lr: u32, current_pc: u32) {
    flush_cycles::<CPU>(asm, total_cycles, current_pc);

    if CPU == ARM9 && HAS_LR_RETURN {
        check_stack_depth(unsafe { mem::transmute(asm as *mut JitAsm) }, current_pc);
    }

    check_scheduler::<CPU, ARM7_HLE>(asm, current_pc);

    asm.runtime_data.pre_cycle_count_sum = 0;
    if HAS_LR_RETURN {
        asm.runtime_data.push_return_stack(lr);
        if BRANCH_LOG {
            JitAsmCommonFuns::<CPU>::debug_push_return_stack(current_pc, lr, asm.runtime_data.get_return_stack_ptr());
        }
    }
}

pub extern "C" fn handle_idle_loop<const ARM7_HLE: bool>(asm: *mut JitAsm, target_pre_cycle_count_sum: u16, current_pc: u32) {
    let asm = unsafe { asm.as_mut_unchecked() };

    let pc_og = ARM9.thread_regs().pc;
    run_scheduler_idle_loop::<ARM7_HLE>(asm);
    if unlikely(ARM9.thread_regs().pc != pc_og) {
        handle_interrupt(asm, pc_og, current_pc);
    }

    asm.runtime_data.accumulated_cycles = 0;
    asm.runtime_data.pre_cycle_count_sum = target_pre_cycle_count_sum;
}

pub unsafe extern "C" fn branch_reg<const CPU: CpuType, const HAS_LR_RETURN: bool, const ARM7_HLE: bool>(total_cycles: u16, target_pc: u32, lr: u32, current_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();

    pre_branch::<CPU, HAS_LR_RETURN, ARM7_HLE>(asm, total_cycles, lr, current_pc);

    if BRANCH_LOG {
        JitAsmCommonFuns::<CPU>::debug_branch_reg(current_pc, target_pc);
    }

    call_jit_fun::<CPU>(asm, target_pc);
    if HAS_LR_RETURN {
        asm.runtime_data.pre_cycle_count_sum = total_cycles;
    }
}

pub unsafe extern "C" fn branch_lr<const CPU: CpuType, const ARM7_HLE: bool>(total_cycles: u16, target_pc: u32, current_pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();

    flush_cycles::<CPU>(asm, total_cycles, current_pc);
    check_scheduler::<CPU, ARM7_HLE>(asm, current_pc);

    if IS_DEBUG {
        asm.runtime_data.set_branch_out_pc(current_pc);
    }

    let desired_lr = asm.runtime_data.pop_return_stack();
    if likely(desired_lr == target_pc) {
        asm.emu.thread_set_thumb(CPU, target_pc & 1 == 1);
        if BRANCH_LOG {
            JitAsmCommonFuns::<CPU>::debug_branch_lr(current_pc, target_pc);
        }
    } else {
        if BRANCH_LOG {
            JitAsmCommonFuns::<CPU>::debug_branch_lr_failed(current_pc, target_pc, desired_lr);
        }
        if CPU == ARM9 && unlikely(asm.runtime_data.is_in_interrupt()) {
            let sp_depth_size = asm.runtime_data.get_sp_depth_size();
            if likely(sp_depth_size < MAX_STACK_DEPTH_SIZE) {
                asm.runtime_data.pre_cycle_count_sum = 0;
                asm.runtime_data.push_return_stack(desired_lr);
                unsafe { call_jit_fun::<CPU>(asm, target_pc) };
            } else if BRANCH_LOG {
                JitAsmCommonFuns::<CPU>::debug_stack_depth_too_big(sp_depth_size, current_pc);
            }
        }
        exit_guest_context!(asm);
    }
}

pub unsafe extern "C" fn branch_any_reg<const ARM7_HLE: bool>(total_cycles: u16, current_pc: u32) {
    let asm = get_jit_asm_ptr::<{ ARM9 }>().as_mut_unchecked();

    flush_cycles::<{ ARM9 }>(asm, total_cycles, current_pc);
    check_stack_depth(asm, current_pc);
    check_scheduler::<{ ARM9 }, ARM7_HLE>(asm, current_pc);

    asm.runtime_data.pre_cycle_count_sum = 0;
    call_jit_fun::<{ ARM9 }>(asm, ARM9.thread_regs().pc);
}

pub unsafe fn breakout_imm<const CPU: CpuType>(asm: &mut JitAsm, total_cycles: u16, current_pc: u32) {
    asm.runtime_data.accumulated_cycles += total_cycles - asm.runtime_data.pre_cycle_count_sum;
    let is_thumb = current_pc & 1 == 1;
    let pc = current_pc & !1;
    if IS_DEBUG {
        asm.runtime_data.set_branch_out_pc(pc);
    }
    let next_pc_offset = (1 << (!is_thumb as u8)) + 2;
    CPU.thread_regs().pc = pc + next_pc_offset;
    asm.emu.breakout_imm = false;
    asm.runtime_data.pre_cycle_count_sum = 0;

    match CPU {
        ARM9 => {
            let arm7_hle = asm.emu.settings.arm7_emu() == Arm7Emu::Hle;
            if arm7_hle {
                run_scheduler::<true>(asm, current_pc);
            } else {
                run_scheduler::<false>(asm, current_pc);
            }
            while asm.emu.cpu_halted_by_gxfifo() {
                if arm7_hle {
                    run_scheduler_idle_loop::<true>(asm);
                } else {
                    run_scheduler_idle_loop::<false>(asm);
                }
            }
            asm.emu.breakout_imm = false;
            if ARM9.thread_regs().pc != pc + next_pc_offset || asm.emu.cpu_is_halted(ARM9) {
                exit_guest_context!(asm);
            }
        }
        ARM7 => {
            exit_guest_context!(asm);
        }
    }
}
