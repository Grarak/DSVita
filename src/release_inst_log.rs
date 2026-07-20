use crate::core::emu::Emu;
use crate::core::CpuType;

#[inline]
pub fn is_logging() -> bool {
    false
}

#[inline]
pub fn log_text(_: &str) {}

#[inline]
pub fn log(_: &Emu, _: CpuType, _: u32, _: u32) {}
