use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::IpcFifoTag;
use crate::logging::debug_println;

pub(super) struct MicHle {
    data: [u16; 16],
}

impl MicHle {
    pub(super) fn new() -> Self {
        MicHle { data: [0; 16] }
    }
}

impl Emu {
    pub(super) fn mic_hle_ipc_recv(&mut self, data: u32) {
        let mic = &mut self.hle.mic;

        if data & (1 << 25) != 0 {
            mic.data.fill(0);
        }

        mic.data[((data >> 16) & 0xF) as usize] = data as u16;

        if (data & (1 << 24)) == 0 {
            return;
        }

        let cmd = (mic.data[0] >> 8) - 0x40;
        match cmd {
            0 => self.arm7_hle_send_ipc_fifo(IpcFifoTag::Mic, 0x0300C000, false),
            _ => {
                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Mic, 0x0300C000, false); // Just send same reply, seems to work
                debug_println!("unknown mic request {data:x}");
            }
        }
    }
}
