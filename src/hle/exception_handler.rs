use crate::hle::bios;
use crate::hle::cp15_context::Cp15Context;
use crate::hle::thread_regs::ThreadRegs;

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

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn exception_handler(
    cp15_context: *const Cp15Context,
    regs: *mut ThreadRegs,
    opcode: u32,
    vector: ExceptionVector,
) {
    if (*cp15_context).exception_addr != 0 {
        match vector {
            ExceptionVector::SoftwareInterrupt => {
                bios::swi(((opcode >> 16) & 0xFF) as u8, regs.as_mut().unwrap())
            }
            _ => todo!(),
        }
        return;
    }

    todo!()
}
