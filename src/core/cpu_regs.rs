use crate::core::cycle_manager::EventType;
use crate::core::emu::Emu;
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
    pub bios_wait_flags: u32,
}

impl CpuRegs {
    pub fn new() -> Self {
        CpuRegs {
            ime: 0,
            ie: 0,
            irf: 0,
            post_flg: 0,
            halt_cnt: 0,
            halt: 0,
            bios_wait_flags: 0,
        }
    }
}

impl Emu {
    pub fn cpu_set_ime(&mut self, cpu: CpuType, value: u8) {
        self.cpu[cpu].ime = value & 0x1;
        self.cpu_check_for_interrupt(cpu);
    }

    pub fn cpu_set_ie(&mut self, cpu: CpuType, mut mask: u32, value: u32) {
        let CpuRegs { ie, .. } = &mut self.cpu[cpu];
        mask &= match cpu {
            ARM9 => 0x003F3F7F,
            ARM7 => 0x01FF3FFF,
        };
        *ie = (*ie & !mask) | (value & mask);
        debug_println!("{cpu:?} set ie {ie:x} {:?}", InterruptFlags(*ie));
        self.cpu_check_for_interrupt(cpu);
    }

    pub fn cpu_check_for_interrupt(&mut self, cpu: CpuType) {
        let cpu_regs = &self.cpu[cpu];
        if cpu_regs.ime != 0 && (cpu_regs.ie & cpu_regs.irf) != 0 && !Cpsr::from(cpu.thread_regs().cpsr).irq_disable() {
            self.cpu_schedule_interrupt(cpu);
        }
    }

    fn cpu_schedule_interrupt(&mut self, cpu: CpuType) {
        self.cm.schedule_imm(
            match cpu {
                ARM9 => EventType::CpuInterruptArm9,
                ARM7 => EventType::CpuInterruptArm7,
            },
            0,
        )
    }

    pub fn cpu_set_irf(&mut self, cpu: CpuType, mask: u32, value: u32) {
        // debug_println!("{:?} set irf {:?}", self.cpu_type, InterruptFlags(value & mask));
        self.cpu[cpu].irf &= !(value & mask);
    }

    pub fn cpu_set_post_flg(&mut self, cpu: CpuType, value: u8) {
        let cpu_regs = &mut self.cpu[cpu];
        cpu_regs.post_flg |= value & 0x1;
        if cpu == ARM9 {
            cpu_regs.post_flg = (cpu_regs.post_flg & !0x2) | (value & 0x2);
        }
    }

    pub fn cpu_halt(&mut self, cpu: CpuType, bit: u8) {
        debug_println!("{cpu:?} halt with bit {bit}");
        self.cpu[cpu].halt |= 1 << bit;
    }

    pub fn cpu_unhalt(&mut self, cpu: CpuType, bit: u8) {
        debug_println!("{cpu:?} unhalt with bit {bit}");
        self.cpu[cpu].halt &= !(1 << bit);
    }

    pub fn cpu_is_halted(&self, cpu: CpuType) -> bool {
        self.cpu[cpu].halt != 0
    }

    #[inline(never)]
    pub fn cpu_send_interrupt(&mut self, cpu: CpuType, flag: InterruptFlag) {
        let cpu_regs = &mut self.cpu[cpu];
        cpu_regs.irf |= 1 << flag as u8;
        debug_println!(
            "{cpu:?} send interrupt {flag:?} {:?} {:?} {:x} {}",
            InterruptFlags(cpu_regs.ie),
            InterruptFlags(cpu_regs.irf),
            cpu_regs.ime,
            !Cpsr::from(cpu.thread_regs().cpsr).irq_disable()
        );
        if (cpu_regs.ie & cpu_regs.irf) != 0 {
            if cpu_regs.ime != 0 && !Cpsr::from(cpu.thread_regs().cpsr).irq_disable() {
                debug_println!("{cpu:?} schedule send interrupt {flag:?}");
                self.cpu_schedule_interrupt(cpu);
            } else if cpu == ARM7 || cpu_regs.ime != 0 {
                debug_println!("{cpu:?} unhalt send interrupt {flag:?}");
                self.cpu_unhalt(cpu, 0);
            }
        }
    }

    pub fn cpu_set_halt_cnt(&mut self, cpu: CpuType, value: u8) {
        self.cpu[cpu].halt_cnt = value & 0xC0;

        match self.cpu[cpu].halt_cnt {
            1 => todo!("gba mode"),
            2 => todo!("halt"),
            _ => {}
        }
    }

    pub fn cpu_on_interrupt_event<const CPU: CpuType>(&mut self, _: u16) {
        let cpu_regs = &self.cpu[CPU];
        let interrupted = {
            let interrupt = cpu_regs.ime != 0 && (cpu_regs.ie & cpu_regs.irf) != 0 && !Cpsr::from(CPU.thread_regs().cpsr).irq_disable();
            if interrupt {
                debug_println!("{CPU:?} interrupt {:?}", InterruptFlags(cpu_regs.ie & cpu_regs.irf));
            } else {
                debug_println!(
                    "{CPU:?} can't interrupt {:x} {:?} {}",
                    cpu_regs.ime,
                    InterruptFlags(cpu_regs.ie & cpu_regs.irf),
                    !Cpsr::from(CPU.thread_regs().cpsr).irq_disable()
                );
            }
            interrupt
        };
        if interrupted {
            exception_handler::handle::<CPU>(self, 0, ExceptionVector::NormalInterrupt);
            self.cpu_unhalt(CPU, 0);
        }
    }
}
