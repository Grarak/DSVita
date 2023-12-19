use crate::hle::CpuType;
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

pub struct IpcHandler {
    sync_regs: [u16; 2],
}

impl IpcHandler {
    pub fn new() -> Self {
        IpcHandler {
            sync_regs: [0u16; 2],
        }
    }

    pub fn get_sync_reg(&self, cpu_type: CpuType) -> u16 {
        self.sync_regs[cpu_type as usize]
    }

    pub fn set_sync_reg(&mut self, cpu_type: CpuType, value: u16) {
        let mut current_cpu_ipc_sync = IpcSync::from(self.sync_regs[cpu_type as usize]);
        let mut other_cpu_ipc_sync = IpcSync::from(self.sync_regs[!cpu_type as usize]);

        let new_ipc_sync = IpcSync::from(value);
        current_cpu_ipc_sync.set_data_out(new_ipc_sync.data_out());
        current_cpu_ipc_sync.set_enable_irq(new_ipc_sync.enable_irq());
        other_cpu_ipc_sync.set_data_in(new_ipc_sync.data_out());

        let send_interrupt =
            bool::from(new_ipc_sync.send_irq()) && bool::from(other_cpu_ipc_sync.enable_irq());

        self.sync_regs[cpu_type as usize] = u16::from(current_cpu_ipc_sync);
        self.sync_regs[!cpu_type as usize] = u16::from(other_cpu_ipc_sync);

        if send_interrupt {
            todo!()
        }
    }
}
