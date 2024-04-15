use crate::emu::emu::{get_common, get_spi, Emu};
use crate::emu::hle::arm7_hle::Arm7Hle;
use crate::emu::CpuType::ARM7;

pub(super) struct TouchscreenHle {
    status: i32,
    data: [u16; 16],
    num_samples: u16,
    sample_pos: [u16; 4],
}

impl TouchscreenHle {
    pub(super) fn new() -> Self {
        TouchscreenHle {
            status: 0,
            data: [0; 16],
            num_samples: 0,
            sample_pos: [0; 4],
        }
    }

    fn sample(&self, emu: &mut Emu) {
        let mut ts = (emu.mem_read::<{ ARM7 }, u16>(0x027FFFAA) as u32)
            | ((emu.mem_read::<{ ARM7 }, u16>(0x027FFFAC) as u32) << 16);

        let is_pressed = get_common!(emu).input.get_ext_key_in() & 0x40 == 0;
        if is_pressed {
            let (x, y) = get_spi!(emu).get_touch_coordinates();
            ts &= 0xF9000000;
            ts |= (x & 0xFFF) as u32;
            ts |= ((y & 0xFFF) as u32) << 12;
            ts |= 0x01000000;
        } else {
            ts &= 0xFE000000;
            ts |= 0x06000000;
        }

        emu.mem_write::<{ ARM7 }, _>(0x027FFFAA, ts as u16);
        emu.mem_write::<{ ARM7 }, _>(0x027FFFAC, (ts >> 16) as u16);
    }

    pub(super) fn ipc_recv(&mut self, data: u32, emu: &mut Emu) {
        if data & (1 << 25) != 0 {
            self.data.fill(0);
        }

        self.data[((data >> 16) & 0xF) as usize] = data as u16;

        if data & (1 << 24) == 0 {
            return;
        }

        match self.data[0] >> 8 {
            0 => {
                self.sample(emu);
                Arm7Hle::send_ipc_fifo(0x6, 0x03008000, 0, emu);
            }
            1 => {
                if self.status != 0 {
                    Arm7Hle::send_ipc_fifo(0x6, 0x03008103, 0, emu);
                    return;
                }

                let num = self.data[0] & 0xFF;
                if num == 0 || num > 4 {
                    Arm7Hle::send_ipc_fifo(0x6, 0x03008102, 0, emu);
                    return;
                }

                let offset = self.data[1];
                if offset >= 263 {
                    Arm7Hle::send_ipc_fifo(0x6, 0x03008102, 0, emu);
                    return;
                }

                self.status = 1;

                self.num_samples = num;
                for i in 0..num {
                    let y_pos = offset + ((i * 263 / num) % 263);
                    self.sample_pos[i as usize] = y_pos;
                }

                self.status = 2;
                Arm7Hle::send_ipc_fifo(0x6, 0x03008100, 0, emu);
            }
            2 => {
                if self.status != 2 {
                    Arm7Hle::send_ipc_fifo(0x6, 0x03008103, 0, emu);
                    return;
                }

                self.status = 3;
                self.num_samples = 0;
                self.status = 0;
                Arm7Hle::send_ipc_fifo(0x6, 0x03008200, 0, emu);
            }
            3 => {
                self.sample(emu);
                Arm7Hle::send_ipc_fifo(0x6, 0x03008300, 0, emu);
            }
            _ => {}
        }
    }

    pub(super) fn on_scanline(&self, v_count: u16, emu: &mut Emu) {
        for i in 0..self.num_samples as usize {
            if v_count == self.sample_pos[i] {
                self.sample(emu);
                Arm7Hle::send_ipc_fifo(0x6, 0x03009000 | i as u32, 0, emu);
                break;
            }
        }
    }
}
