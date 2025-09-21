use crate::core::cpu_regs::InterruptFlag;
use crate::core::emu::Emu;
use crate::core::hle::arm7_hle::{IpcFifoMessage, IpcFifoTag};
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::fixed_fifo::FixedFifo;
use crate::logging::debug_println;
use crate::settings::{Arm7Emu, Settings};
use bilge::prelude::*;
use enum_dispatch::enum_dispatch;
use std::hint::unreachable_unchecked;

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
pub struct IpcSyncCnt {
    pub data_in: u4,
    not_used: u4,
    pub data_out: u4,
    // R/W
    not_used1: u1,
    pub send_irq: bool,
    pub enable_irq: bool,
    // R/W
    not_used2: u1,
}

#[bitsize(16)]
#[derive(Copy, Clone, FromBits)]
pub struct IpcFifoCnt {
    pub send_empty_status: bool,
    pub send_full_status: bool,
    pub send_empty_irq: bool,
    pub send_clear: bool,
    not_used: u4,
    pub recv_empty: bool,
    pub recv_full: bool,
    pub recv_not_empty_irq: bool,
    not_used1: u3,
    pub err: bool,
    pub enable: bool,
}

pub struct Fifo {
    pub cnt: IpcFifoCnt,
    pub queue: FixedFifo<u32, 16>,
    last_received: u32,
}

impl Fifo {
    fn new() -> Self {
        Fifo {
            cnt: IpcFifoCnt::from(0x0101),
            queue: FixedFifo::new(),
            last_received: 0,
        }
    }
}

pub struct Ipc {
    pub sync_regs: [IpcSyncCnt; 2],
    pub fifo: [Fifo; 2],
    ipc_type: IpcType,
}

#[derive(Clone)]
struct IpcLle(Arm7Emu);
#[derive(Clone)]
struct IpcHle;

#[enum_dispatch]
#[derive(Clone)]
enum IpcType {
    IpcLle,
    IpcHle,
}

#[enum_dispatch(IpcType)]
trait IpcTrait {
    fn get_fifo_cnt(&self, cpu: CpuType, emu: &Emu) -> u16;
    fn set_sync_reg(&self, cpu: CpuType, mask: u16, value: u16, emu: &mut Emu);
    fn fifo_send(&self, cpu: CpuType, mask: u32, value: u32, emu: &mut Emu);
}

impl IpcTrait for IpcLle {
    fn get_fifo_cnt(&self, cpu: CpuType, emu: &Emu) -> u16 {
        emu.ipc.fifo[cpu].cnt.into()
    }

    fn set_sync_reg(&self, cpu: CpuType, mut mask: u16, value: u16, emu: &mut Emu) {
        mask &= 0x4F00;
        emu.ipc.sync_regs[cpu] = ((u16::from(emu.ipc.sync_regs[cpu]) & !mask) | (value & mask)).into();
        emu.ipc.sync_regs[!cpu] = ((u16::from(emu.ipc.sync_regs[!cpu]) & !((mask >> 8) & 0xF)) | (((value & mask) >> 8) & 0xF)).into();

        if IpcSyncCnt::from(value).send_irq() && emu.ipc.sync_regs[!cpu].enable_irq() {
            emu.cpu_send_interrupt(!cpu, InterruptFlag::IpcSync);
        }
    }

