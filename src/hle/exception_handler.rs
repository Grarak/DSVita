use crate::hle::bios_context::BiosContext;
use crate::hle::cp15_context::Cp15Context;
use crate::hle::CpuType;

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
    use crate::hle::bios_context::BiosContext;
    use crate::hle::cp15_context::Cp15Context;
    use crate::hle::exception_handler::ExceptionVector;
    use crate::hle::CpuType;

    pub fn handle<const CPU: CpuType, const THUMB: bool>(
        cp15_context: Option<&Cp15Context>,
        bios_context: &mut BiosContext<CPU>,
        opcode: u32,
        vector: ExceptionVector,
    ) {
        if CPU == CpuType::ARM7 || cp15_context.unwrap().exception_addr != 0 {
            match vector {
                ExceptionVector::SoftwareInterrupt => {
                    bios_context.swi(((opcode >> if THUMB { 0 } else { 16 }) & 0xFF) as u8);
                }
                ExceptionVector::NormalInterrupt => {
                    bios_context.interrupt(cp15_context);
                }
                _ => todo!(),
            }
        } else {
            todo!()
        }
    }
}

pub(super) use exception_handler::*;

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn exception_handler_arm9<const THUMB: bool>(
    cp15_context: *const Cp15Context,
    bios_context: *mut BiosContext<{ CpuType::ARM9 }>,
    opcode: u32,
    vector: ExceptionVector,
) {
    handle::<{ CpuType::ARM9 }, THUMB>(
        Some(cp15_context.as_ref().unwrap()),
        bios_context.as_mut().unwrap(),
        opcode,
        vector,
    )
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn exception_handler_arm7<const THUMB: bool>(
    bios_context: *mut BiosContext<{ CpuType::ARM7 }>,
    opcode: u32,
    vector: ExceptionVector,
) {
    handle::<{ CpuType::ARM7 }, THUMB>(None, bios_context.as_mut().unwrap(), opcode, vector)
}
