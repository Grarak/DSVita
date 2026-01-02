use crate::core::exception_handler::ExceptionVector;
use crate::core::{exception_handler, CpuType};
use crate::get_jit_asm_ptr;
use crate::jit::inst_branch_handler::breakout_imm;

pub unsafe extern "C" fn software_interrupt_handler<const CPU: CpuType>(opcode: u8, pc: u32, total_cycles: u16) {
    let asm = get_jit_asm_ptr::<CPU>().as_mut_unchecked();
    exception_handler::handle::<CPU>(asm.emu, opcode, ExceptionVector::SoftwareInterrupt);
    if asm.emu.cpu_is_halted(CPU) {
        breakout_imm::<CPU>(asm, total_cycles, pc);
    }
}
