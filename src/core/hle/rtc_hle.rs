use crate::core::emu::{io_rtc_mut, Emu};
use crate::core::hle::arm7_hle::Arm7Hle;
use crate::core::CpuType::ARM7;

pub(super) struct RtcHle;

impl RtcHle {
    pub(super) fn new() -> Self {
        RtcHle
    }

    pub(super) fn ipc_recv(&mut self, data: u32, emu: &mut Emu) {
        let cmd = (data >> 8) & 0x7F;

        if (2..=15).contains(&cmd) || (26..=34).contains(&cmd) || (cmd >= 42) {
            Arm7Hle::send_ipc_fifo(0x5, 0x8001 | (cmd << 8), 0, emu);
            return;
        }

        let rtc = io_rtc_mut!(emu);

        match cmd {
            0x10 => {
                // read date and time
                rtc.update_date_time();
                for i in 0..7 {
                    emu.mem_write::<{ ARM7 }, _>(0x027FFDE8 + i, rtc.date_time[i as usize]);
                }
                Arm7Hle::send_ipc_fifo(0x5, 0x9000, 0, emu);
            }
            0x11 => {
                // read date
                rtc.update_date_time();
                for i in 0..4 {
                    emu.mem_write::<{ ARM7 }, _>(0x027FFDE8 + i, rtc.date_time[i as usize]);
                }
                Arm7Hle::send_ipc_fifo(0x5, 0x9100, 0, emu);
            }
            0x12 => {
                // read time
                rtc.update_date_time();
                for i in 4..7 {
                    emu.mem_write::<{ ARM7 }, _>(0x027FFDE8 + i, rtc.date_time[i as usize]);
                }
                Arm7Hle::send_ipc_fifo(0x5, 0x9200, 0, emu);
            }
            _ => {}
        }
    }
}
