use crate::core::emu::{get_regs_mut, Emu};
use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType;
use crate::get_jit_asm_ptr;

pub unsafe extern "C" fn register_set_cpsr_checked<const CPU: CpuType>(value: u32, flags: u8) -> u32 {
    let asm = get_jit_asm_ptr::<CPU>();
    let regs = get_regs_mut!((*asm).emu, CPU);
    regs.set_cpsr_with_flags(value, flags, (*asm).emu);
    regs.cpsr
}

pub unsafe extern "C" fn register_set_spsr_checked<const CPU: CpuType>(value: u32, flags: u8) -> u32 {
    let asm = get_jit_asm_ptr::<CPU>();
    let regs = get_regs_mut!((*asm).emu, CPU);
    regs.set_spsr_with_flags(value, flags);
    regs.cpsr
}

pub unsafe extern "C" fn register_restore_spsr(regs: *mut ThreadRegs, emu: *mut Emu) {
    (*regs).restore_spsr(emu.as_mut_unchecked());
}

pub unsafe extern "C" fn restore_thumb_after_restore_spsr(regs: *mut ThreadRegs) {
    (*regs).restore_thumb_mode();
}

pub unsafe extern "C" fn set_pc_arm_mode(regs: *mut ThreadRegs) {
    (*regs).force_pc_arm_mode()
}

pub unsafe extern "C" fn set_pc_thumb_mode(regs: *mut ThreadRegs) {
    (*regs).force_pc_thumb_mode()
}
