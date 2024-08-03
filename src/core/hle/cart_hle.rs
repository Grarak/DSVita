use crate::core::emu::{get_common_mut, Emu};
use crate::core::hle::arm7_hle::Arm7Hle;
use crate::core::CpuType::ARM7;
use crate::logging::debug_println;

pub(super) struct CartHle {
    cmd: u32,
    data_pos: u32,
    buffer: u32,
}

impl CartHle {
    pub(super) fn new() -> Self {
        CartHle { cmd: 0, data_pos: 0, buffer: 0 }
    }

    pub(super) fn ipc_recv(&mut self, data: u32, emu: &mut Emu) {
        if self.data_pos == 0 {
            self.cmd = data;
        }

        match self.cmd {
            0 => {
                if self.data_pos == 1 {
                    self.buffer = data;
                    Arm7Hle::send_ipc_fifo(0xB, 0x1, 1, emu);
                    self.data_pos = 0;
                    return;
                }
            }
            2 => {
                let save_param = emu.mem_read::<{ ARM7 }, u32>(self.buffer + 0x4);
                let save_size_shift = (save_param >> 8) & 0xFF;
                get_common_mut!(emu).cartridge.io.resize_save_file(1 << save_size_shift);
                Arm7Hle::send_ipc_fifo(0xB, 0x1, 1, emu);
                self.data_pos = 0;
                return;
            }
            6 => {
                let offset = emu.mem_read::<{ ARM7 }, u32>(self.buffer + 0x0C);
                let dst = emu.mem_read::<{ ARM7 }, u32>(self.buffer + 0x10);
                let len = emu.mem_read::<{ ARM7 }, u32>(self.buffer + 0x14);

                let cartridge_io = &mut get_common_mut!(emu).cartridge.io;
                for i in 0..len {
                    emu.mem_write::<{ ARM7 }, u8>(dst + i, cartridge_io.read_save_buf((offset + i) & (cartridge_io.save_file_size - 1)));
                }

                Arm7Hle::send_ipc_fifo(0xB, 0x1, 1, emu);
                self.data_pos = 0;
                return;
            }
            7 | 8 => {
                let src = emu.mem_read::<{ ARM7 }, u32>(self.buffer + 0x0C);
                let offset = emu.mem_read::<{ ARM7 }, u32>(self.buffer + 0x10);
                let len = emu.mem_read::<{ ARM7 }, u32>(self.buffer + 0x14);

                let mut buf = vec![0xFF; len as usize];
                for i in 0..len {
                    buf[i as usize] = emu.mem_read::<{ ARM7 }, u8>(src + i);
                }
                let cartridge_io = &mut get_common_mut!(emu).cartridge.io;
                cartridge_io.write_save_buf_slice(offset & (cartridge_io.save_file_size - 1), &buf);

                Arm7Hle::send_ipc_fifo(0xB, 0x1, 1, emu);
                self.data_pos = 0;
                return;
            }
            9 => {
                Arm7Hle::send_ipc_fifo(0xB, 0x1, 1, emu);
                self.data_pos = 0;
                return;
            }
            _ => {
                debug_println!("cart save: unknown cmd {:x}", self.cmd);
            }
        }

        self.data_pos += 1;
    }
}
