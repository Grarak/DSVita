use crate::hle::exception_handler::{
    exception_handler_arm7_thumb, exception_handler_arm9_thumb, ExceptionVector,
};
use crate::hle::CpuType;
use crate::jit::jit_asm::JitAsm;
use std::ptr;

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit_swi_thumb(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        let bios_context_addr = ptr::addr_of_mut!(self.bios_context) as u32;
        match CPU {
            CpuType::ARM9 => {
                self.emit_call_host_func(
                    |_| {},
                    &[
                        Some(self.cp15_context.as_ptr() as _),
                        Some(bios_context_addr),
                        Some(inst_info.opcode),
                        Some(ExceptionVector::SoftwareInterrupt as u32),
                    ],
                    exception_handler_arm9_thumb as _,
                );
            }
            CpuType::ARM7 => {
                self.emit_call_host_func(
                    |_| {},
                    &[
                        Some(bios_context_addr),
                        Some(inst_info.opcode),
                        Some(ExceptionVector::SoftwareInterrupt as u32),
                    ],
                    exception_handler_arm7_thumb as _,
                );
            }
        }
    }
}
