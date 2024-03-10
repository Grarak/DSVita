use crate::hle::exception_handler::ExceptionVector;
use crate::hle::hle::Hle;
use crate::hle::{bios, exception_handler, CpuType};

pub unsafe extern "C" fn exception_handler<const CPU: CpuType, const THUMB: bool>(
    hle: *mut Hle,
    opcode: u32,
    vector: ExceptionVector,
) {
    exception_handler::handle::<CPU, THUMB>(hle.as_mut().unwrap(), opcode, vector)
}

pub unsafe extern "C" fn bios_uninterrupt<const CPU: CpuType>(hle: *mut Hle) {
    bios::uninterrupt::<CPU>(hle.as_mut().unwrap())
}
