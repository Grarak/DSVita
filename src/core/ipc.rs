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

impl Default for IpcSyncCnt {
    fn default() -> Self {
        IpcSyncCnt::from(0)
    }
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

impl Default for IpcFifoCnt {
    fn default() -> Self {
        IpcFifoCnt::from(0x0101)
    }
}

pub struct Fifo {
    pub cnt: IpcFifoCnt,
    pub queue: FixedFifo<u32, 16>,
    last_received: u32,
}

impl Fifo {
    fn new() -> Self {
        Fifo {
            cnt: IpcFifoCnt::default(),
            queue: FixedFifo::new(),
            last_received: 0,
        }
    }
}

pub struct Ipc {
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
    fn set_sync_reg(&self, cpu: CpuType, cnt: IpcSyncCnt, remote_cnt: IpcSyncCnt, emu: &mut Emu);
    fn fifo_send(&self, cpu: CpuType, value: u32, emu: &mut Emu);
}

impl IpcTrait for IpcLle {
    fn get_fifo_cnt(&self, cpu: CpuType, emu: &Emu) -> u16 {
        emu.ipc.fifo[cpu].cnt.into()
    }

    fn set_sync_reg(&self, cpu: CpuType, cnt: IpcSyncCnt, remote_cnt: IpcSyncCnt, emu: &mut Emu) {
        if cnt.send_irq() && remote_cnt.enable_irq() {
            emu.cpu_send_interrupt(!cpu, InterruptFlag::IpcSync);
        }
    }

    fn fifo_send(&self, cpu: CpuType, value: u32, emu: &mut Emu) {
        if emu.ipc.fifo[cpu].cnt.enable() {
            let fifo_len = emu.ipc.fifo[cpu].queue.len();
            if fifo_len < 16 {
                let message = IpcFifoMessage::from(value);
                debug_println!("{cpu:?} ipc send {:x} {:x} {}", u8::from(message.tag()), u32::from(message.data()), message.err());
                if cpu == ARM9 {
                    match self.0 {
                        Arm7Emu::SoundHle => {
                            let message = IpcFifoMessage::from(value);
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

                emu.ipc.fifo[cpu].queue.push_back(value);

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

    fn set_sync_reg(&self, _: CpuType, _: IpcSyncCnt, _: IpcSyncCnt, emu: &mut Emu) {
        emu.arm7_hle_ipc_sync();
    }

    fn fifo_send(&self, _: CpuType, value: u32, emu: &mut Emu) {
        if emu.ipc.fifo[ARM9].cnt.enable() {
            let fifo_len = emu.ipc.fifo[ARM9].queue.len();
            if fifo_len < 16 {
                let message = IpcFifoMessage::from(value);
                debug_println!("hle ipc send {:x} {:x} {}", u8::from(message.tag()), u32::from(message.data()), message.err());
                emu.ipc.fifo[ARM9].queue.push_back(value);

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
            fifo: [Fifo::new(), Fifo::new()],
            ipc_type: IpcLle(Arm7Emu::AccurateLle).into(),
        }
    }

    pub fn init(&mut self, settings: &Settings) {
        self.fifo = [Fifo::new(), Fifo::new()];
        self.ipc_type = match settings.arm7_emu() {
            Arm7Emu::AccurateLle | Arm7Emu::SoundHle => IpcLle(settings.arm7_emu()).into(),
            Arm7Emu::Hle => IpcHle.into(),
        };
    }
}

impl Emu {
    pub fn ipc_get_fifo_cnt(&mut self, cpu: CpuType) {
        let value = self.ipc.ipc_type.clone().get_fifo_cnt(cpu, self);
        self.mem.io.ipc_fifo_cnt(cpu).value = value;
    }

    pub fn ipc_set_sync_reg(&mut self, cpu: CpuType) {
        let sync_reg = self.mem.io.ipc_sync_reg(cpu);
        let data_out = sync_reg.data_out();
        let cnt = *sync_reg;
        sync_reg.value &= 0x4F00;

        let remote_sync_reg = self.mem.io.ipc_sync_reg(!cpu);
        remote_sync_reg.set_data_in(data_out);

        self.ipc.ipc_type.clone().set_sync_reg(cpu, cnt, *remote_sync_reg, self);
    }

    pub fn ipc_set_fifo_cnt(&mut self, cpu: CpuType) {
        let new_fifo = *self.mem.io.ipc_fifo_cnt(cpu);

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

        let mask = 0x8404;
        self.ipc.fifo[cpu].cnt = ((u16::from(self.ipc.fifo[cpu].cnt) & !mask) | (new_fifo.value & mask)).into();
    }

    pub fn ipc_fifo_send(&mut self, cpu: CpuType) {
        let value = *self.mem.io.ipc_fifo_send(cpu);
        *self.mem.io.ipc_fifo_send(cpu) = 0;
        self.ipc.ipc_type.clone().fifo_send(cpu, value, self);
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
