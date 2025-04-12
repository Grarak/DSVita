use crate::core::exception_handler::ExceptionVector;
use crate::core::{exception_handler, CpuType};
use crate::get_jit_asm_ptr;
use crate::jit::inst_mem_handler::imm_breakout;

pub unsafe extern "C" fn exception_handler<const CPU: CpuType, const THUMB: bool>(opcode: u8, vector: ExceptionVector, pc: u32, total_cycles: u16) {
    let asm = get_jit_asm_ptr::<CPU>();
    exception_handler::handle::<CPU, THUMB>((*asm).emu, opcode, vector);
    if (*asm).emu.cpu_is_halted(CPU) {
        imm_breakout!(CPU, (*asm), pc | (THUMB as u32), total_cycles);
    }
}
