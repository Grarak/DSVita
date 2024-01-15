use crate::hle::CpuType;
use crate::logging::debug_println;
use bilge::prelude::*;

#[bitsize(16)]
#[derive(FromBits)]
struct IpcSync {
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
struct IpcFifo {
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
    queue: Vec<u32>,
    last_received: u32,
}

impl Fifo {
    fn new() -> Self {
        Fifo {
            cnt: 0x0101,
            queue: Vec::new(),
            last_received: 0,
        }
    }
}

pub struct IpcHandler {
    sync_regs: [u16; 2],
    fifo: [Fifo; 2],
}

impl IpcHandler {
    pub fn new() -> Self {
        IpcHandler {
            sync_regs: [0u16; 2],
            fifo: [Fifo::new(), Fifo::new()],
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

        let current_cpu_ipc_sync = IpcSync::from(self.sync_regs[CPU as usize]);
        let other_cpu_ipc_sync = IpcSync::from(self.sync_regs[!CPU as usize]);

        if bool::from(current_cpu_ipc_sync.send_irq())
            && bool::from(other_cpu_ipc_sync.enable_irq())
        {
            todo!()
        }
    }

    pub fn set_fifo_cnt<const CPU: CpuType>(&mut self, mut mask: u16, value: u16) {
        let mut current_fifo = IpcFifo::from(self.fifo[CPU as usize].cnt);
        let new_fifo = IpcFifo::from(value);

        if bool::from(new_fifo.send_clear()) && !self.fifo[CPU as usize].queue.is_empty() {
            self.fifo[CPU as usize].queue.clear();
            self.fifo[CPU as usize].last_received = 0;

            current_fifo.set_send_empty_status(u1::new(1));
            current_fifo.set_send_full_status(u1::new(0));
            self.fifo[CPU as usize].cnt = u16::from(current_fifo.clone());

            let mut other = IpcFifo::from(self.fifo[!CPU as usize].cnt);
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
        {}

        if !bool::from(current_fifo.recv_empty())
            && !bool::from(current_fifo.recv_not_empty_irq())
            && bool::from(new_fifo.recv_not_empty_irq())
        {
            todo!()
        }

        if bool::from(new_fifo.err()) {
            current_fifo.set_err(u1::new(0));
            self.fifo[CPU as usize].cnt = u16::from(current_fifo);
        }

        mask &= 0x8404;
        self.fifo[CPU as usize].cnt = (self.fifo[CPU as usize].cnt & !mask) | (value & mask);
    }
}
