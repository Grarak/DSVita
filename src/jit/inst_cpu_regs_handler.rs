use crate::core::cpu_regs::CpuRegs;
use crate::core::CpuType;

pub unsafe extern "C" fn cpu_regs_halt<const CPU: CpuType>(cpu_regs: *mut CpuRegs, bit: u8) {
    (*cpu_regs).halt(bit)
}
