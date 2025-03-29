use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::{Arm7Hle, IpcFifoTag};
use crate::logging::debug_println;

pub(super) struct MicHle {
    data: [u16; 16],
}

impl MicHle {
    pub(super) fn new() -> Self {
        MicHle { data: [0; 16] }
    }

    pub(super) fn ipc_recv(&mut self, data: u32, emu: &mut Emu) {
        if data & (1 << 25) != 0 {
            self.data.fill(0);
        }

        self.data[((data >> 16) & 0xF) as usize] = data as u16;

        if !(data & (1 << 24)) != 0 {
            return;
        }

        let cmd = (self.data[0] >> 8) - 0x40;
        match cmd {
            0 => Arm7Hle::send_ipc_fifo(IpcFifoTag::Mic, 0x0300C000, false, emu),
            _ => debug_println!("unknown mic request {data:x}"),
        }
    }
}
