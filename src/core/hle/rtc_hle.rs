use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::IpcFifoTag;
use crate::core::CpuType::ARM7;

pub struct RtcHle;

impl RtcHle {
    pub(super) fn new() -> Self {
        RtcHle
    }
}

impl Emu {
    pub fn rtc_hle_ipc_recv(&mut self, data: u32) {
        let cmd = (data >> 8) & 0x7F;

        if (2..=15).contains(&cmd) || (26..=34).contains(&cmd) || (cmd >= 42) {
            self.arm7_hle_send_ipc_fifo(IpcFifoTag::Rtc, 0x8001 | (cmd << 8), false);
            return;
        }

        match cmd {
            0x10 => {
                // read date and time
                self.rtc.update_date_time();
                for i in 0..7 {
                    self.mem_write::<{ ARM7 }, _>(0x027FFDE8 + i, self.rtc.date_time[i as usize]);
                }
                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Rtc, 0x9000, false);
            }
            0x11 => {
                // read date
                self.rtc.update_date_time();
                for i in 0..4 {
                    self.mem_write::<{ ARM7 }, _>(0x027FFDE8 + i, self.rtc.date_time[i as usize]);
                }
                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Rtc, 0x9100, false);
            }
            0x12 => {
                // read time
                self.rtc.update_date_time();
                for i in 4..7 {
                    self.mem_write::<{ ARM7 }, _>(0x027FFDE8 + i, self.rtc.date_time[i as usize]);
                }
                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Rtc, 0x9200, false);
            }
            _ => {}
        }
    }
}
