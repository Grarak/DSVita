use crate::hle::cp15_context::Cp15Context;
use crate::hle::cpu_regs::CpuRegs;
use crate::hle::gpu::gpu_2d_context::Gpu2DContext;
use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::dma::Dma;
use crate::hle::memory::io_ports::IoPorts;
use crate::hle::memory::main_memory::MainMemory;
use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::memory::vram_context::VramContext;
use crate::hle::memory::wram_context::WramContext;
use crate::hle::spi_context::SpiContext;
use crate::hle::spu_context::SpuContext;
use crate::hle::thread_regs::ThreadRegs;
use crate::hle::timers_context::TimersContext;
use crate::hle::CpuType;
use crate::jit::jit_asm::JitAsm;
use crate::jit::jit_cycle_handler::JitCycleManager;
use crate::jit::jit_memory::JitMemory;
use crate::utils::FastCell;
use std::rc::Rc;
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, RwLock};
use std::thread;

pub struct ThreadContext {
    cpu_type: CpuType,
    jit: JitAsm,
    pub regs: Rc<FastCell<ThreadRegs>>,
    pub cp15_context: Rc<FastCell<Cp15Context>>,
    pub mem_handler: Arc<MemHandler>,
}

impl ThreadContext {
    pub fn new(
        cpu_type: CpuType,
        jit_cycle_manager: Arc<RwLock<JitCycleManager>>,
        jit_memory: Arc<RwLock<JitMemory>>,
        memory: Arc<RwLock<MainMemory>>,
        wram_context: Arc<WramContext>,
        spi_context: Arc<RwLock<SpiContext>>,
        ipc_handler: Arc<RwLock<IpcHandler>>,
    ) -> Self {
        let regs = ThreadRegs::new(cpu_type);
        let cp15_context = Rc::new(FastCell::new(Cp15Context::new()));
        let cpu_regs = Rc::new(FastCell::new(CpuRegs::new(cpu_type)));
        let dma = Rc::new(FastCell::new(Dma::new(cpu_type)));
        let timers_context = Rc::new(FastCell::new(TimersContext::new()));

        let vram_stat = Arc::new(AtomicU8::new(0));
        let vram_context = Arc::new(VramContext::new(vram_stat.clone()));

        let gpu_context = Rc::new(FastCell::new(GpuContext::new()));
        let gpu_2d_context_0 = Rc::new(FastCell::new(Gpu2DContext::new()));
        let gpu_2d_context_1 = Rc::new(FastCell::new(Gpu2DContext::new()));

        let spu_context = Rc::new(FastCell::new(SpuContext::new()));

        let io_ports = IoPorts::new(
            cpu_type,
            memory.clone(),
            wram_context.clone(),
            ipc_handler,
            cpu_regs,
            dma.clone(),
            timers_context.clone(),
            vram_context,
            gpu_context,
            gpu_2d_context_0,
            gpu_2d_context_1,
            vram_stat,
            spi_context,
            spu_context,
        );

        let mem_handler =
            Arc::new(MemHandler::new(
                cpu_type,
                memory.clone(),
                wram_context,
                cp15_context.clone(),
                io_ports,
            ));

        dma.borrow_mut().set_mem_handler(mem_handler.clone());

        ThreadContext {
            cpu_type,
            jit: JitAsm::new(
                cpu_type,
                jit_cycle_manager,
                jit_memory,
                regs.clone(),
                cp15_context.clone(),
                timers_context,
                mem_handler.clone(),
            ),
            regs,
            cp15_context,
            mem_handler,
        }
    }

    pub fn run(&mut self) {
        println!(
            "{:?} start with host thread id {:x}",
            self.cpu_type,
            thread::current().id().as_u64()
        );
        loop {
            self.jit.execute();
        }
    }

    pub fn iterate(&mut self, count: usize) {
        for _ in 0..count {
            self.jit.execute();
        }
    }
}
