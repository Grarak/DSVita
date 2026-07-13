use crate::core::emu::Emu;
use crate::core::CpuType;
pub use crate::inst_log_format::MemLogKind;
use crate::utils::Convert;

#[inline]
pub fn is_logging() -> bool {
    false
}

#[inline]
pub fn log_text(_: &str) {}

#[inline]
pub fn log(_: &Emu, _: CpuType, _: u32, _: u32) {}

#[inline]
pub fn log_mem(_: CpuType, _: MemLogKind, _: u32, _: u32) {}

#[inline]
pub fn log_mem_slice<T: Convert>(_: CpuType, _: MemLogKind, _: u32, _: &[T]) {}
