use crate::emu::cpu_regs::InterruptFlag;
use crate::emu::emu::{get_cm, get_cpu_regs_mut, Emu};
use crate::emu::CpuType;
use crate::logging::debug_println;
use bilge::prelude::*;
use std::collections::VecDeque;

#[bitsize(16)]
#[derive(FromBits)]
struct IpcSyncCnt {
    data_in: u4,
    not_used: u4,
    data_out: u4,
    // R/W
    not_used1: u1,
    send_irq: u1,
    enable_irq: u1,
    // R/W
    not_used2: u1,
}

#[bitsize(16)]
#[derive(Clone, FromBits)]
struct IpcFifoCnt {
    send_empty_status: u1,
    send_full_status: u1,
    send_empty_irq: u1,
    send_clear: u1,
    not_used: u4,
    recv_empty: u1,
    recv_full: u1,
    recv_not_empty_irq: u1,
    not_used1: u3,
    err: u1,
    enable: u1,
}

struct Fifo {
    cnt: u16,
    queue: VecDeque<u32>,
    last_received: u32,
}

impl Fifo {
    fn new() -> Self {
        Fifo {
            cnt: 0x0101,
            queue: VecDeque::new(),
            last_received: 0,
        }
    }
}

pub struct Ipc {
    sync_regs: [u16; 2],
    fifo: [Fifo; 2],
}

impl Ipc {
    pub fn new() -> Self {
        Ipc {
            sync_regs: [0u16; 2],
            fifo: [Fifo::new(), Fifo::new()],
        }
    }

    pub fn get_sync_reg<const CPU: CpuType>(&self) -> u16 {
        self.sync_regs[CPU]
    }

    pub fn get_fifo_cnt<const CPU: CpuType>(&self) -> u16 {
        self.fifo[CPU].cnt
    }

    pub fn set_sync_reg<const CPU: CpuType>(&mut self, mut mask: u16, value: u16, emu: &mut Emu) {
        debug_println!(
            "{:?} set ipc sync with mask {:x} and value {:x}",
            CPU,
            mask,
            value
        );

        mask &= 0x4F00;
        self.sync_regs[CPU] = (self.sync_regs[CPU] & !mask) | (value & mask);
        self.sync_regs[!CPU] =
            (self.sync_regs[!CPU] & !((mask >> 8) & 0xF)) | (((value & mask) >> 8) & 0xF);

        let value_sync = IpcSyncCnt::from(value);
        let other_cpu_ipc_sync = IpcSyncCnt::from(self.sync_regs[!CPU]);

        if bool::from(value_sync.send_irq()) && bool::from(other_cpu_ipc_sync.enable_irq()) {
            get_cpu_regs_mut!(emu, !CPU).send_interrupt(InterruptFlag::IpcSync, get_cm!(emu));
        }
    }

    pub fn set_fifo_cnt<const CPU: CpuType>(&mut self, mut mask: u16, value: u16, emu: &mut Emu) {
        let mut current_fifo = IpcFifoCnt::from(self.fifo[CPU].cnt);
        let new_fifo = IpcFifoCnt::from(value);

        if bool::from(new_fifo.send_clear()) && !self.fifo[CPU].queue.is_empty() {
            self.fifo[CPU].queue.clear();
            self.fifo[CPU].last_received = 0;

            current_fifo.set_send_empty_status(u1::new(1));
            current_fifo.set_send_full_status(u1::new(0));
            self.fifo[CPU].cnt = u16::from(current_fifo.clone());

            let mut other = IpcFifoCnt::from(self.fifo[!CPU].cnt);
            other.set_recv_empty(u1::new(1));
            other.set_recv_full(u1::new(0));
            self.fifo[!CPU].cnt = u16::from(other);

            if bool::from(current_fifo.send_empty_irq()) {
                todo!()
            }
        }

        if bool::from(current_fifo.send_empty_status())
            && !bool::from(current_fifo.send_empty_irq())
            && bool::from(new_fifo.send_empty_irq())
        {
            get_cpu_regs_mut!(emu, CPU)
                .send_interrupt(InterruptFlag::IpcSendFifoEmpty, get_cm!(emu));
        }

        if !bool::from(current_fifo.recv_empty())
            && !bool::from(current_fifo.recv_not_empty_irq())
            && bool::from(new_fifo.recv_not_empty_irq())
        {
            get_cpu_regs_mut!(emu, CPU)
                .send_interrupt(InterruptFlag::IpcRecvFifoNotEmpty, get_cm!(emu));
        }

        if bool::from(new_fifo.err()) {
            current_fifo.set_err(u1::new(0));
            self.fifo[CPU].cnt = u16::from(current_fifo);
        }

        mask &= 0x8404;
        self.fifo[CPU].cnt = (self.fifo[CPU].cnt & !mask) | (value & mask);
    }

