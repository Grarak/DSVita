use crate::hle::cpu_regs::CpuRegs;
use crate::hle::gpu::gpu_2d_context::Gpu2DContext;
use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::dma::Dma;
use crate::hle::memory::memory::Memory;
use crate::hle::spi_context::SpiContext;
use crate::hle::spu_context::SpuContext;
use crate::hle::timers_context::TimersContext;
use crate::hle::CpuType;
use crate::utils::Convert;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

pub struct IoPorts {
    cpu_type: CpuType,
    pub memory: Arc<RwLock<Memory>>,
    pub ipc_handler: Arc<RwLock<IpcHandler>>,
    pub cpu_regs: Rc<RefCell<CpuRegs>>,
    pub dma: Rc<RefCell<Dma>>,
    pub timers_context: Rc<RefCell<TimersContext>>,

    pub gpu_context: Rc<RefCell<GpuContext>>,
    pub gpu_2d_context_0: Rc<RefCell<Gpu2DContext>>,
    pub gpu_2d_context_1: Rc<RefCell<Gpu2DContext>>,

    pub spi_context: Arc<RwLock<SpiContext>>,
    pub spu_context: Rc<RefCell<SpuContext>>,
}

impl IoPorts {
    pub fn new(
        cpu_type: CpuType,
        memory: Arc<RwLock<Memory>>,
        ipc_handler: Arc<RwLock<IpcHandler>>,
        cpu_regs: Rc<RefCell<CpuRegs>>,
        dma: Rc<RefCell<Dma>>,
        timers_context: Rc<RefCell<TimersContext>>,
        gpu_context: Rc<RefCell<GpuContext>>,
        gpu_2d_context_0: Rc<RefCell<Gpu2DContext>>,
        gpu_2d_context_1: Rc<RefCell<Gpu2DContext>>,
        spi_context: Arc<RwLock<SpiContext>>,
        spu_context: Rc<RefCell<SpuContext>>,
    ) -> Self {
        IoPorts {
            cpu_type,
            memory,
            ipc_handler,
            cpu_regs,
            dma,
            timers_context,
            gpu_context,
            gpu_2d_context_0,
            gpu_2d_context_1,
            spi_context,
            spu_context,
        }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        match self.cpu_type {
            CpuType::ARM9 => self.read_arm9(addr_offset),
            CpuType::ARM7 => self.read_arm7(addr_offset),
        }
    }

    pub fn write<T: Convert>(&self, addr_offset: u32, value: T) {
        match self.cpu_type {
            CpuType::ARM9 => self.write_arm9(addr_offset, value),
            CpuType::ARM7 => self.write_arm7(addr_offset, value),
        }
    }
}
