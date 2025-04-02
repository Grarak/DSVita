use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::IpcFifoTag;

pub(super) struct PowerManagerHle {
    data: [u16; 16],
}

impl PowerManagerHle {
    pub(super) fn new() -> Self {
        PowerManagerHle { data: [0; 16] }
    }
}

impl Emu {
    pub(super) fn power_manager_hle_ipc_recv(&mut self, data: u32) {
        if data & (1 << 25) != 0 {
            self.hle.power_manager.data.fill(0);
        }

        self.hle.power_manager.data[((data >> 16) & 0xF) as usize] = data as u16;

        if data & (1 << 24) == 0 {
            return;
        }

        let cmd = (self.hle.power_manager.data[0] >> 8) - 0x60;
        match cmd {
            1 => self.arm7_hle_send_ipc_fifo(IpcFifoTag::PowerManager, 0x0300E300, false),
            3 => self.arm7_hle_send_ipc_fifo(IpcFifoTag::PowerManager, 0x0300E300, false),
            4 => self.arm7_hle_send_ipc_fifo(IpcFifoTag::PowerManager, 0x03008000 | (((self.hle.power_manager.data[1] as u32 + 0x70) & 0xFF) << 8), false),
            5 => self.arm7_hle_send_ipc_fifo(IpcFifoTag::PowerManager, 0x03008000 | (((self.hle.power_manager.data[1] as u32 + 0x70) & 0xFF) << 8), false),
            6 => self.arm7_hle_send_ipc_fifo(IpcFifoTag::PowerManager, 0x03008000 | (((self.hle.power_manager.data[1] as u32 + 0x70) & 0xFF) << 8), false),
            _ => {}
        }
    }
}
