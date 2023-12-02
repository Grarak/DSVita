#[repr(u8)]
pub enum ExceptionVector {
    Reset = 0x0,
    UndefinedInstruction = 0x4,
    SoftwareInterrupt = 0x8,
    PrefetchAbort = 0xC,
    DataAbort = 0x10,
    AddressExceeds26Bit = 0x14,
    NormalInterrupt = 0x18,
    FastInterrupt = 0x1C,
}

mod exception_handler {
    use crate::hle::bios;
    use crate::hle::cp15_context::Cp15Context;
    use crate::hle::exception_handler::ExceptionVector;
    use crate::hle::thread_context::ThreadRegs;

    pub extern "C" fn exception(
        cp15_context: &Cp15Context,
        regs: &mut ThreadRegs,
        opcode: u32,
        vector: ExceptionVector,
    ) {
        if cp15_context.exception_addr != 0 {
            match vector {
                ExceptionVector::SoftwareInterrupt => {
                    bios::swi(((opcode >> 16) & 0xFF) as u8, regs)
                }
                _ => todo!(),
            }
            return;
        }

        todo!()
    }
}

pub use exception_handler::exception;
