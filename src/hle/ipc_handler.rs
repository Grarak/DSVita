use crate::hle::cpu_regs::{CpuRegs, InterruptFlag};
use crate::hle::CpuType;
use crate::logging::debug_println;
use bilge::prelude::*;
use std::collections::VecDeque;
use std::sync::Arc;

#[bitsize(16)]
#[derive(FromBits)]
struct IpcSyncCnt {
    data_in: u4,
    not_used: u4,
    data_out: u4, // R/W
    not_used1: u1,
    send_irq: u1,
    enable_irq: u1, // R/W
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

pub struct IpcHandler {
    sync_regs: [u16; 2],
    fifo: [Fifo; 2],
    cpu_regs_arm9: Arc<CpuRegs<{ CpuType::ARM9 }>>,
    cpu_regs_arm7: Arc<CpuRegs<{ CpuType::ARM7 }>>,
}

impl IpcHandler {
    pub fn new(
        cpu_regs_arm9: Arc<CpuRegs<{ CpuType::ARM9 }>>,
        cpu_regs_arm7: Arc<CpuRegs<{ CpuType::ARM7 }>>,
    ) -> Self {
        IpcHandler {
            sync_regs: [0u16; 2],
            fifo: [Fifo::new(), Fifo::new()],
            cpu_regs_arm9,
            cpu_regs_arm7,
        }
    }

    pub fn get_sync_reg<const CPU: CpuType>(&self) -> u16 {
        self.sync_regs[CPU as usize]
    }

    pub fn get_fifo_cnt<const CPU: CpuType>(&self) -> u16 {
        self.fifo[CPU as usize].cnt
    }

    pub fn set_sync_reg<const CPU: CpuType>(&mut self, mut mask: u16, value: u16) {
        debug_println!(
            "{:?} set ipc sync with mask {:x} and value {:x}",
            CPU,
            mask,
            value
        );

        mask &= 0x4F00;
        let current = &mut self.sync_regs[CPU as usize];
        *current = (*current & !mask) | (value & mask);
        let other = &mut self.sync_regs[!CPU as usize];
        *other = (*other & !((mask >> 8) & 0xF)) | (((value & mask) >> 8) & 0xF);

        let current_cpu_ipc_sync = IpcSyncCnt::from(self.sync_regs[CPU as usize]);
        let other_cpu_ipc_sync = IpcSyncCnt::from(self.sync_regs[!CPU as usize]);

        if bool::from(current_cpu_ipc_sync.send_irq())
            && bool::from(other_cpu_ipc_sync.enable_irq())
        {
            todo!()
        }
    }

    pub fn set_fifo_cnt<const CPU: CpuType>(&mut self, mut mask: u16, value: u16) {
        let mut current_fifo = IpcFifoCnt::from(self.fifo[CPU as usize].cnt);
        let new_fifo = IpcFifoCnt::from(value);

        if bool::from(new_fifo.send_clear()) && !self.fifo[CPU as usize].queue.is_empty() {
            self.fifo[CPU as usize].queue.clear();
            self.fifo[CPU as usize].last_received = 0;

            current_fifo.set_send_empty_status(u1::new(1));
            current_fifo.set_send_full_status(u1::new(0));
            self.fifo[CPU as usize].cnt = u16::from(current_fifo.clone());

            let mut other = IpcFifoCnt::from(self.fifo[!CPU as usize].cnt);
            other.set_recv_empty(u1::new(1));
            other.set_recv_full(u1::new(0));
            self.fifo[!CPU as usize].cnt = u16::from(other);

            if bool::from(current_fifo.send_empty_irq()) {
                todo!()
            }
        }

        if bool::from(current_fifo.send_empty_status())
            && !bool::from(current_fifo.send_empty_irq())
            && bool::from(new_fifo.send_empty_irq())
        {
            if CPU == CpuType::ARM9 {
                self.cpu_regs_arm9
                    .send_interrupt(InterruptFlag::IpcSendFifoEmpty);
            } else {
                self.cpu_regs_arm7
                    .send_interrupt(InterruptFlag::IpcSendFifoEmpty);
            }
        }

        if !bool::from(current_fifo.recv_empty())
            && !bool::from(current_fifo.recv_not_empty_irq())
            && bool::from(new_fifo.recv_not_empty_irq())
        {
            if CPU == CpuType::ARM9 {
                self.cpu_regs_arm9
                    .send_interrupt(InterruptFlag::IpcRecvFifoNotEmpty);
            } else {
                self.cpu_regs_arm7
                    .send_interrupt(InterruptFlag::IpcRecvFifoNotEmpty);
            }
        }

        if bool::from(new_fifo.err()) {
            current_fifo.set_err(u1::new(0));
            self.fifo[CPU as usize].cnt = u16::from(current_fifo);
        }

        mask &= 0x8404;
        self.fifo[CPU as usize].cnt = (self.fifo[CPU as usize].cnt & !mask) | (value & mask);
    }

