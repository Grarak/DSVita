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

mod handler {
    use crate::core::emu::{get_cp15, Emu};
    use crate::core::exception_handler::ExceptionVector;
    use crate::core::hle::bios;
    use crate::core::CpuType;

    pub fn handle<const CPU: CpuType, const THUMB: bool>(emu: &mut Emu, opcode: u32, vector: ExceptionVector) {
        if CPU == CpuType::ARM7 || get_cp15!(emu).exception_addr != 0 {
            match vector {
                ExceptionVector::SoftwareInterrupt => bios::swi::<CPU>(((opcode >> if THUMB { 0 } else { 16 }) & 0xFF) as u8, emu),
                ExceptionVector::NormalInterrupt => bios::interrupt::<CPU>(emu),
                _ => todo!(),
            }
        } else {
            todo!()
        }
    }
}

pub use handler::handle;
