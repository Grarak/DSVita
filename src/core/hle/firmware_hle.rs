use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::IpcFifoTag;
use crate::core::CpuType::ARM7;

pub(super) struct FirmwareHle {
    data: [u16; 16],
}

impl FirmwareHle {
    pub(super) fn new() -> Self {
        FirmwareHle { data: [0; 16] }
    }
}

impl Emu {
    pub(super) fn firmware_hle_ipc_recv(&mut self, data: u32) {
        let firmware = &mut self.hle.firmware;

        if data & (1 << 25) != 0 {
            firmware.data.fill(0);
        }

        firmware.data[((data >> 16) & 0xF) as usize] = data as u16;

        if data & (1 << 24) == 0 {
            return;
        }

        let cmd = (firmware.data[0] >> 8) - 0x20;
        match cmd {
            0 => self.arm7_hle_send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A000, false),
            1 => self.arm7_hle_send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A100, false),
            2 => {
                let addr = (((firmware.data[0] as u32) & 0xFF) << 24) | ((firmware.data[1] as u32) << 8) | (((firmware.data[2] as u32) >> 8) & 0xFF);
                if (0x02000000..0x02800000).contains(&addr) {
                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A200, false);
                } else {
                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A202, false);
                }
            }
            3 => {
                let addr = ((firmware.data[4] as u32) << 16) | firmware.data[5] as u32;
                if (0x02000000..0x02800000).contains(&addr) {
                    let src = (((firmware.data[0] as u32) & 0xFF) << 16) | firmware.data[1] as u32;
                    let len = ((firmware.data[2] as u32) << 16) | firmware.data[3] as u32;

                    for i in 0..len {
                        let val = self.spi.firmware[(src + i) as usize % self.spi.firmware.len()];
                        self.mem_write::<{ ARM7 }, _>(addr + i, val);
                    }

                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A300, false);
                } else {
                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A302, false);
                }
            }
            5 => {
                let addr = ((firmware.data[3] as u32) << 16) | firmware.data[4] as u32;
                if (0x02000000..0x02800000).contains(&addr) {
                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A500, false);
                } else {
                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A502, false);
                }
            }
            _ => {}
        }
    }
}
