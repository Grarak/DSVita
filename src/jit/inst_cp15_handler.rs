use crate::hle::cp15::Cp15;
use crate::hle::hle::Hle;

pub unsafe extern "C" fn cp15_write(context: *mut Cp15, reg: u32, value: u32, hle: *const Hle) {
    (*context).write(reg, value, &*hle)
}

pub unsafe extern "C" fn cp15_read(context: *const Cp15, reg: u32, value: *mut u32) {
    (*context).read(reg, value.as_mut().unwrap())
}
