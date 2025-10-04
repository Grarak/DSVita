use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::IpcFifoTag;
use crate::core::CpuType::ARM7;

pub struct TouchscreenHle {
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
}

impl Emu {
    fn touchscreen_hle_sample(&mut self) {
        let mut ts = (self.mem_read::<{ ARM7 }, u16>(0x027FFFAA) as u32) | ((self.mem_read::<{ ARM7 }, u16>(0x027FFFAC) as u32) << 16);

        let is_pressed = self.input.get_ext_key_in() & 0x40 == 0;
        if is_pressed {
            let (x, y) = self.spi.get_touch_coordinates();
            ts &= 0xF9000000;
            ts |= (x & 0xFFF) as u32;
            ts |= ((y & 0xFFF) as u32) << 12;
            ts |= 0x01000000;
        } else {
            ts &= 0xFE000000;
            ts |= 0x06000000;
        }

        self.mem_write::<{ ARM7 }, _>(0x027FFFAA, ts as u16);
        self.mem_write::<{ ARM7 }, _>(0x027FFFAC, (ts >> 16) as u16);
    }

    pub fn touchscreen_hle_ipc_recv(&mut self, data: u32) {
        let touchscreen = &mut self.hle.touchscreen;

        if data & (1 << 25) != 0 {
            touchscreen.data.fill(0);
        }

        touchscreen.data[((data >> 16) & 0xF) as usize] = data as u16;

        if data & (1 << 24) == 0 {
            return;
        }

        match touchscreen.data[0] >> 8 {
            0 => {
                self.touchscreen_hle_sample();
                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Touchpanel, 0x03008000, false);
            }
            1 => {
                if touchscreen.status != 0 {
                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Touchpanel, 0x03008103, false);
                    return;
                }

                let num = touchscreen.data[0] & 0xFF;
                if num == 0 || num > 4 {
                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Touchpanel, 0x03008102, false);
                    return;
                }

                let offset = touchscreen.data[1];
                if offset >= 263 {
                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Touchpanel, 0x03008102, false);
                    return;
                }

                touchscreen.status = 1;

                touchscreen.num_samples = num;
                for i in 0..num {
                    let y_pos = offset + ((i * 263 / num) % 263);
                    touchscreen.sample_pos[i as usize] = y_pos;
                }

                touchscreen.status = 2;
                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Touchpanel, 0x03008100, false);
            }
            2 => {
                if touchscreen.status != 2 {
                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Touchpanel, 0x03008203, false);
                    return;
                }

                touchscreen.status = 3;
                touchscreen.num_samples = 0;
                touchscreen.status = 0;
                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Touchpanel, 0x03008200, false);
            }
            3 => {
                self.touchscreen_hle_sample();
                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Touchpanel, 0x03008300, false);
            }
            _ => {}
        }
    }

    pub(super) fn touchscreen_hle_on_scanline(&mut self, v_count: u16) {
        for i in 0..self.hle.touchscreen.num_samples as usize {
            if v_count == self.hle.touchscreen.sample_pos[i] {
                self.touchscreen_hle_sample();
                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Touchpanel, 0x03009000 | i as u32, false);
                break;
            }
        }
    }
}
