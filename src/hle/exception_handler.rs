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

    pub fn arm9<const THUMB: bool>(
        cp15_context: &Cp15Context,
        bios_context: &mut BiosContext<{ CpuType::ARM9 }>,
        opcode: u32,
        vector: ExceptionVector,
    ) {
        if cp15_context.exception_addr != 0 {
            handle_arm9::<THUMB>(bios_context, opcode, vector);
            return;
        }

        todo!()
    }

    pub fn handle_arm9<const THUMB: bool>(
        bios_context: &mut BiosContext<{ CpuType::ARM9 }>,
        opcode: u32,
        vector: ExceptionVector,
    ) {
        match vector {
            ExceptionVector::SoftwareInterrupt => {
                bios_context.swi_arm9(((opcode >> if THUMB { 0 } else { 16 }) & 0xFF) as u8);
            }
            _ => todo!(),
        }
    }

    pub fn handle_arm7<const THUMB: bool>(
        bios_context: &mut BiosContext<{ CpuType::ARM7 }>,
        opcode: u32,
        vector: ExceptionVector,
    ) {
        match vector {
            ExceptionVector::SoftwareInterrupt => {
                bios_context.swi_arm7(((opcode >> if THUMB { 0 } else { 16 }) & 0xFF) as u8);
            }
            _ => todo!(),
        }
    }
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn exception_handler_arm9(
    cp15_context: *const Cp15Context,
    bios_context: *mut BiosContext<{ CpuType::ARM9 }>,
    opcode: u32,
    vector: ExceptionVector,
) {
    exception_handler::arm9::<false>(
        cp15_context.as_ref().unwrap(),
        bios_context.as_mut().unwrap(),
        opcode,
        vector,
    )
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn exception_handler_arm7(
    bios_context: *mut BiosContext<{ CpuType::ARM7 }>,
    opcode: u32,
    vector: ExceptionVector,
) {
    exception_handler::handle_arm7::<false>(bios_context.as_mut().unwrap(), opcode, vector)
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn exception_handler_arm9_thumb(
    cp15_context: *const Cp15Context,
    bios_context: *mut BiosContext<{ CpuType::ARM9 }>,
    opcode: u32,
    vector: ExceptionVector,
) {
    exception_handler::arm9::<true>(
        cp15_context.as_ref().unwrap(),
        bios_context.as_mut().unwrap(),
        opcode,
        vector,
    )
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn exception_handler_arm7_thumb(
    bios_context: *mut BiosContext<{ CpuType::ARM7 }>,
    opcode: u32,
    vector: ExceptionVector,
) {
    exception_handler::handle_arm7::<true>(bios_context.as_mut().unwrap(), opcode, vector)
}
