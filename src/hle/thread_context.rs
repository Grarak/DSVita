use crate::hle::cp15_context::Cp15Context;
use crate::hle::cpu_regs::CpuRegs;
use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::dma::Dma;
use crate::hle::memory::io_ports::IoPorts;
use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::memory::memory::Memory;
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
use std::thread;

pub struct ThreadContext {
    cpu_type: CpuType,
    jit: JitAsm,
    pub regs: Rc<RefCell<ThreadRegs>>,
    pub cp15_context: Rc<RefCell<Cp15Context>>,
    pub mem_handler: Arc<MemHandler>,
}

impl ThreadContext {
    pub fn new(
        cpu_type: CpuType,
        jit_memory: Arc<RwLock<JitMemory>>,
        memory: Arc<RwLock<Memory>>,
        spi_context: Arc<RwLock<SpiContext>>,
        ipc_handler: Arc<RwLock<IpcHandler>>,
    ) -> Self {
        let regs = ThreadRegs::new(cpu_type);
        let cp15_context = Rc::new(RefCell::new(Cp15Context::new()));
        let cpu_regs = Rc::new(RefCell::new(CpuRegs::new(cpu_type)));
        let gpu_context = Rc::new(RefCell::new(GpuContext::new()));
        let spu_context = Rc::new(RefCell::new(SpuContext::new()));
        let dma = Arc::new(RefCell::new(Dma::new(cpu_type)));
        let timers_context = Rc::new(RefCell::new(TimersContext::new()));

        let io_ports = IoPorts::new(
            cpu_type,
            memory.clone(),
            spi_context,
            ipc_handler,
            cpu_regs,
            gpu_context,
            spu_context,
            dma.clone(),
            timers_context,
        );

        let mem_handler =
            Arc::new(MemHandler::new(cpu_type, memory.clone(), cp15_context.clone(), io_ports));

        dma.borrow_mut().set_mem_handler(mem_handler.clone());

        ThreadContext {
            cpu_type,
            jit: JitAsm::new(
                cpu_type,
                jit_memory,
                regs.clone(),
                cp15_context.clone(),
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
