use crate::core::emu::get_cpu_regs_mut;
use crate::core::CpuType;
use crate::jit::jit_asm::JitAsm;

pub unsafe extern "C" fn cpu_regs_halt<const CPU: CpuType>(asm: *mut JitAsm<CPU>, bit: u8) {
    get_cpu_regs_mut!((*asm).emu, CPU).halt(bit)
}
