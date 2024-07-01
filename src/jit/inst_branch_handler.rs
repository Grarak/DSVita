use crate::core::emu::{get_regs, get_regs_mut};
use crate::core::CpuType;
use crate::jit::jit_asm::JitAsm;
use crate::DEBUG_LOG_BRANCH_OUT;
use std::arch::asm;
use std::hint::unreachable_unchecked;

pub unsafe extern "C" fn inst_branch_label<const CPU: CpuType, const THUMB: bool>(asm: *mut JitAsm<CPU>, cycles: u32, new_pc: u32, pc: u32) {
    let total_cycles = (cycles & 0xFFFF) as u16;

    let asm = asm.as_mut().unwrap_unchecked();
    let executed_cycles = total_cycles - asm.runtime_data.pre_cycle_count_sum + 2;
    let new_accumulated_cycles = executed_cycles + asm.runtime_data.accumulated_cycles;
    let cycle_correction = get_regs!(asm.emu, CPU).cycle_correction;
    if (new_accumulated_cycles as i16 + cycle_correction) as u16 >= asm.runtime_data.next_event_in_cycles {
        if DEBUG_LOG_BRANCH_OUT {
            asm.runtime_data.branch_out_pc = pc;
        }
        asm.runtime_data.branch_out_total_cycles = total_cycles;
        get_regs_mut!(asm.emu, CPU).pc = new_pc;
        let breakout_addr = if THUMB { asm.breakout_skip_save_regs_thumb_addr } else { asm.breakout_skip_save_regs_addr };
        asm!("bx {}", in(reg) breakout_addr);
        unreachable_unchecked();
    } else {
        let new_pre_cycle_sum = (cycles >> 16) as u16;
        asm.runtime_data.accumulated_cycles = new_accumulated_cycles;
        asm.runtime_data.pre_cycle_count_sum = new_pre_cycle_sum;
    }
}
