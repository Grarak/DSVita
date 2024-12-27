use crate::core::cpu_regs::CpuRegs;
use crate::core::emu::Emu;
use crate::core::CpuType;
use crate::jit::reg::Reg;
use crate::logging::debug_println;
use crate::DEBUG_LOG;
use bilge::prelude::*;
use std::{mem, ptr};

#[bitsize(32)]
#[derive(FromBits)]
pub struct Cpsr {
    pub mode: u5,
    pub thumb: bool,
    pub fiq_disable: bool,
    pub irq_disable: bool,
    pub reserved: u19,
    pub q: bool,
    pub v: bool,
    pub c: bool,
    pub z: bool,
    pub n: bool,
}

#[repr(C)]
#[derive(Default)]
pub struct UserRegs {
    pub gp_regs: [u32; 5],
    pub sp: u32,
    pub lr: u32,
}

#[repr(C)]
#[derive(Default)]
pub struct FiqRegs {
    pub gp_regs: [u32; 5],
    pub sp: u32,
    pub lr: u32,
    pub spsr: u32,
}

#[repr(C)]
#[derive(Default)]
pub struct OtherModeRegs {
    pub sp: u32,
    pub lr: u32,
    pub spsr: u32,
}

#[repr(C, align(32))]
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
    pub cpu: CpuRegs,
    is_user: bool,
}

impl ThreadRegs {
    pub fn new(cpu_type: CpuType) -> Self {
        ThreadRegs {
            gp_regs: [0u32; 13],
            sp: 0,
            lr: 0,
            pc: 0,
            cpsr: 0,
            spsr: 0,
            is_user: false,
            user: UserRegs::default(),
            fiq: FiqRegs::default(),
            svc: OtherModeRegs::default(),
            abt: OtherModeRegs::default(),
            irq: OtherModeRegs::default(),
            und: OtherModeRegs::default(),
            cpu: CpuRegs::new(cpu_type),
        }
    }

    pub fn get_reg_mut_ptr(&mut self) -> *mut u32 {
        self.gp_regs.as_mut_ptr()
    }

    pub fn get_reg(&self, reg: Reg) -> &u32 {
        debug_assert_ne!(reg, Reg::None);
        let base_ptr = ptr::addr_of!(self.gp_regs[0]);
        unsafe { base_ptr.offset(reg as _).as_ref().unwrap_unchecked() }
    }

    pub fn get_reg_mut(&mut self, reg: Reg) -> &mut u32 {
        debug_assert_ne!(reg, Reg::None);
        let base_ptr = ptr::addr_of_mut!(self.gp_regs[0]);
        unsafe { base_ptr.offset(reg as _).as_mut().unwrap_unchecked() }
    }

    pub fn get_reg_usr(&self, reg: Reg) -> &u32 {
        debug_assert_ne!(reg, Reg::None);
        match reg {
            Reg::R8 => &self.user.gp_regs[0],
            Reg::R9 => &self.user.gp_regs[1],
            Reg::R10 => &self.user.gp_regs[2],
            Reg::R11 => &self.user.gp_regs[3],
            Reg::R12 => &self.user.gp_regs[4],
            Reg::SP => &self.user.sp,
            Reg::LR => &self.user.lr,
            _ => self.get_reg(reg),
        }
    }

    pub fn get_reg_usr_mut(&mut self, reg: Reg) -> &mut u32 {
        debug_assert_ne!(reg, Reg::None);
        match reg {
            Reg::R8 => &mut self.user.gp_regs[0],
            Reg::R9 => &mut self.user.gp_regs[1],
            Reg::R10 => &mut self.user.gp_regs[2],
            Reg::R11 => &mut self.user.gp_regs[3],
            Reg::R12 => &mut self.user.gp_regs[4],
            Reg::SP => &mut self.user.sp,
            Reg::LR => &mut self.user.lr,
            _ => self.get_reg_mut(reg),
        }
    }

    pub const fn get_user_regs_offset() -> u8 {
        mem::offset_of!(ThreadRegs, user) as u8
    }

    #[inline]
    pub fn set_cpsr_with_flags(&mut self, value: u32, flags: u8, emu: &mut Emu) {
        if flags & 1 == 1 {
            let mask = if u8::from(Cpsr::from(self.cpsr).mode()) == 0x10 { 0xE0 } else { 0xFF };
            self.set_cpsr::<false>((self.cpsr & !mask) | (value & mask), emu);
        }

        for i in 1..4 {
            if (flags & (1 << i)) != 0 {
                let mask = 0xFF << (i << 3);
                self.cpsr = (self.cpsr & !mask) | (value & mask);
            }
        }
    }

    #[inline]
    pub fn set_spsr_with_flags(&mut self, value: u32, flags: u8) {
        if DEBUG_LOG {
            let mode = u8::from(Cpsr::from(self.cpsr).mode());
            debug_assert_ne!(mode, 0x10);
            debug_assert_ne!(mode, 0x1F);
        }

        for i in 0..4 {
            if (flags & (1 << i)) != 0 {
                let mask = 0xFF << (i << 3);
                self.spsr = (self.spsr & !mask) | (value & mask);
            }
        }
    }

    #[inline]
    pub fn restore_spsr(&mut self, emu: &mut Emu) {
        if !self.is_user {
            self.set_cpsr::<false>(self.spsr, emu);
        }
    }

    pub fn restore_thumb_mode(&mut self) {
        self.pc &= !1;
        self.pc |= Cpsr::from(self.cpsr).thumb() as u32;
    }

    pub fn force_pc_arm_mode(&mut self) {
        self.pc &= !1;
    }

    pub fn force_pc_thumb_mode(&mut self) {
        self.pc |= 1;
    }

    pub fn set_cpsr<const SAVE: bool>(&mut self, value: u32, emu: &mut Emu) {
        let current_cpsr = Cpsr::from(self.cpsr);
        let new_cpsr = Cpsr::from(value);

        let current_mode = u8::from(current_cpsr.mode());
        let new_mode = u8::from(new_cpsr.mode());
        if current_mode != new_mode {
            match current_mode {
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

            self.is_user = false;
            match new_mode {
                // User | System
                0x10 | 0x1F => {
                    self.gp_regs[8..13].copy_from_slice(&self.user.gp_regs);
                    self.sp = self.user.sp;
                    self.lr = self.user.lr;
                    if DEBUG_LOG {
                        self.spsr = 0;
                    }
                    self.is_user = true;
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

        if SAVE {
            self.spsr = self.cpsr;
        }
        self.cpsr = value;
        self.cpu.check_for_interrupt(emu);
    }

    pub fn set_thumb(&mut self, thumb: bool) {
        self.cpsr = (self.cpsr & !(1 << 5)) | ((thumb as u32) << 5);
    }

    pub fn is_thumb(&self) -> bool {
        bool::from(Cpsr::from(self.cpsr).thumb())
    }
}
