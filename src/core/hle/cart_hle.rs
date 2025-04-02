use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::IpcFifoTag;
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
}

impl Emu {
    pub(super) fn cart_hle_ipc_recv(&mut self, data: u32) {
        if self.hle.cart.data_pos == 0 {
            self.hle.cart.cmd = data;
        }

        match self.hle.cart.cmd {
            0 => {
                if self.hle.cart.data_pos == 1 {
                    self.hle.cart.buffer = data;
                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Filesystem, 0x1, true);
                    self.hle.cart.data_pos = 0;
                    return;
                }
            }
            2 => {
                if self.cartridge.io.save_file_size == 0 {
                    let save_param = self.mem_read::<{ ARM7 }, u32>(self.hle.cart.buffer + 0x4);
                    let save_size_shift = (save_param >> 8) & 0xFF;
                    self.cartridge.io.resize_save_file(1 << save_size_shift);
                }
                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Filesystem, 0x1, true);
                self.hle.cart.data_pos = 0;
                return;
            }
            6 => {
                let offset = self.mem_read::<{ ARM7 }, u32>(self.hle.cart.buffer + 0x0C);
                let dst = self.mem_read::<{ ARM7 }, u32>(self.hle.cart.buffer + 0x10);
                let len = self.mem_read::<{ ARM7 }, u32>(self.hle.cart.buffer + 0x14);

                for i in 0..len {
                    self.mem_write::<{ ARM7 }, u8>(dst + i, self.cartridge.io.read_save_buf((offset + i) & (self.cartridge.io.save_file_size - 1)));
                }

                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Filesystem, 0x1, true);
                self.hle.cart.data_pos = 0;
                return;
            }
            7 | 8 => {
                let src = self.mem_read::<{ ARM7 }, u32>(self.hle.cart.buffer + 0x0C);
                let offset = self.mem_read::<{ ARM7 }, u32>(self.hle.cart.buffer + 0x10);
                let len = self.mem_read::<{ ARM7 }, u32>(self.hle.cart.buffer + 0x14);

                let mut buf = vec![0xFF; len as usize];
                for i in 0..len {
                    buf[i as usize] = self.mem_read::<{ ARM7 }, u8>(src + i);
                }
                let cartridge_io = &mut self.cartridge.io;
                cartridge_io.write_save_buf_slice(offset & (cartridge_io.save_file_size - 1), &buf);

                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Filesystem, 0x1, true);
                self.hle.cart.data_pos = 0;
                return;
            }
            9 => {
                self.arm7_hle_send_ipc_fifo(IpcFifoTag::Filesystem, 0x1, true);
                self.hle.cart.data_pos = 0;
                return;
            }
            _ => debug_println!("cart save: unknown cmd {:x}", self.hle.cart.cmd),
        }

        self.hle.cart.data_pos += 1;
    }
}
