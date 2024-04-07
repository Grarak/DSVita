use crate::emu::cp15::Cp15;
use crate::emu::emu::Emu;

pub unsafe extern "C" fn cp15_write(context: *mut Cp15, reg: u32, value: u32, emu: *const Emu) {
    (*context).write(reg, value, &*emu)
}

pub unsafe extern "C" fn cp15_read(context: *const Cp15, reg: u32, value: *mut u32) {
    (*context).read(reg, value.as_mut().unwrap())
}