    fn fifo_send(&self, cpu: CpuType, mask: u32, value: u32, emu: &mut Emu) {
        if emu.ipc.fifo[cpu].cnt.enable() {
            let fifo_len = emu.ipc.fifo[cpu].queue.len();
            if fifo_len < 16 {
                let message = IpcFifoMessage::from(value & mask);
                debug_println!("{cpu:?} ipc send {:x} {:x} {}", u8::from(message.tag()), u32::from(message.data()), message.err());
                if cpu == ARM9 {
                    match self.0 {
                        Arm7Emu::SoundHle => {
                            let message = IpcFifoMessage::from(value & mask);
                            match IpcFifoTag::from(u8::from(message.tag())) {
                                IpcFifoTag::Sound => {
                                    emu.sound_hle_ipc_recv(u32::from(message.data()));
                                    return;
                                }
                                _ => {}
                            }
                        }
                        Arm7Emu::Hle => unsafe { unreachable_unchecked() },
                        Arm7Emu::AccurateLle => {}
                    }
                }

                emu.ipc.fifo[cpu].queue.push_back(value & mask);

                if fifo_len == 0 {
                    emu.ipc.fifo[cpu].cnt.set_send_empty_status(false);
                    emu.ipc.fifo[!cpu].cnt.set_recv_empty(false);

                    if emu.ipc.fifo[!cpu].cnt.recv_not_empty_irq() {
                        emu.cpu_send_interrupt(!cpu, InterruptFlag::IpcRecvFifoNotEmpty);
                    }
                } else if fifo_len == 15 {
                    emu.ipc.fifo[cpu].cnt.set_send_full_status(true);
                    emu.ipc.fifo[!cpu].cnt.set_recv_full(true);
                }
            }
        } else {
            emu.ipc.fifo[cpu].cnt.set_err(true);
        }
    }
}

impl IpcTrait for IpcHle {
    fn get_fifo_cnt(&self, _: CpuType, emu: &Emu) -> u16 {
        let mut cnt = emu.ipc.fifo[ARM9].cnt;

        cnt.set_send_empty_status(false);
        cnt.set_send_full_status(false);
        cnt.set_recv_empty(false);
        cnt.set_recv_full(false);

        if emu.ipc.fifo[ARM9].queue.is_empty() {
            cnt.set_send_empty_status(true);
        } else if emu.ipc.fifo[ARM9].queue.len() == 16 {
            cnt.set_send_full_status(true);
        }

        cnt.set_send_empty_status(true);
        if emu.ipc.fifo[ARM7].queue.is_empty() {
            cnt.set_recv_empty(true);
        } else if emu.ipc.fifo[ARM7].queue.len() == 16 {
            cnt.set_recv_full(true);
        }

        cnt.into()
    }

    fn set_sync_reg(&self, _: CpuType, mut mask: u16, value: u16, emu: &mut Emu) {
        mask &= 0x4F00;
        emu.ipc.sync_regs[ARM9] = ((u16::from(emu.ipc.sync_regs[ARM9]) & !mask) | (value & mask)).into();
        emu.ipc.sync_regs[ARM7] = ((u16::from(emu.ipc.sync_regs[ARM7]) & !((mask >> 8) & 0xF)) | (((value & mask) >> 8) & 0xF)).into();

        emu.arm7_hle_ipc_sync();
    }

    fn fifo_send(&self, _: CpuType, mask: u32, value: u32, emu: &mut Emu) {
        if emu.ipc.fifo[ARM9].cnt.enable() {
            let fifo_len = emu.ipc.fifo[ARM9].queue.len();
            if fifo_len < 16 {
                let message = IpcFifoMessage::from(value & mask);
                debug_println!("hle ipc send {:x} {:x} {}", u8::from(message.tag()), u32::from(message.data()), message.err());
                emu.ipc.fifo[ARM9].queue.push_back(value & mask);

                if fifo_len == 0 {
                    emu.ipc.fifo[ARM9].cnt.set_send_empty_status(false);
                } else if fifo_len == 15 {
                    emu.ipc.fifo[ARM9].cnt.set_send_full_status(true);
                }
                emu.arm7_hle_ipc_recv();
            } else {
                emu.ipc.fifo[ARM9].cnt.set_err(true);
            }
        }
    }
}

impl Ipc {
    pub fn new() -> Self {
        Ipc {
            sync_regs: [IpcSyncCnt::from(0); 2],
            fifo: [Fifo::new(), Fifo::new()],
            ipc_type: IpcLle(Arm7Emu::AccurateLle).into(),
        }
    }

