use crate::core::emu::Emu;
use crate::core::CpuType;
use crate::jit::reg::Reg;
use crate::logging::debug_println;
use crate::{DEBUG_LOG, IS_DEBUG};
use bilge::prelude::*;
use std::ptr;

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
}

impl ThreadRegs {
    pub fn new() -> Self {
        ThreadRegs {
            gp_regs: [0u32; 13],
            sp: 0,
            lr: 0,
            pc: 0,
            cpsr: 0,
            spsr: 0,
            user: UserRegs::default(),
            fiq: FiqRegs::default(),
            svc: OtherModeRegs::default(),
            abt: OtherModeRegs::default(),
            irq: OtherModeRegs::default(),
            und: OtherModeRegs::default(),
        }
    }
}

impl Emu {
    pub fn thread_get_reg_mut_ptr(&mut self, cpu: CpuType) -> *mut u32 {
        self.thread[cpu].gp_regs.as_mut_ptr()
    }

    pub fn thread_get_reg(&self, cpu: CpuType, reg: Reg) -> &u32 {
        debug_assert_ne!(reg, Reg::None);
        let base_ptr = ptr::addr_of!(self.thread[cpu].gp_regs[0]);
        unsafe { base_ptr.offset(reg as _).as_ref().unwrap_unchecked() }
    }

    pub fn thread_get_reg_mut(&mut self, cpu: CpuType, reg: Reg) -> &mut u32 {
        debug_assert_ne!(reg, Reg::None);
        let base_ptr = ptr::addr_of_mut!(self.thread[cpu].gp_regs[0]);
        unsafe { base_ptr.offset(reg as _).as_mut().unwrap_unchecked() }
    }

    pub fn thread_get_reg_usr(&self, cpu: CpuType, reg: Reg) -> &u32 {
        debug_assert_ne!(reg, Reg::None);
        match reg {
            Reg::R8 => &self.thread[cpu].user.gp_regs[0],
            Reg::R9 => &self.thread[cpu].user.gp_regs[1],
            Reg::R10 => &self.thread[cpu].user.gp_regs[2],
            Reg::R11 => &self.thread[cpu].user.gp_regs[3],
            Reg::R12 => &self.thread[cpu].user.gp_regs[4],
            Reg::SP => &self.thread[cpu].user.sp,
            Reg::LR => &self.thread[cpu].user.lr,
            _ => self.thread_get_reg(cpu, reg),
        }
    }

    pub fn thread_get_reg_usr_mut(&mut self, cpu: CpuType, reg: Reg) -> &mut u32 {
        debug_assert_ne!(reg, Reg::None);
        match reg {
            Reg::R8 => &mut self.thread[cpu].user.gp_regs[0],
            Reg::R9 => &mut self.thread[cpu].user.gp_regs[1],
            Reg::R10 => &mut self.thread[cpu].user.gp_regs[2],
            Reg::R11 => &mut self.thread[cpu].user.gp_regs[3],
            Reg::R12 => &mut self.thread[cpu].user.gp_regs[4],
            Reg::SP => &mut self.thread[cpu].user.sp,
            Reg::LR => &mut self.thread[cpu].user.lr,
            _ => self.thread_get_reg_mut(cpu, reg),
        }
    }

    #[inline]
    pub fn thread_set_cpsr_with_flags(&mut self, cpu: CpuType, value: u32, flags: u8) {
        if flags & 1 == 1 {
            let mask = if u8::from(Cpsr::from(self.thread[cpu].cpsr).mode()) == 0x10 { 0xE0 } else { 0xFF };
            self.thread_set_cpsr::<false>(cpu, (self.thread[cpu].cpsr & !mask) | (value & mask));
        }

        for i in 1..4 {
            if (flags & (1 << i)) != 0 {
                let mask = 0xFF << (i << 3);
                self.thread[cpu].cpsr = (self.thread[cpu].cpsr & !mask) | (value & mask);
            }
        }
    }

    #[inline]
    pub fn thread_set_spsr_with_flags(&mut self, cpu: CpuType, value: u32, flags: u8) {
        let regs = &mut self.thread[cpu];
        if IS_DEBUG {
            let mode = u8::from(Cpsr::from(regs.cpsr).mode());
            debug_assert_ne!(mode, 0x10);
            debug_assert_ne!(mode, 0x1F);
        }

        for i in 0..4 {
            if (flags & (1 << i)) != 0 {
                let mask = 0xFF << (i << 3);
                regs.spsr = (regs.spsr & !mask) | (value & mask);
            }
        }
    }

    #[inline]
    pub fn thread_restore_spsr(&mut self, cpu: CpuType) {
        if !self.thread_is_user_mode(cpu) {
            self.thread_set_cpsr::<false>(cpu, self.thread[cpu].spsr);
        }
    }

    pub fn thread_restore_thumb_mode(&mut self, cpu: CpuType) {
        let regs = &mut self.thread[cpu];
        regs.pc &= !1;
        regs.pc |= Cpsr::from(regs.cpsr).thumb() as u32;
    }

    pub fn thread_force_pc_arm_mode(&mut self, cpu: CpuType) {
        self.thread[cpu].pc &= !1;
    }

