mod bios;
mod bios_lookup_table;
pub mod cp15_context;
pub mod exception_handler;
mod gpu;
pub mod memory;
pub mod registers;
pub mod thread_context;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CpuType {
    ARM7,
    ARM9,
}

impl Default for CpuType {
    fn default() -> Self {
        CpuType::ARM9
    }
}