    pub fn fifo_send<const CPU: CpuType>(&mut self, mask: u32, value: u32) {
        let mut fifo_cnt = IpcFifoCnt::from(self.fifo[CPU as usize].cnt);
        if bool::from(fifo_cnt.enable()) {
            let fifo_len = self.fifo[CPU as usize].queue.len();
            if fifo_len < 16 {
                self.fifo[CPU as usize].queue.push_back(value & mask);

                if fifo_len == 0 {
                    let mut other_fifo_cnt = IpcFifoCnt::from(self.fifo[!CPU as usize].cnt);

                    fifo_cnt.set_send_empty_status(u1::new(0));
                    other_fifo_cnt.set_recv_empty(u1::new(0));

                    let irq = bool::from(other_fifo_cnt.recv_not_empty_irq());
                    self.fifo[CPU as usize].cnt = u16::from(fifo_cnt);
                    self.fifo[!CPU as usize].cnt = u16::from(other_fifo_cnt);

                    if irq {
                        if CPU == CpuType::ARM9 {
                            self.cpu_regs_arm7
                                .send_interrupt(InterruptFlag::IpcRecvFifoNotEmpty);
                        } else {
                            self.cpu_regs_arm9
                                .send_interrupt(InterruptFlag::IpcRecvFifoNotEmpty);
                        }
                    }
                } else if fifo_len == 15 {
                    let mut other_fifo_cnt = IpcFifoCnt::from(self.fifo[!CPU as usize].cnt);

                    fifo_cnt.set_send_full_status(u1::new(1));
                    other_fifo_cnt.set_recv_full(u1::new(1));

                    self.fifo[CPU as usize].cnt = u16::from(fifo_cnt);
                    self.fifo[!CPU as usize].cnt = u16::from(other_fifo_cnt);
                }
            } else {
                fifo_cnt.set_err(u1::new(1));
                self.fifo[CPU as usize].cnt = u16::from(fifo_cnt);
            }
        }
    }

    pub fn fifo_recv<const CPU: CpuType>(&mut self) -> u32 {
        let mut fifo_cnt = IpcFifoCnt::from(self.fifo[CPU as usize].cnt);
        let other_fifo_len = self.fifo[!CPU as usize].queue.len();
        if other_fifo_len > 0 {
            self.fifo[CPU as usize].last_received =
                *self.fifo[!CPU as usize].queue.front().unwrap();

            if bool::from(fifo_cnt.enable()) {
                self.fifo[!CPU as usize].queue.pop_front();

                if other_fifo_len == 1 {
                    let mut other_fifo_cnt = IpcFifoCnt::from(self.fifo[!CPU as usize].cnt);

                    fifo_cnt.set_recv_empty(u1::new(1));
                    other_fifo_cnt.set_send_empty_status(u1::new(1));

                    let irq = bool::from(other_fifo_cnt.send_empty_irq());
                    self.fifo[CPU as usize].cnt = u16::from(fifo_cnt);
                    self.fifo[!CPU as usize].cnt = u16::from(other_fifo_cnt);

                    if irq {
                        if CPU == CpuType::ARM9 {
                            self.cpu_regs_arm7
                                .send_interrupt(InterruptFlag::IpcSendFifoEmpty);
                        } else {
                            self.cpu_regs_arm9
                                .send_interrupt(InterruptFlag::IpcSendFifoEmpty);
                        }
                    }
                } else if other_fifo_len == 16 {
                    let mut other_fifo_cnt = IpcFifoCnt::from(self.fifo[!CPU as usize].cnt);

                    fifo_cnt.set_recv_full(u1::new(0));
                    other_fifo_cnt.set_send_full_status(u1::new(0));

                    self.fifo[CPU as usize].cnt = u16::from(fifo_cnt);
                    self.fifo[!CPU as usize].cnt = u16::from(other_fifo_cnt);
                }
            }
        } else {
            fifo_cnt.set_err(u1::new(1));
            self.fifo[CPU as usize].cnt = u16::from(fifo_cnt);
        }

        self.fifo[CPU as usize].last_received
    }
}
