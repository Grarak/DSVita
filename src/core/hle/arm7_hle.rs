use crate::core::cpu_regs::InterruptFlag;
use crate::core::emu::{get_common, get_cpu_regs_mut, get_ipc, get_ipc_mut, Emu};
use crate::core::hle::cart_hle::CartHle;
use crate::core::hle::firmware_hle::FirmwareHle;
use crate::core::hle::mic_hle::MicHle;
use crate::core::hle::power_manager_hle::PowerManagerHle;
use crate::core::hle::rtc_hle::RtcHle;
use crate::core::hle::sound_hle::SoundHle;
use crate::core::hle::touchscreen_hle::TouchscreenHle;
use crate::core::hle::wifi_hle::WifiHle;
use crate::core::CpuType::{ARM7, ARM9};
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
    firmware: FirmwareHle,
    pub rtc: RtcHle,
    pub touchscreen: TouchscreenHle,
    pub sound: SoundHle,
    power_manager: PowerManagerHle,
    mic: MicHle,
    cart: CartHle,
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

    pub fn initialize(emu: &mut Emu) {
        WifiHle::initialize(emu);
    }

    fn send_ipc_sync(val: u8, emu: &mut Emu) {
        get_ipc_mut!(emu).sync_regs[ARM9].set_data_in(u4::new(val));
    }

    pub fn send_ipc_fifo(tag: IpcFifoTag, data: u32, err: bool, emu: &mut Emu) {
        let ipc = get_ipc_mut!(emu);
        let fifo = &mut ipc.fifo[ARM7];
        if fifo.queue.len() == 16 {
            fifo.cnt.set_err(true);
            fifo.cnt.set_send_full_status(true);
            ipc.fifo[ARM9].cnt.set_recv_full(true);
        } else {
            fifo.queue.push_back(u32::from(IpcFifoMessage::new(u5::new(tag as u8), err, u26::new(data))));
            if fifo.queue.len() == 1 {
                ipc.fifo[ARM7].cnt.set_send_empty_status(false);
                ipc.fifo[ARM9].cnt.set_recv_empty(false);
                if ipc.fifo[ARM9].cnt.recv_not_empty_irq() {
                    get_cpu_regs_mut!(emu, ARM9).send_interrupt(InterruptFlag::IpcRecvFifoNotEmpty, emu);
                }
            }
        }
    }

    #[cold]
    pub fn ipc_recv(&mut self, emu: &mut Emu) {
        let ipc = get_ipc_mut!(emu);
        let val = *ipc.fifo[ARM9].queue.front();
        ipc.fifo[ARM9].queue.pop_front();
        if ipc.fifo[ARM9].queue.is_empty() && ipc.fifo[ARM9].cnt.send_empty_irq() {
            get_cpu_regs_mut!(emu, ARM9).send_interrupt(InterruptFlag::IpcSendFifoEmpty, emu);
        }

        let message = IpcFifoMessage::from(val);
        let tag = IpcFifoTag::from(u8::from(message.tag()));
        let data = u32::from(message.data());

        match tag {
            IpcFifoTag::Nvram => {
                if !message.err() {
                    self.firmware.ipc_recv(data, emu);
                }
            }
            IpcFifoTag::Rtc => {
                if !message.err() {
                    self.rtc.ipc_recv(data, emu);
                }
            }
            IpcFifoTag::Touchpanel => {
                if !message.err() {
                    self.touchscreen.ipc_recv(data, emu);
                }
            }
            IpcFifoTag::Sound => self.sound.ipc_recv(data, emu),
            IpcFifoTag::PowerManager => {
                if !message.err() {
                    self.power_manager.ipc_recv(data, emu);
                }
            }
            IpcFifoTag::Mic => {
                if !message.err() {
                    self.mic.ipc_recv(data, emu);
                }
            }
            IpcFifoTag::WirelessManager => {
                if !message.err() {
                    self.wifi.ipc_recv(data, emu);
                }
            }
            IpcFifoTag::Filesystem => {
                if message.err() {
                    self.cart.ipc_recv(data, emu);
                }
            }
            IpcFifoTag::Os => {
                if data == 0x1000 {
                    Self::send_ipc_fifo(IpcFifoTag::Os, 0x1000, false, emu);
                }
            }
            IpcFifoTag::Cartridge => {
                if (data & 0x3F) == 1 {
                    Self::send_ipc_fifo(IpcFifoTag::Cartridge, 0x1, false, emu);
                }
            }
            IpcFifoTag::ControlDrivingWirelessLib => {
                Self::send_ipc_fifo(IpcFifoTag::ControlDrivingWirelessLib, data, false, emu);
            }
            _ => todo!("hle: ipc request {val:x} tag {tag:?} data {data:x} err {}", message.err()),
        }
    }

    pub fn ipc_sync(emu: &mut Emu) {
        let data_in = u8::from(get_ipc!(emu).sync_regs[ARM7].data_in());
        match data_in.cmp(&5) {
            Ordering::Less => Self::send_ipc_sync(data_in + 1, emu),
            Ordering::Equal => {
                Self::send_ipc_sync(0, emu);
                emu.mem_write::<{ ARM7 }, u32>(0x027FFF8C, 0x3fff0);
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