    pub fn init(&mut self, settings: &Settings) {
        self.sync_regs = [IpcSyncCnt::from(0); 2];
        self.fifo = [Fifo::new(), Fifo::new()];
        self.ipc_type = match settings.arm7_emu() {
            Arm7Emu::AccurateLle | Arm7Emu::SoundHle => IpcLle(settings.arm7_emu()).into(),
            Arm7Emu::Hle => IpcHle.into(),
        };
    }
}

impl Emu {
    pub fn ipc_get_sync_reg(&self, cpu: CpuType) -> u16 {
        self.ipc.sync_regs[cpu].into()
    }

    pub fn ipc_get_fifo_cnt(&self, cpu: CpuType) -> u16 {
        self.ipc.ipc_type.clone().get_fifo_cnt(cpu, self)
    }

    pub fn ipc_set_sync_reg(&mut self, cpu: CpuType, mask: u16, value: u16) {
        self.ipc.ipc_type.clone().set_sync_reg(cpu, mask, value, self);
    }

    pub fn ipc_set_fifo_cnt(&mut self, cpu: CpuType, mut mask: u16, value: u16) {
        let new_fifo = IpcFifoCnt::from(value);

        if new_fifo.send_clear() && !self.ipc.fifo[cpu].queue.is_empty() {
            self.ipc.fifo[cpu].queue.clear();
            self.ipc.fifo[cpu].last_received = 0;

            self.ipc.fifo[cpu].cnt.set_send_empty_status(true);
            self.ipc.fifo[cpu].cnt.set_send_full_status(false);

            self.ipc.fifo[!cpu].cnt.set_recv_empty(true);
            self.ipc.fifo[!cpu].cnt.set_recv_full(false);

            if self.ipc.fifo[cpu].cnt.send_empty_irq() {
                todo!()
            }
        }

        if self.ipc.fifo[cpu].cnt.send_empty_status() && !self.ipc.fifo[cpu].cnt.send_empty_irq() && new_fifo.send_empty_irq() {
            self.cpu_send_interrupt(cpu, InterruptFlag::IpcSendFifoEmpty);
        }

        if !self.ipc.fifo[cpu].cnt.recv_empty() && !self.ipc.fifo[cpu].cnt.recv_not_empty_irq() && new_fifo.recv_not_empty_irq() {
            self.cpu_send_interrupt(cpu, InterruptFlag::IpcRecvFifoNotEmpty);
        }

        if new_fifo.err() {
            self.ipc.fifo[cpu].cnt.set_err(false);
        }

        mask &= 0x8404;
        self.ipc.fifo[cpu].cnt = ((u16::from(self.ipc.fifo[cpu].cnt) & !mask) | (value & mask)).into();
    }

    pub fn ipc_fifo_send(&mut self, cpu: CpuType, mask: u32, value: u32) {
        self.ipc.ipc_type.clone().fifo_send(cpu, mask, value, self);
    }

    pub fn ipc_fifo_recv(&mut self, cpu: CpuType) -> u32 {
        let other_fifo_len = self.ipc.fifo[!cpu].queue.len();
        if other_fifo_len > 0 {
            self.ipc.fifo[cpu].last_received = *self.ipc.fifo[!cpu].queue.front();

            if self.ipc.fifo[cpu].cnt.enable() {
                self.ipc.fifo[!cpu].queue.pop_front();

                if other_fifo_len == 1 {
                    self.ipc.fifo[cpu].cnt.set_recv_empty(true);
                    self.ipc.fifo[!cpu].cnt.set_send_empty_status(true);

                    if self.ipc.fifo[!cpu].cnt.send_empty_irq() {
                        self.cpu_send_interrupt(!cpu, InterruptFlag::IpcSendFifoEmpty);
                    }
                } else if other_fifo_len == 16 {
                    self.ipc.fifo[cpu].cnt.set_recv_full(false);
                    self.ipc.fifo[!cpu].cnt.set_send_full_status(false);
                }
            }
        } else {
            self.ipc.fifo[cpu].cnt.set_err(true);
        }

        self.ipc.fifo[cpu].last_received
    }
}
