use crate::core::CpuType;
use crate::get_jit_asm_ptr;

const MAX_LOOP_CYCLE_COUNT: u16 = 200;

pub unsafe extern "C" fn branch_label_flush_cycles<const CPU: CpuType>(current_total_cycles: u16, target_pre_cycle_count_sum: u16) -> bool {
    let asm = get_jit_asm_ptr::<CPU>();
    let runtime_data = &mut (*asm).runtime_data;
    // +2 for branching
    let total_cycles = runtime_data.accumulated_cycles + current_total_cycles - runtime_data.pre_cycle_count_sum + 2;
    if total_cycles >= MAX_LOOP_CYCLE_COUNT {
        true
    } else {
        runtime_data.accumulated_cycles = total_cycles;
        runtime_data.pre_cycle_count_sum = target_pre_cycle_count_sum;
        false
    }
}
