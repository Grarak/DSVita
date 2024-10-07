use crate::core::emu::{get_regs, get_regs_mut};
use crate::core::CpuType;
use crate::get_jit_asm_ptr;

pub unsafe extern "C" fn register_set_cpsr_checked<const CPU: CpuType>(value: u32, flags: u8) -> u32 {
    let asm = get_jit_asm_ptr::<CPU>();
    get_regs_mut!((*asm).emu, CPU).set_cpsr_with_flags(value, flags, (*asm).emu);
    get_regs!((*asm).emu, CPU).cpsr
}

pub unsafe extern "C" fn register_set_spsr_checked<const CPU: CpuType>(value: u32, flags: u8) -> u32 {
    let asm = get_jit_asm_ptr::<CPU>();
    get_regs_mut!((*asm).emu, CPU).set_spsr_with_flags(value, flags);
    get_regs!((*asm).emu, CPU).cpsr
}

pub unsafe extern "C" fn register_restore_spsr<const CPU: CpuType>() {
    let asm = get_jit_asm_ptr::<CPU>();
    get_regs_mut!((*asm).emu, CPU).restore_spsr((*asm).emu);
}

pub unsafe extern "C" fn restore_thumb_after_restore_spsr<const CPU: CpuType>() {
    let asm = get_jit_asm_ptr::<CPU>();
    get_regs_mut!((*asm).emu, CPU).restore_thumb_mode();
}

pub unsafe extern "C" fn set_pc_arm_mode<const CPU: CpuType>() {
    let asm = get_jit_asm_ptr::<CPU>();
    get_regs_mut!((*asm).emu, CPU).force_pc_arm_mode();
}

pub unsafe extern "C" fn set_pc_thumb_mode<const CPU: CpuType>() {
    let asm = get_jit_asm_ptr::<CPU>();
    get_regs_mut!((*asm).emu, CPU).force_pc_thumb_mode();
}
