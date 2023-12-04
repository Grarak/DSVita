mod bios;
mod bios_lookup_table;
pub mod cp15_context;
pub mod exception_handler;
pub mod indirect_memory;
pub mod thread_context;

#[derive(Debug)]
pub enum CpuType {
    ARM7,
    ARM9,
}
