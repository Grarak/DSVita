use crate::core::CpuType;
use crate::get_jit_asm_ptr;

pub unsafe extern "C" fn cpu_regs_halt<const CPU: CpuType>() {
    let asm = get_jit_asm_ptr::<CPU>();
    (*asm).emu.cpu_halt(CPU, 0);
}
