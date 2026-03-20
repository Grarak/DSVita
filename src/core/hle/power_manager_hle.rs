use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::IpcFifoTag;

const START_BIT: u32 = 0x02000000;
const END_BIT: u32 = 0x01000000;

const CMD_SYNC: u16 = 0;
const CMD_SLEEP_START: u16 = 1;
const CMD_SLEEP_END: u16 = 2;
const CMD_UTILITY: u16 = 3;
const CMD_REG_WRITE: u16 = 4;
const CMD_REG_READ: u16 = 5;
const CMD_SELF_BLINK: u16 = 6;
const CMD_GET_BLINK: u16 = 7;
const CMD_REG0_VALUE: u16 = 16;
const CMD_REG1_VALUE: u16 = 17;
const CMD_REG2_VALUE: u16 = 18;
const CMD_REG3_VALUE: u16 = 19;
const CMD_REG4_VALUE: u16 = 20;

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
        if data & START_BIT != 0 {
            self.hle.power_manager.data.fill(0);
        }

        let seq = (data >> 16) & 0xF;
        self.hle.power_manager.data[seq as usize] = data as u16;

        if data & END_BIT == 0 {
            return;
        }

        let cmd = (self.hle.power_manager.data[0] >> 8) - 0x60;
        match cmd {
            CMD_SLEEP_START => self.arm7_hle_send_ipc_fifo(IpcFifoTag::PowerManager, 0x0300E300, false),
            CMD_UTILITY => self.arm7_hle_send_ipc_fifo(IpcFifoTag::PowerManager, 0x0300E300, false),
            CMD_REG_WRITE => self.arm7_hle_send_ipc_fifo(IpcFifoTag::PowerManager, 0x0300E400, false),
            _ => self.arm7_hle_send_ipc_fifo(IpcFifoTag::PowerManager, 0x03008000 | (((self.hle.power_manager.data[1] as u32 + 0x70) & 0xFF) << 8), false),
        }
    }
}
