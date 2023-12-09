use crate::hle::CpuType;
use crate::jit::assembler::arm::alu_assembler::AluImm;
use crate::jit::assembler::arm::transfer_assembler::{LdmStm, LdrStrImm, Msr};
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::Cond;
use crate::logging::debug_println;
use bilge::prelude::*;
use std::cell::RefCell;
use std::ptr;
use std::rc::Rc;

#[bitsize(32)]
#[derive(FromBits)]
pub struct Cpsr {
    pub mode: u5,
    pub thumb: u1,
    pub fiq_disable: u1,
    pub irq_disable: u1,
    pub reserved: u19,
    pub q: u1,
    pub v: u1,
    pub c: u1,
    pub z: u1,
    pub n: u1,
}

#[derive(Default)]
pub struct UserRegs {
    pub gp_regs: [u32; 5],
    pub sp: u32,
    pub lr: u32,
}

#[derive(Default)]
pub struct FiqRegs {
    pub gp_regs: [u32; 5],
    pub sp: u32,
    pub lr: u32,
    pub spsr: u32,
}

#[derive(Default)]
pub struct OtherModeRegs {
    pub sp: u32,
    pub lr: u32,
    pub spsr: u32,
}

#[derive(Default)]
pub struct ThreadRegs {
    pub gp_regs: [u32; 13],
    pub sp: u32,
    pub lr: u32,
    pub pc: u32,
    pub cpsr: u32,
    pub spsr: u32,
    pub user: UserRegs,
    pub fiq: FiqRegs,
    pub svc: OtherModeRegs,
    pub abt: OtherModeRegs,
    pub irq: OtherModeRegs,
    pub und: OtherModeRegs,
    pub ime: bool,
    pub post_flg: u8,
    pub cpu_type: CpuType,
    pub restore_regs_opcodes: [u32; 6],
    pub save_regs_opcodes: [u32; 4],
}

impl ThreadRegs {
    pub fn new(cpu_type: CpuType) -> Rc<RefCell<Self>> {
        let instance = Rc::new(RefCell::new(ThreadRegs::default()));

        {
            let mut instance = instance.borrow_mut();

            instance.cpu_type = cpu_type;

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

    pub fn set_cpsr(&mut self, value: u32) {
        let old_cpsr = Cpsr::from(self.cpsr);
        let new_cpsr = Cpsr::from(value);

        let old_mode = u8::from(old_cpsr.mode());
        let new_mode = u8::from(new_cpsr.mode());
        if old_mode != new_mode {
            match old_mode {
                // User | System
                0x10 | 0x1F => {
                    self.user.gp_regs.copy_from_slice(&self.gp_regs[8..13]);
                    self.user.sp = self.sp;
                    self.user.lr = self.lr;
                }
                // FIQ
                0x11 => {
                    self.fiq.gp_regs.copy_from_slice(&self.gp_regs[8..13]);
                    self.fiq.sp = self.sp;
                    self.fiq.lr = self.lr;
                    self.fiq.spsr = self.spsr;
                }
                // IRQ
                0x12 => {
                    self.irq.sp = self.sp;
                    self.irq.lr = self.lr;
                    self.irq.spsr = self.spsr;
                }
                // Supervisor
                0x13 => {
                    self.svc.sp = self.sp;
                    self.svc.lr = self.lr;
                    self.svc.spsr = self.spsr;
                }
                // Abort
                0x17 => {
                    self.abt.sp = self.sp;
                    self.abt.lr = self.lr;
                    self.abt.spsr = self.spsr;
                }
                // Undefined
                0x1B => {
                    self.und.sp = self.sp;
                    self.und.lr = self.lr;
                    self.und.spsr = self.spsr;
                }
                _ => {
                    debug_println!("Unknown old cpsr mode {:x}", new_mode)
                }
            }

            match new_mode {
                // User | System
                0x10 | 0x1F => {
                    self.gp_regs[8..13].copy_from_slice(&self.user.gp_regs);
                    self.sp = self.user.sp;
                    self.lr = self.user.lr;
                }
                // FIQ
                0x11 => {
                    self.gp_regs[8..13].copy_from_slice(&self.fiq.gp_regs);
                    self.sp = self.fiq.sp;
                    self.lr = self.fiq.lr;
                    self.spsr = self.fiq.spsr;
                }
                // IRQ
                0x12 => {
                    self.sp = self.irq.sp;
                    self.lr = self.irq.lr;
                    self.spsr = self.irq.spsr;
                }
                // Supervisor
                0x13 => {
                    self.sp = self.svc.sp;
                    self.lr = self.svc.lr;
                    self.spsr = self.svc.spsr;
                }
                // Abort
                0x17 => {
                    self.sp = self.abt.sp;
                    self.lr = self.abt.lr;
                    self.spsr = self.abt.spsr;
                }
                // Undefined
                0x1B => {
                    self.sp = self.und.sp;
                    self.lr = self.und.lr;
                    self.spsr = self.und.spsr;
                }
                _ => {
                    debug_println!("Unknown new cpsr mode {:x}", new_mode)
                }
            }
        }

        self.cpsr = value;
    }

    pub fn set_ime(&mut self, value: u8) {
        self.ime = value & 0x1 == 1;
    }

    pub fn set_post_flg(&mut self, value: u8) {
        self.post_flg |= value & 0x1;
        if self.cpu_type == CpuType::ARM9 {
            self.post_flg = (self.post_flg & !0x2) | (value & 0x2);
        }
    }
}

#[cfg_attr(target_os = "vita", instruction_set(arm::a32))]
pub unsafe extern "C" fn register_set_cpsr(context: *mut ThreadRegs, value: u32) {
    (*context).set_cpsr(value)
}
