use crate::hle::bios_context::BiosContext;
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
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

pub struct ThreadContext<const CPU: CpuType> {
    pub jit: JitAsm<CPU>,
    pub cycle_manager: Rc<CycleManager>,
    pub regs: Rc<RefCell<ThreadRegs<CPU>>>,
    pub cp15_context: Rc<RefCell<Cp15Context>>,
    pub mem_handler: Rc<MemHandler<CPU>>,
    cpu_regs: Rc<CpuRegs<CPU>>,
    bios_context: Rc<RefCell<BiosContext<CPU>>>,
}

unsafe impl<const CPU: CpuType> Send for ThreadContext<CPU> {}

impl<const CPU: CpuType> ThreadContext<CPU> {
    pub fn new(
        cycle_manager: Rc<CycleManager>,
        jit_memory: Rc<RefCell<JitMemory>>,
        main_memory: *mut MainMemory,
        wram_context: Rc<RefCell<WramContext>>,
        spi_context: Rc<RefCell<SpiContext>>,
        ipc_handler: Rc<RefCell<IpcHandler>>,
        vram_context: Rc<RefCell<VramContext>>,
        input_context: Arc<RwLock<InputContext>>,
        gpu_context: Rc<GpuContext>,
        gpu_2d_context_a: Rc<RefCell<Gpu2DContext<{ A }>>>,
        gpu_2d_context_b: Rc<RefCell<Gpu2DContext<{ B }>>>,
        dma: Rc<RefCell<Dma<CPU>>>,
        rtc_context: Rc<RefCell<RtcContext>>,
        spu_context: Rc<RefCell<SpuContext>>,
        palettes_context: Rc<RefCell<PalettesContext>>,
        cp15_context: Rc<RefCell<Cp15Context>>,
        tcm_context: Rc<RefCell<TcmContext>>,
        oam: Rc<RefCell<OamContext>>,
        cpu_regs: Rc<CpuRegs<CPU>>,
    ) -> Self {
        let regs = ThreadRegs::new(cpu_regs.clone());
        let timers_context = Rc::new(RefCell::new(TimersContext::new(cycle_manager.clone())));

        let io_ports = IoPorts::new(
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

        let mem_handler = Rc::new(MemHandler::new(
            main_memory,
            wram_context,
            palettes_context,
            cp15_context.clone(),
            tcm_context,
            io_ports,
            oam,
        ));

        dma.borrow_mut().set_mem_handler(mem_handler.clone());

        let bios_context = Rc::new(RefCell::new(BiosContext::new(
            regs.clone(),
            cpu_regs.clone(),
            mem_handler.clone(),
        )));

        cpu_regs.set_bios_context(bios_context.clone());
        cpu_regs.set_cp15_context(cp15_context.clone());

        ThreadContext {
            jit: JitAsm::new(
                jit_memory,
                regs.clone(),
                cpu_regs.clone(),
                cp15_context.clone(),
                bios_context.clone(),
                mem_handler.clone(),
            ),
            cycle_manager,
            regs,
            cp15_context,
            mem_handler,
            cpu_regs,
            bios_context,
        }
    }

    pub fn is_halted(&self) -> bool {
        self.cpu_regs.is_halted()
    }

    pub fn run(&mut self) -> u16 {
        let pc = self.regs.borrow().pc;
        let cycles =
            if (CPU == CpuType::ARM9 && pc == 0xFFFF0000) || (CPU == CpuType::ARM7 && pc == 0) {
                self.bios_context.borrow_mut().uninterrupt();
                3
            } else {
                self.jit.execute()
            };
        if CPU == CpuType::ARM9 {
            (cycles + 1) >> 1
        } else {
            cycles
        }
    }
}
