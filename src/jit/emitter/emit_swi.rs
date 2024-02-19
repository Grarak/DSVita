use crate::hle::exception_handler::{
    exception_handler_arm7, exception_handler_arm9, ExceptionVector,
};
use crate::hle::CpuType;
use crate::jit::jit_asm::JitAsm;

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit_swi<const THUMB: bool>(&mut self, buf_index: usize, pc: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let swi_code = ((inst_info.opcode >> if THUMB { 0 } else { 16 }) & 0xFF) as u8;
        let is_halt = swi_code == 6;

        if is_halt {
            self.emit_halt::<THUMB>(pc);
        } else {
            let bios_context_addr = self.bios_context.as_ptr() as u32;
            match CPU {
                CpuType::ARM9 => {
                    self.emit_call_host_func(
                        |_| {},
                        |_, _| {},
                        &[
                            Some(self.cp15_context.as_ptr() as _),
                            Some(bios_context_addr),
                            Some(inst_info.opcode),
                            Some(ExceptionVector::SoftwareInterrupt as u32),
                        ],
                        exception_handler_arm9::<THUMB> as _,
                    );
                }
                CpuType::ARM7 => {
                    self.emit_call_host_func(
                        |_| {},
                        |_, _| {},
                        &[
                            Some(bios_context_addr),
                            Some(inst_info.opcode),
                            Some(ExceptionVector::SoftwareInterrupt as u32),
                        ],
                        exception_handler_arm7::<THUMB> as _,
                    );
                }
            }
        }
    }
}
