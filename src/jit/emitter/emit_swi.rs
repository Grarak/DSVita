use crate::hle::exception_handler::{
    exception_handler_arm7, exception_handler_arm9, ExceptionVector,
};
use crate::hle::CpuType;
use crate::jit::jit_asm::JitAsm;
use crate::jit::Cond;

impl<const CPU: CpuType> JitAsm<CPU> {
    pub fn emit_swi(&mut self, buf_index: usize, _: u32) {
        let inst_info = &self.jit_buf.instructions[buf_index];

        if inst_info.cond != Cond::AL {
            todo!()
        }

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
                    exception_handler_arm9::<false> as _,
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
                    exception_handler_arm7::<false> as _,
                );
            }
        }
    }
}
