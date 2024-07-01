use crate::core::cycle_manager::CycleManager;
use crate::core::thread_regs::ThreadRegs;

pub unsafe extern "C" fn register_set_cpsr_checked(regs: *mut ThreadRegs, cm: *mut CycleManager, value: u32, flags: u8) {
    (*regs).set_cpsr_with_flags(value, flags, cm.as_mut().unwrap())
}

pub unsafe extern "C" fn register_set_spsr_checked(regs: *mut ThreadRegs, _: *const CycleManager, value: u32, flags: u8) {
    (*regs).set_spsr_with_flags(value, flags)
}

pub unsafe extern "C" fn register_restore_spsr(regs: *mut ThreadRegs, cm: *mut CycleManager) {
    (*regs).restore_spsr(cm.as_mut().unwrap());
}
