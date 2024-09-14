use crate::core::emu::get_cpu_regs;
use crate::core::exception_handler::ExceptionVector;
use crate::core::hle::bios;
use crate::core::{exception_handler, CpuType};
use crate::get_jit_asm_ptr;
use crate::jit::inst_mem_handler::imm_breakout;

pub unsafe extern "C" fn exception_handler<const CPU: CpuType, const THUMB: bool>(opcode: u32, vector: ExceptionVector, pc: u32) {
    let asm = get_jit_asm_ptr::<CPU>();
    exception_handler::handle::<CPU, THUMB>((*asm).emu, opcode, vector);
    if get_cpu_regs!((*asm).emu, CPU).is_halted() {
        imm_breakout!((*asm), pc, THUMB);
    }
}

pub unsafe extern "C" fn bios_uninterrupt<const CPU: CpuType>() {
    let asm = get_jit_asm_ptr::<CPU>();
    bios::uninterrupt::<CPU>((*asm).emu)
}
