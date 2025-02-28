use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::{Arm7Hle, IpcFifoTag};
use crate::core::spi;
use crate::core::CpuType::ARM7;

pub(super) struct FirmwareHle {
    data: [u16; 16],
}

impl FirmwareHle {
    pub(super) fn new() -> Self {
        FirmwareHle { data: [0; 16] }
    }

    pub(super) fn ipc_recv(&mut self, data: u32, emu: &mut Emu) {
        if data & (1 << 25) != 0 {
            self.data.fill(0);
        }

        self.data[((data >> 16) & 0xF) as usize] = data as u16;

        if data & (1 << 24) == 0 {
            return;
        }

        let cmd = (self.data[0] >> 8) - 0x20;
        match cmd {
            0 => {
                Arm7Hle::send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A000, false, emu);
            }
            1 => {
                Arm7Hle::send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A100, false, emu);
            }
            2 => {
                let addr = (((self.data[0] as u32) & 0xFF) << 24) | ((self.data[1] as u32) << 8) | (((self.data[2] as u32) >> 8) & 0xFF);
                if (0x02000000..0x02800000).contains(&addr) {
                    Arm7Hle::send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A200, false, emu);
                } else {
                    Arm7Hle::send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A202, false, emu);
                }
            }
            3 => {
                let addr = ((self.data[4] as u32) << 16) | self.data[5] as u32;
                if (0x02000000..0x02800000).contains(&addr) {
                    let src = (((self.data[0] as u32) & 0xFF) << 16) | self.data[1] as u32;
                    let len = ((self.data[2] as u32) << 16) | self.data[3] as u32;

                    for i in 0..len {
                        let val = spi::SPI_FIRMWARE[(src + i) as usize % spi::SPI_FIRMWARE.len()];
                        emu.mem_write::<{ ARM7 }, _>(addr + i, val);
                    }

                    Arm7Hle::send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A300, false, emu);
                } else {
                    Arm7Hle::send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A302, false, emu);
                }
            }
            5 => {
                let addr = ((self.data[3] as u32) << 16) | self.data[4] as u32;
                if (0x02000000..0x02800000).contains(&addr) {
                    Arm7Hle::send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A500, false, emu);
                } else {
                    Arm7Hle::send_ipc_fifo(IpcFifoTag::Nvram, 0x0300A502, false, emu);
                }
            }
            _ => {}
        }
    }
}
