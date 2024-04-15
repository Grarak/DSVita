use crate::emu::emu::Emu;
use crate::emu::hle::arm7_hle::Arm7Hle;

pub(super) struct RtcHle;

impl RtcHle {
    pub(super) fn new() -> Self {
        RtcHle {}
    }

    fn read(reg: u8, addr: u32, len: u32, emu: &mut Emu) {
        // TODO
    }

    pub(super) fn ipc_recv(&mut self, data: u32, emu: &mut Emu) {
        let cmd = (data >> 8) & 0x7F;

        if (2..=15).contains(&cmd) || (26..=34).contains(&cmd) || (cmd >= 42) {
            Arm7Hle::send_ipc_fifo(0x5, 0x8001 | (cmd << 8), 0, emu);
            return;
        }

        match cmd {
            0x10 => {
                // read date and time
                Self::read(0x20, 0x027FFDE8, 7, emu);
                Arm7Hle::send_ipc_fifo(0x5, 0x9000, 0, emu);
            }
            0x11 => {
                // read date
                Self::read(0x20, 0x027FFDE8, 4, emu);
                Arm7Hle::send_ipc_fifo(0x5, 0x9100, 0, emu);
            }
            0x12 => {
                // read time
                Self::read(0x60, 0x027FFDE8 + 4, 3, emu);
                Arm7Hle::send_ipc_fifo(0x5, 0x9200, 0, emu);
            }
            _ => {}
        }
    }
}
