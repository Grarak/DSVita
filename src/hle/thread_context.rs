use crate::hle::cp15_context::Cp15Context;
use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::memory::indirect_memory::indirect_mem_handler::IndirectMemHandler;
use crate::hle::memory::memory::Memory;
use crate::hle::registers::ThreadRegs;
use crate::hle::spu_context::SpuContext;
use crate::hle::CpuType;
use crate::jit::jit_asm::JitAsm;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub struct ThreadContext {
    jit: JitAsm,
    pub regs: Rc<RefCell<ThreadRegs>>,
    pub cp15_context: Rc<RefCell<Cp15Context>>,
    pub indirect_mem_handler: Rc<RefCell<IndirectMemHandler>>,
}

impl ThreadContext {
    pub fn new(memory: Arc<Mutex<Memory>>, cpu_type: CpuType) -> Self {
        let regs = ThreadRegs::new(cpu_type);
        let cp15_context = Rc::new(RefCell::new(Cp15Context::new()));
        let gpu_context = Rc::new(RefCell::new(GpuContext::new()));
        let spu_context = Rc::new(RefCell::new(SpuContext::new()));
        let indirect_mem_handler =
            Rc::new(RefCell::new(IndirectMemHandler::new(
                cpu_type,
                memory.clone(),
                regs.clone(),
                gpu_context,
                spu_context,
            )));

        ThreadContext {
            jit: JitAsm::new(
                memory,
                regs.clone(),
                cp15_context.clone(),
                indirect_mem_handler.clone(),
                cpu_type,
            ),
            regs,
            cp15_context,
            indirect_mem_handler,
        }
    }

    pub fn run(&mut self) {
        loop {
            self.jit.execute()
        }
    }
}
