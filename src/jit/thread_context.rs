use crate::jit::assembler::arm::alu_assembler::{AluImm, AluReg};
use crate::jit::assembler::arm::transfer_assembler::LdmStm;
use crate::jit::jit::JitAsm;
use crate::jit::reg::{Reg, GP_REGS};
use crate::memory::VmManager;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
#[repr(C)]
pub struct ThreadRegs {
    pub regs: [u32; 16],
    pub sp: u32,
    pub lr: u32,
    pub pc: u32,
}

impl ThreadRegs {
    pub fn emit_restore_regs(&self) -> [u32; 7] {
        let regs_addr = self.regs.as_ptr() as u32;
        [
            LdmStm::push_al(&[Reg::LR]),
            AluReg::mov_al(Reg::LR, Reg::SP),
            AluImm::mov16_al(Reg::SP, (regs_addr & 0xFFFF) as u16),
            AluImm::mov_t_al(Reg::SP, (regs_addr >> 16) as u16),
            LdmStm::pop_al(&GP_REGS),
            AluReg::mov_al(Reg::SP, Reg::LR),
            LdmStm::pop_al(&[Reg::LR]),
        ]
    }

    pub fn save_regs(&self) -> [u32; 7] {
        let regs_addr = self.regs.as_ptr() as u32;
        [
            LdmStm::push_al(&[Reg::LR]),
            AluReg::mov_al(Reg::LR, Reg::SP),
            AluImm::mov16_al(Reg::SP, (regs_addr & 0xFFFF) as u16),
            AluImm::mov_t_al(Reg::SP, (regs_addr >> 16) as u16),
            LdmStm::push_al(&GP_REGS),
            AluReg::mov_al(Reg::SP, Reg::LR),
            LdmStm::pop_al(&[Reg::LR]),
        ]
    }
}

pub struct ThreadCtx {
    jit: JitAsm,
    pub regs: Rc<RefCell<ThreadRegs>>,
    vmm: Rc<RefCell<VmManager>>,
}

impl ThreadCtx {
    pub fn new(vmm: VmManager) -> Self {
        let vmm = Rc::new(RefCell::new(vmm));
        let regs = Rc::new(RefCell::new(ThreadRegs::default()));

        ThreadCtx {
            jit: JitAsm::new(vmm.clone(), regs.clone()),
            regs,
            vmm,
        }
    }

    pub fn run(&mut self) {
        let pc = self.regs.borrow().pc;
        self.jit.execute(pc)
    }
}
