use crate::hle::cp15_context::Cp15Context;
use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, LdrStrImm, Msr};
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::Cond;
use crate::memory::VmManager;
use std::cell::RefCell;
use std::ptr;
use std::rc::Rc;

#[derive(Default)]
#[repr(C)]
pub struct ThreadRegs {
    pub gp_regs: [u32; 13],
    pub sp: u32,
    pub lr: u32,
    pub pc: u32,
    pub cpsr: u32,
    pub restore_regs_opcodes: [u32; 6],
    pub save_regs_opcodes: [u32; 4],
}

impl ThreadRegs {
    fn new() -> Rc<RefCell<Self>> {
        let instance = Rc::new(RefCell::new(ThreadRegs::default()));

        {
            let mut instance = instance.borrow_mut();

            let gp_regs_addr = instance.gp_regs.as_ptr() as u32;
            let last_regs_addr = ptr::addr_of!(instance.gp_regs[instance.gp_regs.len() - 1]) as u32;
            let sp_addr = ptr::addr_of!(instance.sp) as u32;
            let cpsr_addr = ptr::addr_of!(instance.cpsr) as u32;
            assert_eq!(sp_addr - last_regs_addr, 4);

            {
                let mov = AluImm::mov32(Reg::SP, gp_regs_addr);
                instance.restore_regs_opcodes = [
                    mov[0],
                    mov[1],
                    LdrStrImm::ldr_offset_al(Reg::R0, Reg::SP, (cpsr_addr - gp_regs_addr) as u16),
                    Msr::cpsr_flags(Reg::R0, Cond::AL),
                    LdmStm::pop_post_al(RegReserve::gp()),
                    LdrStrImm::ldr_al(Reg::SP, Reg::SP),
                ]
            }

            {
                let mov = AluImm::mov32(Reg::LR, last_regs_addr);
                instance.save_regs_opcodes = [
                    mov[0],
                    mov[1],
                    LdrStrImm::str_offset_al(Reg::SP, Reg::LR, 4),
                    LdmStm::push_post(RegReserve::gp(), Reg::LR, Cond::AL),
                ]
            }
        }

        instance
    }

    pub fn emit_save_gp_regs(&self, tmp_reg: Reg) -> [u32; 3] {
        debug_assert!(tmp_reg > Reg::R12);
        let last_regs_addr = ptr::addr_of!(self.gp_regs[self.gp_regs.len() - 1]) as u32;
        let mov = AluImm::mov32(tmp_reg, last_regs_addr);

        [
            mov[0],
            mov[1],
            LdmStm::push_post(RegReserve::gp(), tmp_reg, Cond::AL),
        ]
    }

    pub fn emit_get_reg(&self, dest_reg: Reg, src_reg: Reg) -> [u32; 3] {
        let reg_addr = self.get_reg_value(src_reg) as *const _ as u32;

        let mov = AluImm::mov32(dest_reg, reg_addr);
        [mov[0], mov[1], LdrStrImm::ldr_al(dest_reg, dest_reg)]
    }

    pub fn emit_set_reg(&self, dest_reg: Reg, src_reg: Reg, tmp_reg: Reg) -> [u32; 3] {
        debug_assert_ne!(src_reg, tmp_reg);

        let reg_addr = self.get_reg_value(dest_reg) as *const _ as u32;

        let mov = AluImm::mov32(tmp_reg, reg_addr);
        [mov[0], mov[1], LdrStrImm::str_al(src_reg, tmp_reg)]
    }

    pub fn get_reg_value(&self, reg: Reg) -> &u32 {
        match reg {
            Reg::SP => &self.sp,
            Reg::LR => &self.lr,
            Reg::PC => &self.pc,
            Reg::CPSR => &self.cpsr,
            Reg::None => panic!(),
            _ => {
                if reg >= Reg::R0 && reg <= Reg::R12 {
                    &self.gp_regs[reg as usize]
                } else {
                    panic!()
                }
            }
        }
    }

    pub fn get_reg_value_mut(&mut self, reg: Reg) -> &mut u32 {
        match reg {
            Reg::SP => &mut self.sp,
            Reg::LR => &mut self.lr,
            Reg::PC => &mut self.pc,
            Reg::CPSR => &mut self.cpsr,
            Reg::None => panic!(),
            _ => {
                if reg >= Reg::R0 && reg <= Reg::R12 {
                    &mut self.gp_regs[reg as usize]
                } else {
                    panic!()
                }
            }
        }
    }
}

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
