use crate::hle::cp15_context::Cp15Context;
use crate::hle::gpu::gpu_context::GpuContext;
use crate::hle::memory::indirect_memory::indirect_mem_handler::IndirectMemHandler;
use crate::hle::memory::memory::Memory;
use crate::hle::registers::ThreadRegs;
use crate::hle::CpuType;
use crate::jit::jit_asm::JitAsm;
use std::cell::RefCell;
use std::rc::Rc;

pub struct ThreadContext {
    jit: JitAsm,
    pub regs: Rc<RefCell<ThreadRegs>>,
    pub cp15_context: Rc<RefCell<Cp15Context>>,
    pub indirect_mem_handler: Rc<RefCell<IndirectMemHandler>>,
}

impl ThreadContext {
    pub fn new(memory: Rc<RefCell<Memory>>, cpu_type: CpuType) -> Self {
        let vmm = memory.borrow().vmm.clone();
        let regs = ThreadRegs::new(cpu_type);
        let cp15_context = Rc::new(RefCell::new(Cp15Context::new()));
        let gpu_context = Rc::new(RefCell::new(GpuContext::new()));
        let indirect_mem_handler = Rc::new(
            RefCell::new(IndirectMemHandler::new(cpu_type, memory, regs.clone(), gpu_context))
        );

        ThreadContext {
            jit: JitAsm::new(
                vmm,
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
