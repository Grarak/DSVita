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
    use crate::hle::exception_handler::ExceptionVector;
    use crate::hle::hle::{get_cp15, Hle};
    use crate::hle::{bios, CpuType};

    pub fn handle<const CPU: CpuType, const THUMB: bool>(
        hle: &mut Hle,
        opcode: u32,
        vector: ExceptionVector,
    ) {
        if CPU == CpuType::ARM7 || get_cp15!(hle, CPU).exception_addr != 0 {
            match vector {
                ExceptionVector::SoftwareInterrupt => {
                    bios::swi::<CPU>(((opcode >> if THUMB { 0 } else { 16 }) & 0xFF) as u8, hle)
                }
                ExceptionVector::NormalInterrupt => bios::interrupt::<CPU>(hle),
                _ => todo!(),
            }
        } else {
            todo!()
        }
    }
}

pub use handler::handle;
