use crate::hle::cpu_regs::CpuRegs;
use crate::hle::CpuType;

pub unsafe extern "C" fn cpu_regs_halt<const CPU: CpuType>(cpu_regs: *mut CpuRegs, bit: u8) {
    (*cpu_regs).halt(bit)
}