    pub fn thread_force_pc_thumb_mode(&mut self, cpu: CpuType) {
        self.thread[cpu].pc |= 1;
    }

    pub fn thread_set_cpsr<const SAVE: bool>(&mut self, cpu: CpuType, value: u32) {
        let regs = &mut self.thread[cpu];
        let current_cpsr = Cpsr::from(regs.cpsr);
        let new_cpsr = Cpsr::from(value);

        let current_mode = u8::from(current_cpsr.mode());
        let new_mode = u8::from(new_cpsr.mode());
        debug_println!("{cpu:?} set cpsr from mode {current_mode:x} to {new_mode:x}");
        if current_mode != new_mode {
            match current_mode {
                // User | System
                0x10 | 0x1F => {
                    regs.user.gp_regs.copy_from_slice(&regs.gp_regs[8..13]);
                    regs.user.sp = regs.sp;
                    regs.user.lr = regs.lr;
                }
                // FIQ
                0x11 => {
                    regs.fiq.gp_regs.copy_from_slice(&regs.gp_regs[8..13]);
                    regs.fiq.sp = regs.sp;
                    regs.fiq.lr = regs.lr;
                    regs.fiq.spsr = regs.spsr;
                }
                // IRQ
                0x12 => {
                    regs.user.gp_regs.copy_from_slice(&regs.gp_regs[8..13]);
                    regs.irq.sp = regs.sp;
                    regs.irq.lr = regs.lr;
                    regs.irq.spsr = regs.spsr;
                }
                // Supervisor
                0x13 => {
                    regs.user.gp_regs.copy_from_slice(&regs.gp_regs[8..13]);
                    regs.svc.sp = regs.sp;
                    regs.svc.lr = regs.lr;
                    regs.svc.spsr = regs.spsr;
                }
                // Abort
                0x17 => {
                    regs.user.gp_regs.copy_from_slice(&regs.gp_regs[8..13]);
                    regs.abt.sp = regs.sp;
                    regs.abt.lr = regs.lr;
                    regs.abt.spsr = regs.spsr;
                }
                // Undefined
                0x1B => {
                    regs.user.gp_regs.copy_from_slice(&regs.gp_regs[8..13]);
                    regs.und.sp = regs.sp;
                    regs.und.lr = regs.lr;
                    regs.und.spsr = regs.spsr;
                }
                _ => debug_println!("Unknown old cpsr mode {:x}", new_mode),
            }

            match new_mode {
                // User | System
                0x10 | 0x1F => {
                    regs.gp_regs[8..13].copy_from_slice(&regs.user.gp_regs);
                    regs.sp = regs.user.sp;
                    regs.lr = regs.user.lr;
                    if DEBUG_LOG {
                        regs.spsr = 0;
                    }
                }
                // FIQ
                0x11 => {
                    regs.gp_regs[8..13].copy_from_slice(&regs.fiq.gp_regs);
                    regs.sp = regs.fiq.sp;
                    regs.lr = regs.fiq.lr;
                    regs.spsr = regs.fiq.spsr;
                }
                // IRQ
                0x12 => {
                    regs.gp_regs[8..13].copy_from_slice(&regs.user.gp_regs);
                    regs.sp = regs.irq.sp;
                    regs.lr = regs.irq.lr;
                    regs.spsr = regs.irq.spsr;
                }
                // Supervisor
                0x13 => {
                    regs.gp_regs[8..13].copy_from_slice(&regs.user.gp_regs);
                    regs.sp = regs.svc.sp;
                    regs.lr = regs.svc.lr;
                    regs.spsr = regs.svc.spsr;
                }
                // Abort
                0x17 => {
                    regs.gp_regs[8..13].copy_from_slice(&regs.user.gp_regs);
                    regs.sp = regs.abt.sp;
                    regs.lr = regs.abt.lr;
                    regs.spsr = regs.abt.spsr;
                }
                // Undefined
                0x1B => {
                    regs.gp_regs[8..13].copy_from_slice(&regs.user.gp_regs);
                    regs.sp = regs.und.sp;
                    regs.lr = regs.und.lr;
                    regs.spsr = regs.und.spsr;
                }
                _ => debug_println!("Unknown new cpsr mode {:x}", new_mode),
            }
        }

        if SAVE {
            regs.spsr = regs.cpsr;
        }
        regs.cpsr = value;
        self.cpu_check_for_interrupt(cpu);
    }

    pub fn thread_set_thumb(&mut self, cpu: CpuType, thumb: bool) {
        let regs = &mut self.thread[cpu];
        regs.cpsr = (regs.cpsr & !(1 << 5)) | ((thumb as u32) << 5);
    }

    pub fn thread_is_thumb(&self, cpu: CpuType) -> bool {
        Cpsr::from(self.thread[cpu].cpsr).thumb()
    }

    pub fn thread_is_user_mode(&self, cpu: CpuType) -> bool {
        let mode = u8::from(Cpsr::from(self.thread[cpu].cpsr).mode());
        mode == 0x10 || mode == 0x1F
    }

    pub fn thread_is_fiq_mode(&self, cpu: CpuType) -> bool {
        u8::from(Cpsr::from(self.thread[cpu].cpsr).mode()) == 0x11
    }
}
