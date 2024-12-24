use crate::core::cpu_regs::InterruptFlag;
use crate::core::emu::{get_common, get_cpu_regs_mut, get_ipc, get_ipc_mut, Emu};
use crate::core::hle::cart_hle::CartHle;
use crate::core::hle::firmware_hle::FirmwareHle;
use crate::core::hle::power_manager_hle::PowerManagerHle;
use crate::core::hle::rtc_hle::RtcHle;
use crate::core::hle::sound_hle::SoundHle;
use crate::core::hle::touchscreen_hle::TouchscreenHle;
use crate::core::CpuType::{ARM7, ARM9};
use bilge::prelude::*;
use std::cmp::Ordering;

pub struct Arm7Hle {
    firmware: FirmwareHle,
    rtc: RtcHle,
    touchscreen: TouchscreenHle,
    pub(super) sound: SoundHle,
    power_manager: PowerManagerHle,
    cart: CartHle,
}

impl Arm7Hle {
    pub fn new() -> Self {
        Arm7Hle {
            firmware: FirmwareHle::new(),
            rtc: RtcHle::new(),
            touchscreen: TouchscreenHle::new(),
            sound: SoundHle::new(),
            power_manager: PowerManagerHle::new(),
            cart: CartHle::new(),
        }
    }

    fn send_ipc_sync(val: u8, emu: &mut Emu) {
        get_ipc_mut!(emu).sync_regs[ARM9].set_data_in(u4::new(val));
    }

    pub(super) fn send_ipc_fifo(service: u32, data: u32, flag: u32, emu: &mut Emu) {
        let val = (service & 0x1F) | (data << 6) | ((flag & 0x1) << 5);

        let fifo = &mut get_ipc_mut!(emu).fifo[ARM7];
        if fifo.queue.len() == 16 {
            fifo.cnt.set_err(true);
        } else {
            fifo.queue.push_back(val);
            if fifo.queue.len() == 1 {
                get_cpu_regs_mut!(emu, ARM9).send_interrupt(InterruptFlag::IpcRecvFifoNotEmpty, emu);
            }
        }
    }

    pub fn ipc_recv(&mut self, emu: &mut Emu) {
        let val = *get_ipc!(emu).fifo[ARM9].queue.front();
        get_ipc_mut!(emu).fifo[ARM9].queue.pop_front();
        if get_ipc!(emu).fifo[ARM9].queue.is_empty() && get_ipc!(emu).fifo[ARM9].cnt.send_empty_irq() {
            get_cpu_regs_mut!(emu, ARM9).send_interrupt(InterruptFlag::IpcSendFifoEmpty, emu);
        }

        let service = val & 0x1F;
        let data = val >> 6;
        let flag = ((val >> 5) & 0x1) != 0;

        match service {
            0x4 => {
                if !flag {
                    self.firmware.ipc_recv(data, emu);
                }
            }
            0x5 => {
                if !flag {
                    self.rtc.ipc_recv(data, emu);
                }
            }
            0x6 => {
                if !flag {
                    self.touchscreen.ipc_recv(data, emu);
                }
            }
            0x7 => {
                self.sound.ipc_recv(data, emu);
            }
            0x8 => {
                if !flag {
                    self.power_manager.ipc_recv(data, emu);
                }
            }
            0x9 => {
                // Mic
                if !flag {
                    // todo!()
                }
            }
            0xA => {
                // Wifi
                if !flag {
                    // todo!()
                }
            }
            0xB => {
                if flag {
                    self.cart.ipc_recv(data, emu);
                }
            }
            0xC => {
                if data == 0x1000 {
                    Self::send_ipc_fifo(0xC, 0x1000, 0, emu);
                }
            }
            0xD => {
                // Cart
                if (data & 0x3F) == 1 {
                    Self::send_ipc_fifo(0xD, 0x1, 0, emu);
                }
            }
            0xF => {
                if data == 0x10000 {
                    Self::send_ipc_fifo(0xF, 0x10000, 0, emu);
                }
            }
            _ => {
                todo!("hle: Unknown ipc request {:x} service {:x} data {:x} flag {}", val, service, data, flag)
            }
        }
    }

    pub fn ipc_sync(emu: &mut Emu) {
        let data_in = u8::from(get_ipc!(emu).sync_regs[ARM7].data_in());
        match data_in.cmp(&5) {
            Ordering::Less => Self::send_ipc_sync(data_in + 1, emu),
            Ordering::Equal => {
                Self::send_ipc_sync(0, emu);
                emu.mem_write::<{ ARM7 }, u32>(0x027FFF8C, 0x0000FFF0);
            }
            _ => {}
        }
    }

    pub fn on_scanline(&self, v_count: u16, emu: &mut Emu) {
        self.touchscreen.on_scanline(v_count, emu);
    }

    pub fn on_frame(emu: &mut Emu) {
        emu.mem_write::<{ ARM7 }, _>(0x027FFFA8, (get_common!(emu).input.get_ext_key_in() << 10) & 0x2C00);
        let frame_counter = emu.mem_read::<{ ARM7 }, u32>(0x27FFC3C);
        emu.mem_write::<{ ARM7 }, _>(0x27FFC3C, frame_counter.wrapping_add(1));
    }
}
