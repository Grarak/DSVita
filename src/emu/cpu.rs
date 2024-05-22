use crate::emu::cp15::Cp15;
use crate::emu::thread_regs::ThreadRegs;
use crate::emu::CpuType::{ARM7, ARM9};
use std::ops::DerefMut;

pub struct CpuArm9 {
    thread_regs: Box<ThreadRegs>,
    cp15: Cp15,
}

impl CpuArm9 {
    pub fn new() -> Self {
        CpuArm9 {
            thread_regs: ThreadRegs::new(ARM9),
            cp15: Cp15::new(),
        }
    }

    pub fn regs(&self) -> &ThreadRegs {
        &self.thread_regs
    }

    pub fn regs_mut(&mut self) -> &mut ThreadRegs {
        self.thread_regs.deref_mut()
    }

    pub fn cp15(&self) -> &Cp15 {
        &self.cp15
    }

    pub fn cp15_mut(&mut self) -> &mut Cp15 {
        &mut self.cp15
    }
}

pub struct CpuArm7 {
    thread_regs: Box<ThreadRegs>,
}

impl CpuArm7 {
    pub fn new() -> Self {
        CpuArm7 { thread_regs: ThreadRegs::new(ARM7) }
    }

    pub fn regs(&self) -> &ThreadRegs {
        &self.thread_regs
    }

    pub fn regs_mut(&mut self) -> &mut ThreadRegs {
        self.thread_regs.deref_mut()
    }

    pub fn cp15(&self) -> &Cp15 {
        unreachable!()
    }

    pub fn cp15_mut(&mut self) -> &mut Cp15 {
        unreachable!()
    }
}
