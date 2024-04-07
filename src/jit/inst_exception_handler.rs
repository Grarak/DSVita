use crate::emu::exception_handler::ExceptionVector;
use crate::emu::emu::Emu;
use crate::emu::{bios, exception_handler, CpuType};

pub unsafe extern "C" fn exception_handler<const CPU: CpuType, const THUMB: bool>(
    emu: *mut Emu,
    opcode: u32,
    vector: ExceptionVector,
) {
    exception_handler::handle::<CPU, THUMB>(emu.as_mut().unwrap(), opcode, vector)
}

pub unsafe extern "C" fn bios_uninterrupt<const CPU: CpuType>(emu: *mut Emu) {
    bios::uninterrupt::<CPU>(emu.as_mut().unwrap())
}
