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

    pub fn set_sync_reg(&mut self, cpu_type: CpuType, mut mask: u16, value: u16) {
        debug_println!(
            "{:?} set ipc sync with mask {:x} and value {:x}",
            cpu_type,
            mask,
            value
        );

        mask &= 0x4F00;
        let current = &mut self.sync_regs[cpu_type as usize];
        *current = (*current & !mask) | (value & mask);
        let other = &mut self.sync_regs[!cpu_type as usize];
        *other = (*other & !((mask >> 8) & 0xF)) | (((value & mask) >> 8) & 0xF);

        let current_cpu_ipc_sync = IpcSync::from(self.sync_regs[cpu_type as usize]);
        let other_cpu_ipc_sync = IpcSync::from(self.sync_regs[!cpu_type as usize]);

        if bool::from(current_cpu_ipc_sync.send_irq())
            && bool::from(other_cpu_ipc_sync.enable_irq())
        {
            todo!()
        }
    }
}
