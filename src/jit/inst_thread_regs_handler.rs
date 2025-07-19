use crate::core::CpuType;
use crate::get_jit_asm_ptr;

pub unsafe extern "C" fn register_set_cpsr_checked<const CPU: CpuType>(value: u32, flags: u8) -> u32 {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    asm.emu.thread_set_cpsr_with_flags(CPU, value, flags);
    CPU.thread_regs().cpsr
}

pub unsafe extern "C" fn register_set_spsr_checked<const CPU: CpuType>(value: u32, flags: u8) -> u32 {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    asm.emu.thread_set_spsr_with_flags(CPU, value, flags);
    CPU.thread_regs().cpsr
}

pub unsafe extern "C" fn register_restore_spsr<const CPU: CpuType>() {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    asm.emu.thread_restore_spsr(CPU);
}

pub unsafe extern "C" fn restore_thumb_after_restore_spsr<const CPU: CpuType>() {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    asm.emu.thread_restore_thumb_mode(CPU);
}

pub unsafe extern "C" fn set_pc_arm_mode<const CPU: CpuType>() {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    asm.emu.thread_force_pc_arm_mode(CPU)
}

pub unsafe extern "C" fn set_pc_thumb_mode<const CPU: CpuType>() {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    asm.emu.thread_force_pc_thumb_mode(CPU)
}
