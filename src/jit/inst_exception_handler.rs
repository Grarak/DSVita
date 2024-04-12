use crate::emu::emu::{get_cpu_regs, Emu};
use crate::emu::exception_handler::ExceptionVector;
use crate::emu::{bios, exception_handler, CpuType};
use crate::jit::inst_mem_handler::imm_breakout;
use crate::jit::jit_asm::JitAsm;
use std::hint::unreachable_unchecked;

pub unsafe extern "C" fn exception_handler<const CPU: CpuType, const THUMB: bool>(
    asm: *mut JitAsm<CPU>,
    opcode: u32,
    vector: ExceptionVector,
    pc: u32,
) {
    exception_handler::handle::<CPU, THUMB>((*asm).emu, opcode, vector);
    if get_cpu_regs!((*asm).emu, CPU).is_halted() {
        let asm = asm.as_mut().unwrap_unchecked();
        imm_breakout!(asm, pc, THUMB);
    }
}

pub unsafe extern "C" fn bios_uninterrupt<const CPU: CpuType>(emu: *mut Emu) {
    bios::uninterrupt::<CPU>(emu.as_mut().unwrap())
}
