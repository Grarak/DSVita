use crate::core::cpu_regs::InterruptFlag;
use crate::core::emu::{get_arm7_hle_mut, get_cpu_regs_mut, get_ipc, get_ipc_mut, Emu};
use crate::core::hle::arm7_hle::Arm7Hle;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::fixed_fifo::FixedFifo;
use crate::logging::debug_println;
use bilge::prelude::*;
use std::rc::Rc;

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
    inner: Rc<dyn IpcInner>,
}

trait IpcInner {
    fn get_fifo_cnt(&self, cpu: CpuType, ipc: &Ipc) -> u16;
    fn set_sync_reg(&self, cpu: CpuType, mask: u16, value: u16, emu: &mut Emu);
    fn fifo_send(&self, cpu: CpuType, mask: u32, value: u32, emu: &mut Emu);
}

struct IpcLle {}

impl IpcLle {
    fn new() -> Self {
        IpcLle {}
    }
}

impl IpcInner for IpcLle {
    fn get_fifo_cnt(&self, cpu: CpuType, ipc: &Ipc) -> u16 {
        ipc.fifo[cpu].cnt.into()
    }

    fn set_sync_reg(&self, cpu: CpuType, mut mask: u16, value: u16, emu: &mut Emu) {
        mask &= 0x4F00;
        get_ipc_mut!(emu).sync_regs[cpu] = ((u16::from(get_ipc!(emu).sync_regs[cpu]) & !mask) | (value & mask)).into();
        get_ipc_mut!(emu).sync_regs[!cpu] = ((u16::from(get_ipc!(emu).sync_regs[!cpu]) & !((mask >> 8) & 0xF)) | (((value & mask) >> 8) & 0xF)).into();

        if IpcSyncCnt::from(value).send_irq() && get_ipc!(emu).sync_regs[!cpu].enable_irq() {
            get_cpu_regs_mut!(emu, !cpu).send_interrupt(InterruptFlag::IpcSync, emu);
        }
    }

    fn fifo_send(&self, cpu: CpuType, mask: u32, value: u32, emu: &mut Emu) {
        if get_ipc!(emu).fifo[cpu].cnt.enable() {
            let fifo_len = get_ipc!(emu).fifo[cpu].queue.len();
            if fifo_len < 16 {
                get_ipc_mut!(emu).fifo[cpu].queue.push_back(value & mask);

                if fifo_len == 0 {
                    get_ipc_mut!(emu).fifo[cpu].cnt.set_send_empty_status(false);
                    get_ipc_mut!(emu).fifo[!cpu].cnt.set_recv_empty(false);

                    if get_ipc!(emu).fifo[!cpu].cnt.recv_not_empty_irq() {
                        get_cpu_regs_mut!(emu, !cpu).send_interrupt(InterruptFlag::IpcRecvFifoNotEmpty, emu);
                    }
                } else if fifo_len == 15 {
                    get_ipc_mut!(emu).fifo[cpu].cnt.set_send_full_status(true);
                    get_ipc_mut!(emu).fifo[!cpu].cnt.set_recv_full(true);
                }
            } else {
                get_ipc_mut!(emu).fifo[cpu].cnt.set_err(true);
            }
        }
    }
}

struct IpcHle {}

impl IpcHle {
    fn new() -> Self {
        IpcHle {}
    }
}

impl IpcInner for IpcHle {
    fn get_fifo_cnt(&self, _: CpuType, ipc: &Ipc) -> u16 {
        let mut cnt = ipc.fifo[ARM9].cnt;

        cnt.set_send_empty_status(false);
        cnt.set_send_full_status(false);
        cnt.set_recv_empty(false);
        cnt.set_recv_full(false);

        if ipc.fifo[ARM9].queue.is_empty() {
            cnt.set_send_empty_status(true);
        } else if ipc.fifo[ARM9].queue.len() == 16 {
            cnt.set_send_full_status(true);
        }

        cnt.set_send_empty_status(true);
        if ipc.fifo[ARM7].queue.is_empty() {
            cnt.set_recv_empty(true);
        } else if ipc.fifo[ARM7].queue.len() == 16 {
            cnt.set_recv_full(true);
        }

        cnt.into()
    }

    fn set_sync_reg(&self, _: CpuType, mut mask: u16, value: u16, emu: &mut Emu) {
        mask &= 0x4F00;
        get_ipc_mut!(emu).sync_regs[ARM9] = ((u16::from(get_ipc!(emu).sync_regs[ARM9]) & !mask) | (value & mask)).into();
        get_ipc_mut!(emu).sync_regs[ARM7] = ((u16::from(get_ipc!(emu).sync_regs[ARM7]) & !((mask >> 8) & 0xF)) | (((value & mask) >> 8) & 0xF)).into();

        Arm7Hle::ipc_sync(emu);
    }

