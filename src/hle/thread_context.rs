use crate::hle::cp15_context::Cp15Context;
use crate::hle::cpu_regs::CpuRegs;
use crate::hle::cycle_manager::CycleManager;
use crate::hle::gpu::gpu_2d_context::Gpu2DContext;
use crate::hle::gpu::gpu_2d_context::Gpu2DEngine::{A, B};
use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::input_context::InputContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::dma::Dma;
use crate::hle::memory::io_ports::IoPorts;
use crate::hle::memory::main_memory::MainMemory;
use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::memory::oam_context::OamContext;
use crate::hle::memory::palettes_context::PalettesContext;
use crate::hle::memory::tcm_context::TcmContext;
use crate::hle::memory::vram_context::VramContext;
use crate::hle::memory::wram_context::WramContext;
use crate::hle::rtc_context::RtcContext;
use crate::hle::spi_context::SpiContext;
use crate::hle::spu_context::SpuContext;
use crate::hle::thread_regs::ThreadRegs;
use crate::hle::timers_context::TimersContext;
use crate::hle::CpuType;
use crate::jit::jit_asm::JitAsm;
use crate::jit::jit_memory::JitMemory;
use crate::utils::FastCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

pub struct ThreadContext<const CPU: CpuType> {
    jit: JitAsm<CPU>,
    cycle_manager: Arc<CycleManager>,
    pub regs: Rc<FastCell<ThreadRegs<CPU>>>,
    pub cp15_context: Rc<FastCell<Cp15Context>>,
    pub mem_handler: Arc<MemHandler<CPU>>,
    cpu_regs: Arc<CpuRegs<CPU>>,
}

unsafe impl<const CPU: CpuType> Send for ThreadContext<CPU> {}

impl<const CPU: CpuType> ThreadContext<CPU> {
    pub fn new(
        cycle_manager: Arc<CycleManager>,
        jit_memory: Arc<Mutex<JitMemory>>,
        memory: Arc<RwLock<MainMemory>>,
        wram_context: Arc<WramContext>,
        spi_context: Arc<RwLock<SpiContext>>,
        ipc_handler: Arc<RwLock<IpcHandler>>,
        vram_context: Arc<VramContext>,
        input_context: Arc<RwLock<InputContext>>,
        gpu_context: Arc<RwLock<GpuContext>>,
        gpu_2d_context_a: Rc<FastCell<Gpu2DContext<{ A }>>>,
        gpu_2d_context_b: Rc<FastCell<Gpu2DContext<{ B }>>>,
        dma: Arc<RwLock<Dma<CPU>>>,
        rtc_context: Rc<FastCell<RtcContext>>,
        spu_context: Rc<FastCell<SpuContext>>,
        palettes_context: Rc<FastCell<PalettesContext>>,
        cp15_context: Rc<FastCell<Cp15Context>>,
        tcm_context: Rc<FastCell<TcmContext>>,
        oam: Rc<FastCell<OamContext>>,
        cpu_regs: Arc<CpuRegs<CPU>>,
    ) -> Self {
        let regs = ThreadRegs::new(cpu_regs.clone());
        let timers_context = Arc::new(RwLock::new(TimersContext::new(cycle_manager.clone())));

        let io_ports = IoPorts::new(
            memory.clone(),
            wram_context.clone(),
            ipc_handler,
            cpu_regs.clone(),
            dma.clone(),
            timers_context.clone(),
            vram_context,
            input_context,
            gpu_context,
            gpu_2d_context_a,
            gpu_2d_context_b,
            rtc_context,
            spi_context,
            spu_context,
        );

        let mem_handler = Arc::new(MemHandler::new(
            memory.clone(),
            wram_context,
            palettes_context,
            cp15_context.clone(),
            tcm_context,
            io_ports,
            oam,
        ));

        dma.write().unwrap().set_mem_handler(mem_handler.clone());

        ThreadContext {
            jit: JitAsm::new(
                jit_memory,
                regs.clone(),
                cpu_regs.clone(),
                cp15_context.clone(),
                timers_context,
                mem_handler.clone(),
            ),
            cycle_manager,
            regs,
            cp15_context,
            mem_handler,
            cpu_regs,
        }
    }

    fn is_halted(&self) -> bool {
        self.cpu_regs.is_halted()
    }

    pub fn run(&mut self) {
        loop {
            if self.is_halted() {
                self.cycle_manager.add_cycle::<CPU, true>(0);
                thread::yield_now();
            } else {
                let cycles = self.jit.execute();
                if CPU == CpuType::ARM9 {
                    self.cycle_manager
                        .add_cycle::<CPU, false>((cycles + (cycles % 2)) / 2);
                } else {
                    self.cycle_manager.add_cycle::<CPU, false>(cycles);
                }
            }
        }
    }
}
