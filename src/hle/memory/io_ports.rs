use crate::hle::cpu_regs::CpuRegs;
use crate::hle::gpu::gpu_2d_context::Gpu2DContext;
use crate::hle::gpu::gpu_2d_context::Gpu2DEngine::{A, B};
use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::input_context::InputContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::dma::Dma;
use crate::hle::memory::main_memory::MainMemory;
use crate::hle::memory::vram_context::VramContext;
use crate::hle::memory::wram_context::WramContext;
use crate::hle::rtc_context::RtcContext;
use crate::hle::spi_context::SpiContext;
use crate::hle::spu_context::SpuContext;
use crate::hle::timers_context::TimersContext;
use crate::hle::CpuType;
use crate::utils::{Convert, FastCell};
use std::rc::Rc;
use std::sync::{Arc, RwLock};

pub struct IoPorts<const CPU: CpuType> {
    pub memory: Arc<RwLock<MainMemory>>,
    pub wram_context: Arc<WramContext>,
    pub ipc_handler: Arc<RwLock<IpcHandler>>,
    pub cpu_regs: Arc<CpuRegs<CPU>>,
    pub dma: Rc<FastCell<Dma<CPU>>>,
    pub timers_context: Arc<RwLock<TimersContext<CPU>>>,
    pub vram_context: Arc<VramContext>,
    pub input_context: Arc<RwLock<InputContext>>,

    pub gpu_context: Arc<RwLock<GpuContext>>,
    pub gpu_2d_context_a: Rc<FastCell<Gpu2DContext<{ A }>>>,
    pub gpu_2d_context_b: Rc<FastCell<Gpu2DContext<{ B }>>>,

    pub rtc_context: Rc<FastCell<RtcContext>>,
    pub spi_context: Arc<RwLock<SpiContext>>,
    pub spu_context: Rc<FastCell<SpuContext>>,
}

impl<const CPU: CpuType> IoPorts<CPU> {
    pub fn new(
        memory: Arc<RwLock<MainMemory>>,
        wram_context: Arc<WramContext>,
        ipc_handler: Arc<RwLock<IpcHandler>>,
        cpu_regs: Arc<CpuRegs<CPU>>,
        dma: Rc<FastCell<Dma<CPU>>>,
        timers_context: Arc<RwLock<TimersContext<CPU>>>,
        vram_context: Arc<VramContext>,
        input_context: Arc<RwLock<InputContext>>,
        gpu_context: Arc<RwLock<GpuContext>>,
        gpu_2d_context_a: Rc<FastCell<Gpu2DContext<{ A }>>>,
        gpu_2d_context_b: Rc<FastCell<Gpu2DContext<{ B }>>>,
        rtc_context: Rc<FastCell<RtcContext>>,
        spi_context: Arc<RwLock<SpiContext>>,
        spu_context: Rc<FastCell<SpuContext>>,
    ) -> Self {
        IoPorts {
            memory,
            wram_context,
            ipc_handler,
            cpu_regs,
            dma,
            timers_context,
            vram_context,
            input_context,
            gpu_context,
            gpu_2d_context_a,
            gpu_2d_context_b,
            rtc_context,
            spi_context,
            spu_context,
        }
    }

    pub fn read<T: Convert>(&self, addr_offset: u32) -> T {
        match CPU {
            CpuType::ARM9 => self.read_arm9(addr_offset),
            CpuType::ARM7 => self.read_arm7(addr_offset),
        }
    }

    pub fn write<T: Convert>(&self, addr_offset: u32, value: T) {
        match CPU {
            CpuType::ARM9 => self.write_arm9(addr_offset, value),
            CpuType::ARM7 => self.write_arm7(addr_offset, value),
        }
    }
}
