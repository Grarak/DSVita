use crate::emu::emu::Emu;
use crate::emu::hle::arm7_hle::Arm7Hle;

pub(super) struct PowerManagerHle {
    data: [u16; 16],
}

impl PowerManagerHle {
    pub(super) fn new() -> Self {
        PowerManagerHle { data: [0; 16] }
    }

    pub(super) fn ipc_recv(&mut self, data: u32, emu: &mut Emu) {
        if data & (1 << 25) != 0 {
            self.data.fill(0);
        }

        self.data[((data >> 16) & 0xF) as usize] = data as u16;

        if data & (1 << 24) == 0 {
            return;
        }

        let cmd = (self.data[0] >> 8) - 0x60;
        match cmd {
            3 => {
                Arm7Hle::send_ipc_fifo(0x8, 0x0300E300, 0, emu);
            }
            4 => {
                Arm7Hle::send_ipc_fifo(0x8, 0x03008000 | (((self.data[1] as u32 + 0x70) & 0xFF) << 8), 0, emu);
            }
            5 => {
                Arm7Hle::send_ipc_fifo(0x8, 0x03008000 | (((self.data[1] as u32 + 0x70) & 0xFF) << 8), 0, emu);
            }
            6 => {
                Arm7Hle::send_ipc_fifo(0x8, 0x03008000 | (((self.data[1] as u32 + 0x70) & 0xFF) << 8), 0, emu);
            }
            _ => {}
        }
    }
}
