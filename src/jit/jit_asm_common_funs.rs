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

pub(crate) use exit_guest_context;

use crate::core::CpuType;
use crate::logging::branch_println;

pub struct JitAsmCommonFuns<const CPU: CpuType>;

impl<const CPU: CpuType> JitAsmCommonFuns<CPU> {
    pub extern "C" fn debug_push_return_stack(current_pc: u32, lr_pc: u32, stack_size: usize) {
        branch_println!("{CPU:?} push {lr_pc:x} to return stack with size {stack_size} at {current_pc:x}")
    }

    pub extern "C" fn debug_stack_depth_too_big(size: usize, current_pc: u32) {
        branch_println!("{CPU:?} stack depth exceeded {size} at {current_pc:x}")
    }

    pub extern "C" fn debug_branch_reg(current_pc: u32, target_pc: u32) {
        branch_println!("{CPU:?} branch reg from {current_pc:x} to {target_pc:x}")
    }

    pub extern "C" fn debug_branch_lr(current_pc: u32, target_pc: u32) {
        branch_println!("{CPU:?} branch lr from {current_pc:x} to {target_pc:x}")
    }

    pub extern "C" fn debug_branch_lr_failed(current_pc: u32, target_pc: u32, desired_pc: u32) {
        branch_println!("{CPU:?} failed to branch lr from {current_pc:x} to {target_pc:x} desired: {desired_pc:x}")
    }

    pub extern "C" fn debug_return_stack_empty(current_pc: u32, target_pc: u32) {
        branch_println!("{CPU:?} empty return stack {current_pc:x} to {target_pc:x}")
    }
}
