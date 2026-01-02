use crate::core::cpu_regs::InterruptFlag;
use crate::core::emu::Emu;
use crate::core::hle::cart_hle::CartHle;
use crate::core::hle::firmware_hle::FirmwareHle;
use crate::core::hle::mic_hle::MicHle;
use crate::core::hle::power_manager_hle::PowerManagerHle;
use crate::core::hle::rtc_hle::RtcHle;
use crate::core::hle::sound_hle::SoundHle;
use crate::core::hle::touchscreen_hle::TouchscreenHle;
use crate::core::hle::wifi_hle::WifiHle;
use crate::core::CpuType::{ARM7, ARM9};
use crate::logging::debug_println;
use bilge::prelude::*;
use std::cmp::Ordering;
use std::mem;

#[derive(Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum IpcFifoTag {
    Ex = 0,
    User0 = 1,
    User1 = 2,
    System = 3,
    Nvram = 4,
    Rtc = 5,
    Touchpanel = 6,
    Sound = 7,
    PowerManager = 8,
    Mic = 9,
    WirelessManager = 10,
    Filesystem = 11,
    Os = 12,
    Cartridge = 13,
    Card = 14,
    ControlDrivingWirelessLib = 15,
    CartridgeEx = 16,
    Max = 32,
}

impl From<u8> for IpcFifoTag {
    fn from(value: u8) -> Self {
        debug_assert!(value <= IpcFifoTag::Max as u8);
        unsafe { mem::transmute(value) }
    }
}

#[bitsize(32)]
#[derive(FromBits)]
pub struct IpcFifoMessage {
    pub tag: u5,
    pub err: bool,
    pub data: u26,
}

pub struct Arm7Hle {
    pub(super) firmware: FirmwareHle,
    pub rtc: RtcHle,
    pub touchscreen: TouchscreenHle,
    pub sound: SoundHle,
    pub(super) power_manager: PowerManagerHle,
    pub(super) mic: MicHle,
    pub(super) cart: CartHle,
    pub wifi: WifiHle,
}

impl Arm7Hle {
    pub fn new() -> Self {
        Arm7Hle {
            firmware: FirmwareHle::new(),
            rtc: RtcHle::new(),
            touchscreen: TouchscreenHle::new(),
            sound: SoundHle::new(),
            power_manager: PowerManagerHle::new(),
            mic: MicHle::new(),
            cart: CartHle::new(),
            wifi: WifiHle::new(),
        }
    }
}

impl Emu {
    pub fn arm7_hle_initialize(&mut self) {
        self.wifi_hle_initialize();
    }

    fn arm7_hle_send_ipc_sync(&mut self, val: u8) {
        self.ipc.sync_regs[ARM9].set_data_in(u4::new(val));
    }

    pub fn arm7_hle_send_ipc_fifo(&mut self, tag: IpcFifoTag, data: u32, err: bool) {
        debug_println!("hle ipc arm7 response {tag:?} {data:x} {err}");
        let fifo = &mut self.ipc.fifo[ARM7];
        if fifo.queue.len() == 16 {
            fifo.cnt.set_err(true);
            fifo.cnt.set_send_full_status(true);
            self.ipc.fifo[ARM9].cnt.set_recv_full(true);
        } else {
            fifo.queue.push_back(u32::from(IpcFifoMessage::new(u5::new(tag as u8), err, u26::new(data))));
            if fifo.queue.len() == 1 {
                self.ipc.fifo[ARM7].cnt.set_send_empty_status(false);
                self.ipc.fifo[ARM9].cnt.set_recv_empty(false);
                if self.ipc.fifo[ARM9].cnt.recv_not_empty_irq() {
                    self.cpu_send_interrupt(ARM9, InterruptFlag::IpcRecvFifoNotEmpty);
                }
            }
        }
    }

    #[cold]
    pub fn arm7_hle_ipc_recv(&mut self) {
        let val = *self.ipc.fifo[ARM9].queue.front();
        self.ipc.fifo[ARM9].queue.pop_front();
        if self.ipc.fifo[ARM9].queue.is_empty() && self.ipc.fifo[ARM9].cnt.send_empty_irq() {
            self.cpu_send_interrupt(ARM9, InterruptFlag::IpcSendFifoEmpty);
        }

        let message = IpcFifoMessage::from(val);
        let tag = IpcFifoTag::from(u8::from(message.tag()));
        let data = u32::from(message.data());

        match tag {
            IpcFifoTag::Nvram => {
                if !message.err() {
                    self.firmware_hle_ipc_recv(data);
                }
            }
            IpcFifoTag::Rtc => {
                if !message.err() {
                    self.rtc_hle_ipc_recv(data);
                }
            }
            IpcFifoTag::Touchpanel => {
                if !message.err() {
                    self.touchscreen_hle_ipc_recv(data);
                }
            }
            IpcFifoTag::Sound => self.sound_hle_ipc_recv(data),
            IpcFifoTag::PowerManager => {
                if !message.err() {
                    self.power_manager_hle_ipc_recv(data);
                }
            }
            IpcFifoTag::Mic => {
                if !message.err() {
                    self.mic_hle_ipc_recv(data);
                }
            }
            IpcFifoTag::WirelessManager => {
                if !message.err() {
                    self.wifi_hle_ipc_recv(data);
                }
            }
            IpcFifoTag::Filesystem => {
                if message.err() {
                    self.cart_hle_ipc_recv(data);
                }
            }
            IpcFifoTag::Os => {
                if data == 0x1000 {
                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Os, 0x1000, false);
                }
            }
            IpcFifoTag::Cartridge => {
                if (data & 0x3F) == 1 {
                    self.arm7_hle_send_ipc_fifo(IpcFifoTag::Cartridge, 0x1, false);
                }
            }
            IpcFifoTag::ControlDrivingWirelessLib => self.arm7_hle_send_ipc_fifo(IpcFifoTag::ControlDrivingWirelessLib, data, false),
            _ => todo!("hle: ipc request {val:x} tag {tag:?} data {data:x} err {}", message.err()),
        }
    }

    pub fn arm7_hle_ipc_sync(&mut self) {
        let data_in = u8::from(self.ipc.sync_regs[ARM7].data_in());
        match data_in.cmp(&5) {
            Ordering::Less => self.arm7_hle_send_ipc_sync(data_in + 1),
            Ordering::Equal => {
                self.arm7_hle_send_ipc_sync(0);
                self.mem_write::<{ ARM7 }, u32>(0x027FFF8C, 0x3fff0);
            }
            _ => {}
        }
    }

    pub fn arm7_hle_on_scanline(&mut self, v_count: u16) {
        self.touchscreen_hle_on_scanline(v_count);
    }

    pub fn arm7_hle_on_frame(&mut self) {
        self.mem_write::<{ ARM7 }, _>(0x027FFFA8, (self.input.get_ext_key_in() << 10) & 0x2C00);
        let frame_counter = self.mem_read::<{ ARM7 }, u32>(0x27FFC3C);
        self.mem_write::<{ ARM7 }, _>(0x27FFC3C, frame_counter.wrapping_add(1));

        if self.nitro_sdk_version.is_twl_sdk() {
            let sync = self.mem_read::<{ ARM7 }, u8>(0x2FFFFF0);
            self.mem_write::<{ ARM7 }, u8>(0x2FFFFF1, !sync);
            self.mem_write::<{ ARM7 }, u8>(0x2FFFFF2, 1);
            self.mem_write::<{ ARM7 }, u8>(0x2FFFFF3, 1);
        }
    }
}