    pub fn fifo_send<const CPU: CpuType>(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        let mut fifo_cnt = IpcFifoCnt::from(self.fifo[CPU].cnt);
        if bool::from(fifo_cnt.enable()) {
            let fifo_len = self.fifo[CPU].queue.len();
            if fifo_len < 16 {
                self.fifo[CPU].queue.push_back(value & mask);

                if fifo_len == 0 {
                    let mut other_fifo_cnt = IpcFifoCnt::from(self.fifo[!CPU].cnt);

                    fifo_cnt.set_send_empty_status(u1::new(0));
                    other_fifo_cnt.set_recv_empty(u1::new(0));

                    let irq = bool::from(other_fifo_cnt.recv_not_empty_irq());
                    self.fifo[CPU].cnt = u16::from(fifo_cnt);
                    self.fifo[!CPU].cnt = u16::from(other_fifo_cnt);

                    if irq {
                        get_cpu_regs_mut!(emu, !CPU)
                            .send_interrupt(InterruptFlag::IpcRecvFifoNotEmpty, get_cm!(emu));
                    }
                } else if fifo_len == 15 {
                    let mut other_fifo_cnt = IpcFifoCnt::from(self.fifo[!CPU].cnt);

                    fifo_cnt.set_send_full_status(u1::new(1));
                    other_fifo_cnt.set_recv_full(u1::new(1));

                    self.fifo[CPU].cnt = u16::from(fifo_cnt);
                    self.fifo[!CPU].cnt = u16::from(other_fifo_cnt);
                }
            } else {
                fifo_cnt.set_err(u1::new(1));
                self.fifo[CPU].cnt = u16::from(fifo_cnt);
            }
        }
    }

    pub fn fifo_recv<const CPU: CpuType>(&mut self, emur: &mut Emu) -> u32 {
        let mut fifo_cnt = IpcFifoCnt::from(self.fifo[CPU].cnt);
        let other_fifo_len = self.fifo[!CPU].queue.len();
        if other_fifo_len > 0 {
            self.fifo[CPU].last_received = *self.fifo[!CPU].queue.front().unwrap();

            if bool::from(fifo_cnt.enable()) {
                self.fifo[!CPU].queue.pop_front();

                if other_fifo_len == 1 {
                    let mut other_fifo_cnt = IpcFifoCnt::from(self.fifo[!CPU].cnt);

                    fifo_cnt.set_recv_empty(u1::new(1));
                    other_fifo_cnt.set_send_empty_status(u1::new(1));

                    let irq = bool::from(other_fifo_cnt.send_empty_irq());
                    self.fifo[CPU].cnt = u16::from(fifo_cnt);
                    self.fifo[!CPU].cnt = u16::from(other_fifo_cnt);

                    if irq {
                        get_cpu_regs_mut!(emur, !CPU)
                            .send_interrupt(InterruptFlag::IpcSendFifoEmpty, get_cm!(emur));
                    }
                } else if other_fifo_len == 16 {
                    let mut other_fifo_cnt = IpcFifoCnt::from(self.fifo[!CPU].cnt);

                    fifo_cnt.set_recv_full(u1::new(0));
                    other_fifo_cnt.set_send_full_status(u1::new(0));

                    self.fifo[CPU].cnt = u16::from(fifo_cnt);
                    self.fifo[!CPU].cnt = u16::from(other_fifo_cnt);
                }
            }
        } else {
            fifo_cnt.set_err(u1::new(1));
            self.fifo[CPU].cnt = u16::from(fifo_cnt);
        }

        self.fifo[CPU].last_received
    }
}
