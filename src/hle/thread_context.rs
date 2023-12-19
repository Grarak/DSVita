use crate::hle::cp15_context::Cp15Context;
use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::ipc_handler::IpcHandler;
use crate::hle::memory::handler::mem_handler::MemHandler;
use crate::hle::memory::memory::Memory;
use crate::hle::registers::ThreadRegs;
use crate::hle::spu_context::SpuContext;
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
    pub mem_handler: Rc<RefCell<MemHandler>>,
}

impl ThreadContext {
    pub fn new(
        cpu_type: CpuType,
        jit_memory: Arc<RwLock<JitMemory>>,
        memory: Arc<RwLock<Memory>>,
        ipc_handler: Arc<RwLock<IpcHandler>>,
    ) -> Self {
        let regs = ThreadRegs::new(cpu_type);
        let cp15_context = Rc::new(RefCell::new(Cp15Context::new()));
        let gpu_context = Rc::new(RefCell::new(GpuContext::new()));
        let spu_context = Rc::new(RefCell::new(SpuContext::new()));
        let mem_handler = Rc::new(RefCell::new(MemHandler::new(
            cpu_type,
            memory,
            ipc_handler,
            regs.clone(),
            gpu_context,
            spu_context,
        )));

        ThreadContext {
            cpu_type,
            jit: JitAsm::new(
                jit_memory,
                regs.clone(),
                cp15_context.clone(),
                mem_handler.clone(),
                cpu_type,
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
