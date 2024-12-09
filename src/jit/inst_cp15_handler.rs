use crate::core::emu::{get_cp15, get_cp15_mut};
use crate::core::CpuType::ARM9;
use crate::get_jit_asm_ptr;

pub unsafe extern "C" fn cp15_write(reg: u32, value: u32) {
    let asm = get_jit_asm_ptr::<{ ARM9 }>();
    get_cp15_mut!((*asm).emu).write(reg, value, (*asm).emu);
}

pub unsafe extern "C" fn cp15_read(reg: u32) -> u32 {
    let asm = get_jit_asm_ptr::<{ ARM9 }>();
    get_cp15!((*asm).emu).read(reg)
}
