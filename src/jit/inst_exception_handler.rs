use crate::core::emu::get_cpu_regs;
use crate::core::exception_handler::ExceptionVector;
use crate::core::hle::bios;
use crate::core::{exception_handler, CpuType};
use crate::jit::inst_mem_handler::imm_breakout;
use crate::jit::jit_asm::JitAsm;

pub unsafe extern "C" fn exception_handler<const CPU: CpuType, const THUMB: bool>(asm: *mut JitAsm<CPU>, opcode: u32, vector: ExceptionVector, pc: u32) {
    exception_handler::handle::<CPU, THUMB>((*asm).emu, opcode, vector);
    if get_cpu_regs!((*asm).emu, CPU).is_halted() {
        imm_breakout!((*asm), pc, THUMB);
    }
}

pub unsafe extern "C" fn bios_uninterrupt<const CPU: CpuType>(asm: *mut JitAsm<CPU>) {
    bios::uninterrupt::<CPU>((*asm).emu)
}
