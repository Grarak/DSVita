use crate::hle::cp15_context::Cp15Context;
use crate::hle::registers::ThreadRegs;
use crate::hle::CpuType;
use crate::jit::jit_asm::JitAsm;
use crate::memory::VmManager;
use std::cell::RefCell;
use std::rc::Rc;

pub struct ThreadContext {
    jit: JitAsm,
    pub regs: Rc<RefCell<ThreadRegs>>,
    pub cp15_context: Rc<RefCell<Cp15Context>>,
}

impl ThreadContext {
    pub fn new(vmm: Rc<RefCell<VmManager>>, cpu_type: CpuType) -> Self {
        let regs = ThreadRegs::new();
        let cp15_context = Rc::new(RefCell::new(Cp15Context::new()));

        ThreadContext {
            jit: JitAsm::new(vmm, regs.clone(), cp15_context.clone(), cpu_type),
            regs,
            cp15_context,
        }
    }

    pub fn run(&mut self) {
        loop {
            self.jit.execute()
        }
    }
}
