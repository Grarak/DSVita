use crate::core::cycle_manager::{CycleManager, EventType};
use crate::core::emu::{get_cm_mut, get_cpu_regs, get_cpu_regs_mut, get_regs, Emu};
use crate::core::exception_handler::ExceptionVector;
use crate::core::thread_regs::Cpsr;
use crate::core::CpuType::ARM7;
use crate::core::{exception_handler, CpuType};
use crate::logging::debug_println;
use std::fmt::{Debug, Formatter};
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

pub struct InterruptFlags(pub u32);

impl Debug for InterruptFlags {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug_set = f.debug_set();
        for i in 0..=InterruptFlag::Wifi as u8 {
            if self.0 & (1 << i) != 0 {
                let flag = InterruptFlag::from(i);
                debug_set.entry(&flag);
            }
        }
        debug_set.finish()
    }
}

#[repr(C)]
pub struct CpuRegs {
    pub ie: u32,
    pub irf: u32,
    pub ime: u8,
    pub post_flg: u8,
    pub halt_cnt: u8,
    halt: u8,
    pub cpu_type: CpuType,
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
            halt: 0,
            bios_wait_flags: 0,
        }
    }

    pub fn set_ime(&mut self, value: u8, emu: &mut Emu) {
        self.ime = value & 0x1;
        self.check_for_interrupt(emu);
    }

    pub fn set_ie(&mut self, mut mask: u32, value: u32, emu: &mut Emu) {
        mask &= match self.cpu_type {
            ARM9 => 0x003F3F7F,
            ARM7 => 0x01FF3FFF,
        };
        self.ie = (self.ie & !mask) | (value & mask);
        debug_println!("{:?} set ie {:x} {:?}", self.cpu_type, self.ie, InterruptFlags(self.ie));
        self.check_for_interrupt(emu);
    }

    pub fn check_for_interrupt(&self, emu: &mut Emu) {
        if self.ime != 0 && (self.ie & self.irf) != 0 && !Cpsr::from(get_regs!(emu, self.cpu_type).cpsr).irq_disable() {
            self.schedule_interrupt(get_cm_mut!(emu));
        }
    }

    fn schedule_interrupt(&self, cycle_manager: &mut CycleManager) {
        cycle_manager.schedule_imm(
            match self.cpu_type {
                ARM9 => EventType::CpuInterruptArm9,
                ARM7 => EventType::CpuInterruptArm7,
            },
            0,
        )
    }

    pub fn set_irf(&mut self, mask: u32, value: u32) {
        // debug_println!("{:?} set irf {:?}", self.cpu_type, InterruptFlags(value & mask));
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
        self.halt |= 1 << bit;
    }

    pub fn unhalt(&mut self, bit: u8) {
        debug_println!("{:?} unhalt with bit {}", self.cpu_type, bit);
        self.halt &= !(1 << bit);
    }

    pub fn is_halted(&self) -> bool {
        self.halt != 0
    }

    pub fn send_interrupt(&mut self, flag: InterruptFlag, emu: &mut Emu) {
        self.irf |= 1 << flag as u8;
        debug_println!(
            "{:?} send interrupt {:?} {:?} {:?} {:x} {}",
            self.cpu_type,
            flag,
            InterruptFlags(self.ie),
            InterruptFlags(self.irf),
            self.ime,
            !Cpsr::from(get_regs!(emu, self.cpu_type).cpsr).irq_disable()
        );
        if (self.ie & self.irf) != 0 {
            if self.ime != 0 && !Cpsr::from(get_regs!(emu, self.cpu_type).cpsr).irq_disable() {
                debug_println!("{:?} schedule send interrupt {:?}", self.cpu_type, flag);
                self.schedule_interrupt(get_cm_mut!(emu));
            } else if self.cpu_type == ARM7 || self.ime != 0 {
                debug_println!("{:?} unhalt send interrupt {:?}", self.cpu_type, flag);
                self.unhalt(0);
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

    pub fn on_interrupt_event<const CPU: CpuType>(_: &mut CycleManager, emu: &mut Emu, _: u16) {
        let interrupted = {
            let cpu_regs = get_cpu_regs!(emu, CPU);
            let interrupt = cpu_regs.ime != 0 && (cpu_regs.ie & cpu_regs.irf) != 0 && !Cpsr::from(get_regs!(emu, CPU).cpsr).irq_disable();
            if interrupt {
                debug_println!("{:?} interrupt {:?}", cpu_regs.cpu_type, InterruptFlags(cpu_regs.ie & cpu_regs.irf));
            }
            interrupt
        };
        if interrupted {
            exception_handler::handle::<CPU, false>(emu, 0, ExceptionVector::NormalInterrupt);
            get_cpu_regs_mut!(emu, CPU).unhalt(0);
        }
    }
}