    fn fifo_send(&self, _: CpuType, mask: u32, value: u32, emu: &mut Emu) {
        if get_ipc!(emu).fifo[ARM9].cnt.enable() {
            let fifo_len = get_ipc!(emu).fifo[ARM9].queue.len();
            if fifo_len < 16 {
                get_ipc_mut!(emu).fifo[ARM9].queue.push_back(value & mask);

                if fifo_len == 0 {
                    get_ipc_mut!(emu).fifo[ARM9].cnt.set_send_empty_status(false);
                } else if fifo_len == 15 {
                    get_ipc_mut!(emu).fifo[ARM9].cnt.set_send_full_status(true);
                }
                get_arm7_hle_mut!(emu).ipc_recv(emu);
            } else {
                get_ipc_mut!(emu).fifo[ARM9].cnt.set_err(true);
            }
        }
    }
}

impl Ipc {
    pub fn new() -> Self {
        Ipc {
            sync_regs: [IpcSyncCnt::from(0); 2],
            fifo: [Fifo::new(), Fifo::new()],
            inner: Rc::new(IpcLle::new()),
        }
    }

    pub fn use_hle(&mut self) {
        self.inner = Rc::new(IpcHle::new());
    }

    pub fn get_sync_reg<const CPU: CpuType>(&self) -> u16 {
        self.sync_regs[CPU].into()
    }

    pub fn get_fifo_cnt<const CPU: CpuType>(&self) -> u16 {
        self.inner.get_fifo_cnt(CPU, self)
    }

    pub fn set_sync_reg<const CPU: CpuType>(&mut self, mask: u16, value: u16, emu: &mut Emu) {
        debug_println!("{:?} set ipc sync with mask {:x} and value {:x}", CPU, mask, value);
        self.inner.clone().set_sync_reg(CPU, mask, value, emu);
    }

    pub fn set_fifo_cnt<const CPU: CpuType>(&mut self, mut mask: u16, value: u16, emu: &mut Emu) {
        let new_fifo = IpcFifoCnt::from(value);

        if bool::from(new_fifo.send_clear()) && !self.fifo[CPU].queue.is_empty() {
            self.fifo[CPU].queue.clear();
            self.fifo[CPU].last_received = 0;

            self.fifo[CPU].cnt.set_send_empty_status(true);
            self.fifo[CPU].cnt.set_send_full_status(false);

            self.fifo[!CPU].cnt.set_recv_empty(true);
            self.fifo[!CPU].cnt.set_recv_full(false);

            if self.fifo[CPU].cnt.send_empty_irq() {
                todo!()
            }
        }

        if bool::from(self.fifo[CPU].cnt.send_empty_status()) && !self.fifo[CPU].cnt.send_empty_irq() && new_fifo.send_empty_irq() {
            get_cpu_regs_mut!(emu, CPU).send_interrupt(InterruptFlag::IpcSendFifoEmpty, emu);
        }

        if !bool::from(self.fifo[CPU].cnt.recv_empty()) && !self.fifo[CPU].cnt.recv_not_empty_irq() && new_fifo.recv_not_empty_irq() {
            get_cpu_regs_mut!(emu, CPU).send_interrupt(InterruptFlag::IpcRecvFifoNotEmpty, emu);
        }

        if bool::from(new_fifo.err()) {
            self.fifo[CPU].cnt.set_err(false);
        }

        mask &= 0x8404;
        self.fifo[CPU].cnt = ((u16::from(self.fifo[CPU].cnt) & !mask) | (value & mask)).into();
    }

    pub fn fifo_send<const CPU: CpuType>(&mut self, mask: u32, value: u32, emu: &mut Emu) {
        self.inner.clone().fifo_send(CPU, mask, value, emu);
    }

    pub fn fifo_recv<const CPU: CpuType>(&mut self, emu: &mut Emu) -> u32 {
        let other_fifo_len = self.fifo[!CPU].queue.len();
        if other_fifo_len > 0 {
            self.fifo[CPU].last_received = *self.fifo[!CPU].queue.front();

            if self.fifo[CPU].cnt.enable() {
                self.fifo[!CPU].queue.pop_front();

                if other_fifo_len == 1 {
                    self.fifo[CPU].cnt.set_recv_empty(true);
                    self.fifo[!CPU].cnt.set_send_empty_status(true);

                    if self.fifo[!CPU].cnt.send_empty_irq() {
                        get_cpu_regs_mut!(emu, !CPU).send_interrupt(InterruptFlag::IpcSendFifoEmpty, emu);
                    }
                } else if other_fifo_len == 16 {
                    self.fifo[CPU].cnt.set_recv_full(false);
                    self.fifo[!CPU].cnt.set_send_full_status(false);
                }
            }
        } else {
            self.fifo[CPU].cnt.set_err(true);
        }

        self.fifo[CPU].last_received
    }
}
