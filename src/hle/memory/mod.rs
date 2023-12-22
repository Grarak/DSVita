pub mod dma;
mod io_ports;
pub mod mem_handler;
pub mod memory;
pub mod regions;

pub trait Convert: Copy + Into<u32> {
    fn from(value: u32) -> Self;
}

impl Convert for u8 {
    fn from(value: u32) -> Self {
        value as u8
    }
}

impl Convert for u16 {
    fn from(value: u32) -> Self {
        value as u16
    }
}

impl Convert for u32 {
    fn from(value: u32) -> Self {
        value
    }
}
