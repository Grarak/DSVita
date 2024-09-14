use crate::core::emu::get_cpu_regs_mut;
use crate::core::CpuType;
use crate::get_jit_asm_ptr;

pub unsafe extern "C" fn cpu_regs_halt<const CPU: CpuType>() {
    let asm = get_jit_asm_ptr::<CPU>();
    get_cpu_regs_mut!((*asm).emu, CPU).halt(0)
}
