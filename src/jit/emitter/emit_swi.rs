use crate::hle::exception_handler::ExceptionVector;
use crate::hle::CpuType;
use crate::jit::inst_exception_handler::exception_handler;
use crate::jit::jit_asm::JitAsm;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_swi<const THUMB: bool>(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let swi_code = ((inst_info.opcode >> if THUMB { 0 } else { 16 }) & 0xFF) as u8;
        let is_halt = swi_code == 6;

        if is_halt {
            self.emit_halt::<THUMB>(pc);
        } else {
            let hle_addr = self.hle as *mut _ as _;
            self.emit_call_host_func(
                |_| {},
                |_, _| {},
                &[
                    Some(hle_addr),
                    Some(inst_info.opcode),
                    Some(ExceptionVector::SoftwareInterrupt as u32),
                ],
                exception_handler::<CPU, THUMB> as _,
            );
        }
    }
}
