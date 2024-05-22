use crate::emu::exception_handler::ExceptionVector;
use crate::emu::CpuType;
use crate::jit::inst_exception_handler::exception_handler;
use crate::jit::jit_asm::JitAsm;

impl<'a, const CPU: CpuType> JitAsm<'a, CPU> {
    pub fn emit_swi<const THUMB: bool>(&mut self, buf_index: usize, pc: u32) {
        let jit_asm_addr = self as *mut _ as _;
        let inst_info = &self.jit_buf.instructions[buf_index];

        self.jit_buf.emit_opcodes.extend(self.emit_call_host_func(
            |_, _| {},
            &[Some(jit_asm_addr), Some(inst_info.opcode), Some(ExceptionVector::SoftwareInterrupt as u32), Some(pc)],
            exception_handler::<CPU, THUMB> as _,
        ));
    }
}
