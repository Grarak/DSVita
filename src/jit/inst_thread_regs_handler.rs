use crate::core::emu::Emu;
use crate::core::thread_regs::ThreadRegs;

pub unsafe extern "C" fn register_set_cpsr_checked(value: u32, flags: u8, regs: *mut ThreadRegs, emu: *mut Emu) -> u32 {
    (*regs).set_cpsr_with_flags(value, flags, emu.as_mut_unchecked());
    (*regs).cpsr
}

pub unsafe extern "C" fn register_set_spsr_checked(value: u32, flags: u8, regs: *mut ThreadRegs) -> u32 {
    (*regs).set_spsr_with_flags(value, flags);
    (*regs).cpsr
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
