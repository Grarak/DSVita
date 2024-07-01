use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::Arm7Hle;
use crate::logging::debug_println;

pub(super) struct CartHle {
    cmd: u32,
    data_pos: u32,
    buffer: u32,
}

impl CartHle {
    pub(super) fn new() -> Self {
        CartHle { cmd: 0, data_pos: 0, buffer: 0 }
    }

    pub(super) fn ipc_recv(&mut self, data: u32, emu: &mut Emu) {
        if self.data_pos == 0 {
            self.cmd = data;
        }

        match self.cmd {
            0 => {
                if self.data_pos == 1 {
                    self.buffer = data;
                    Arm7Hle::send_ipc_fifo(0xB, 0x1, 1, emu);
                    self.data_pos = 0;
                    return;
                }
            }
            2 => {
                Arm7Hle::send_ipc_fifo(0xB, 0x1, 1, emu);
                self.data_pos = 0;
                return;
            }
            6 => {
                Arm7Hle::send_ipc_fifo(0xB, 0x1, 1, emu);
                self.data_pos = 0;
                return;
            }
            7 => {
                Arm7Hle::send_ipc_fifo(0xB, 0x1, 1, emu);
                self.data_pos = 0;
                return;
            }
            8 => {
                Arm7Hle::send_ipc_fifo(0xB, 0x1, 1, emu);
                self.data_pos = 0;
                return;
            }
            9 => {
                Arm7Hle::send_ipc_fifo(0xB, 0x1, 1, emu);
                self.data_pos = 0;
                return;
            }
            _ => {
                debug_println!("cart save: unknown cmd {:x}", self.cmd);
            }
        }

        self.data_pos += 1;
    }
}
