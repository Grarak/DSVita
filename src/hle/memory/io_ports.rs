use crate::hle::cpu_regs::CpuRegs;
use crate::hle::gpu::gpu_2d_context::Gpu2DContext;
use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::dma::Dma;
use crate::hle::memory::main_memory::MainMemory;
use crate::hle::memory::vram_context::VramContext;
use crate::hle::memory::wram_context::WramContext;
use crate::hle::spi_context::SpiContext;
use crate::hle::spu_context::SpuContext;
use crate::hle::timers_context::TimersContext;
use crate::hle::CpuType;
use crate::utils::{Convert, FastCell};
use std::rc::Rc;
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, RwLock};

pub struct IoPorts {
    cpu_type: CpuType,
    pub memory: Arc<RwLock<MainMemory>>,
    pub wram_context: Arc<WramContext>,
    pub ipc_handler: Arc<RwLock<IpcHandler>>,
    pub cpu_regs: Rc<FastCell<CpuRegs>>,
    pub dma: Rc<FastCell<Dma>>,
    pub timers_context: Rc<FastCell<TimersContext>>,

    pub vram_context: Arc<VramContext>,
    pub gpu_context: Rc<FastCell<GpuContext>>,
    pub gpu_2d_context_0: Rc<FastCell<Gpu2DContext>>,
    pub gpu_2d_context_1: Rc<FastCell<Gpu2DContext>>,

    pub vram_stat: Arc<AtomicU8>,
    pub spi_context: Arc<RwLock<SpiContext>>,
    pub spu_context: Rc<FastCell<SpuContext>>,
}

impl IoPorts {
    pub fn new(
        cpu_type: CpuType,
        memory: Arc<RwLock<MainMemory>>,
        wram_context: Arc<WramContext>,
        ipc_handler: Arc<RwLock<IpcHandler>>,
        cpu_regs: Rc<FastCell<CpuRegs>>,
        dma: Rc<FastCell<Dma>>,
        timers_context: Rc<FastCell<TimersContext>>,
        vram_context: Arc<VramContext>,
        gpu_context: Rc<FastCell<GpuContext>>,
        gpu_2d_context_0: Rc<FastCell<Gpu2DContext>>,
        gpu_2d_context_1: Rc<FastCell<Gpu2DContext>>,
        vram_stat: Arc<AtomicU8>,
        spi_context: Arc<RwLock<SpiContext>>,
        spu_context: Rc<FastCell<SpuContext>>,
    ) -> Self {
        IoPorts {
            cpu_type,
            memory,
            wram_context,
            ipc_handler,
            cpu_regs,
            dma,
            timers_context,
            vram_context,
            gpu_context,
            gpu_2d_context_0,
            gpu_2d_context_1,
            vram_stat,
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
