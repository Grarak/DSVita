use crate::core::emu::{get_cp15, get_cp15_mut};
use crate::core::CpuType::ARM9;
use crate::jit::jit_asm::JitAsm;

pub unsafe extern "C" fn cp15_write(asm: *mut JitAsm<{ ARM9 }>, reg: u32, value: u32) {
    get_cp15_mut!((*asm).emu, ARM9).write(reg, value, (*asm).emu);
}

pub unsafe extern "C" fn cp15_read(asm: *mut JitAsm<{ ARM9 }>, reg: u32) -> u32 {
    get_cp15!((*asm).emu, ARM9).read(reg)
}
