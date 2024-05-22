use crate::emu::cycle_manager::{CycleManager, EventType};
use crate::emu::emu::{get_cpu_regs, get_cpu_regs_mut, Emu};
use crate::emu::exception_handler::ExceptionVector;
use crate::emu::CpuType::ARM7;
use crate::emu::{exception_handler, CpuType};
use crate::logging::debug_println;
use std::mem;
use CpuType::ARM9;

#[repr(u8)]
#[derive(Copy, Clone, Debug)]
pub enum InterruptFlag {
    LcdVBlank = 0,
    LcdHBlank = 1,
    LcdVCounterMatch = 2,
    Timer0Overflow = 3,
    Timer1Overflow = 4,
    Timer2Overflow = 5,
    Timer3Overflow = 6,
    Rtc = 7,
    Dma0 = 8,
    Dma1 = 9,
    Dma2 = 10,
    Dma3 = 11,
    Keypad = 12,
    GbaSlot = 13,
    IpcSync = 16,
    IpcSendFifoEmpty = 17,
    IpcRecvFifoNotEmpty = 18,
    NdsSlotTransferCompletion = 19,
    NdsSlotIreqMc = 20,
    GeometryCmdFifo = 21,
    ScreensUnfolding = 22,
    SpiBus = 23,
    Wifi = 24,
}

impl From<u8> for InterruptFlag {
    fn from(value: u8) -> Self {
        debug_assert!(value <= InterruptFlag::Wifi as u8);
        unsafe { mem::transmute(value) }
    }
}

pub struct CpuRegs {
    cpu_type: CpuType,
    pub ime: u8,
    pub ie: u32,
    pub irf: u32,
    pub post_flg: u8,
    pub halt_cnt: u8,
    cpsr_irq_enabled: bool,
    halt: u8,
    pub bios_wait_flags: u32,
}

impl CpuRegs {
    pub fn new(cpu_type: CpuType) -> Self {
        CpuRegs {
            cpu_type,
            ime: 0,
            ie: 0,
            irf: 0,
            post_flg: 0,
            halt_cnt: 0,
            cpsr_irq_enabled: false,
            halt: 0,
            bios_wait_flags: 0,
        }
    }

    pub fn set_ime(&mut self, value: u8, cycle_manager: &mut CycleManager) {
        self.ime = value & 0x1;
        self.check_for_interrupt(cycle_manager);
    }

    pub fn set_ie(&mut self, mut mask: u32, value: u32, cycle_manager: &mut CycleManager) {
        mask &= match self.cpu_type {
            ARM9 => 0x003F3F7F,
            ARM7 => 0x01FF3FFF,
        };
        self.ie = (self.ie & !mask) | (value & mask);
        self.check_for_interrupt(cycle_manager);
    }

    pub fn check_for_interrupt(&self, cycle_manager: &mut CycleManager) {
        if self.ime != 0 && (self.ie & self.irf) != 0 && self.cpsr_irq_enabled {
            self.schedule_interrupt(cycle_manager);
        }
    }

    fn schedule_interrupt(&self, cycle_manager: &mut CycleManager) {
        match self.cpu_type {
            ARM9 => cycle_manager.schedule(1, EventType::CpuInterruptArm9),
            ARM7 => cycle_manager.schedule(1, EventType::CpuInterruptArm7),
        };
    }

    pub fn set_irf(&mut self, mask: u32, value: u32) {
        debug_println!("{:?} set irf {:x} {:x}", self.cpu_type, mask, value);
        self.irf &= !(value & mask);
    }

    pub fn set_post_flg(&mut self, value: u8) {
        self.post_flg |= value & 0x1;
        if self.cpu_type == ARM9 {
            self.post_flg = (self.post_flg & !0x2) | (value & 0x2);
        }
    }

    pub fn halt(&mut self, bit: u8) {
        debug_println!("{:?} halt with bit {}", self.cpu_type, bit);
        self.halt |= 1;
    }

    pub fn is_halted(&self) -> bool {
        self.halt != 0
    }

    pub fn set_cpsr_irq_enabled(&mut self, enabled: bool) {
        self.cpsr_irq_enabled = enabled;
    }

    pub fn send_interrupt(&mut self, flag: InterruptFlag, cycle_manager: &mut CycleManager) {
        self.irf |= 1 << flag as u8;
        debug_println!("{:?} send interrupt {:?} {:x} {:x} {:x} {}", self.cpu_type, flag, self.ie, self.irf, self.ime, self.cpsr_irq_enabled);
        if (self.ie & self.irf) != 0 {
            if self.ime != 0 && self.cpsr_irq_enabled {
                debug_println!("{:?} schedule send interrupt {:?}", self.cpu_type, flag);
                self.schedule_interrupt(cycle_manager);
            } else if self.cpu_type == ARM7 || self.ime != 0 {
                debug_println!("{:?} unhalt send interrupt {:?}", self.cpu_type, flag);
                self.halt &= !1;
            }
        }
    }

    pub fn set_halt_cnt(&mut self, value: u8) {
        self.halt_cnt = value & 0xC0;

        match self.halt_cnt {
            1 => {
                todo!("gba mode")
            }
            2 => {
                todo!("halt")
            }
            _ => {}
        }
    }

    pub fn on_interrupt_event<const CPU: CpuType>(emu: &mut Emu) {
        let interrupted = {
            let cpu_regs = get_cpu_regs!(emu, CPU);
            let interrupt = cpu_regs.ime != 0 && (cpu_regs.ie & cpu_regs.irf) != 0 && cpu_regs.cpsr_irq_enabled;
            if interrupt {
                debug_println!("{:?} interrupt {:x} {:x}", cpu_regs.cpu_type, cpu_regs.ie, cpu_regs.irf);
            }
            interrupt
        };
        if interrupted {
            exception_handler::handle::<CPU, false>(emu, 0, ExceptionVector::NormalInterrupt);
            get_cpu_regs_mut!(emu, CPU).halt &= !1;
        }
    }
}
