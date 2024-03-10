use crate::hle::bios_context::BiosContext;
use crate::hle::cp15_context::Cp15Context;
use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::exception_handler::ExceptionVector;
use crate::hle::{exception_handler, CpuType};
use crate::logging::debug_println;
use std::cell::RefCell;
use std::mem;
use std::ops::DerefMut;
use std::rc::Rc;

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

#[derive(Default)]
struct CpuRegsInner<const CPU: CpuType> {
    ime: u8,
    ie: u32,
    irf: u32,
    post_flg: u8,
    halt_cnt: u8,
    cpsr_irq_enabled: bool,
    bios_context: Option<Rc<RefCell<BiosContext<CPU>>>>,
    cp15_context: Option<Rc<RefCell<Cp15Context>>>,
}

impl<const CPU: CpuType> CpuRegsInner<CPU> {
    fn new() -> Self {
        CpuRegsInner::default()
    }

    fn set_ime(&mut self, value: u8) {
        self.ime = value & 0x1;
    }

    fn set_ie(&mut self, mut mask: u32, value: u32) {
        mask &= match CPU {
            CpuType::ARM9 => 0x003F3F7F,
            CpuType::ARM7 => 0x01FF3FFF,
        };
        self.ie = (self.ie & !mask) | (value & mask);
    }

    fn set_irf(&mut self, mask: u32, value: u32) {
        self.irf &= !(value & mask);
    }

    fn set_post_flg(&mut self, value: u8) {
        self.post_flg |= value & 0x1;
        if CPU == CpuType::ARM9 {
            self.post_flg = (self.post_flg & !0x2) | (value & 0x2);
        }
    }
}

pub struct CpuRegs<const CPU: CpuType> {
    inner: Rc<RefCell<CpuRegsInner<CPU>>>,
    halt: Rc<RefCell<u8>>,
    cycle_manager: Rc<CycleManager>,
}

impl<const CPU: CpuType> CpuRegs<CPU> {
    pub fn new(cycle_manager: Rc<CycleManager>) -> Self {
        CpuRegs {
            inner: Rc::new(RefCell::new(CpuRegsInner::new())),
            halt: Rc::new(RefCell::new(0)),
            cycle_manager,
        }
    }

    pub fn set_bios_context(&self, bios_context: Rc<RefCell<BiosContext<CPU>>>) {
        self.inner.borrow_mut().bios_context = Some(bios_context);
    }

    pub fn set_cp15_context(&self, cp15_context: Rc<RefCell<Cp15Context>>) {
        self.inner.borrow_mut().cp15_context = Some(cp15_context);
    }

    pub fn get_ime(&self) -> u8 {
        self.inner.borrow().ime
    }

    pub fn get_ie(&self) -> u32 {
        self.inner.borrow().ie
    }

    pub fn get_irf(&self) -> u32 {
        self.inner.borrow().irf
    }

    pub fn get_post_flg(&self) -> u8 {
        self.inner.borrow().post_flg
    }

    pub fn get_halt_cnt(&self) -> u8 {
        self.inner.borrow().halt_cnt
    }

    pub fn set_ime(&self, value: u8) {
        self.inner.borrow_mut().set_ime(value);
        self.check_for_interrupt();
    }

    pub fn set_ie(&self, mask: u32, value: u32) {
        self.inner.borrow_mut().set_ie(mask, value);
        self.check_for_interrupt();
    }

    pub fn check_for_interrupt(&self) {
        let inner = self.inner.borrow_mut();
        if inner.ime != 0 && (inner.ie & inner.irf) != 0 && inner.cpsr_irq_enabled {
            self.schedule_interrupt();
        }
    }

    fn schedule_interrupt(&self) {
        self.cycle_manager.schedule(
            1,
            Box::new(InterruptEvent::new(self.inner.clone(), self.halt.clone())),
        );
    }

    pub fn set_irf(&self, mask: u32, value: u32) {
        debug_println!("{:?} set irf {:x} {:x}", CPU, mask, value);
        self.inner.borrow_mut().set_irf(mask, value);
    }

    pub fn set_post_flg(&self, value: u8) {
        self.inner.borrow_mut().set_post_flg(value);
    }

    pub fn halt(&self, bit: u8) {
        debug_println!("{:?} halt with bit {}", CPU, bit);
        *self.halt.borrow_mut() |= 1;
    }

    pub fn is_halted(&self) -> bool {
        unsafe { self.halt.as_ptr().read() != 0 }
    }

    pub fn set_cpsr_irq_enabled(&self, enabled: bool) {
        self.inner.borrow_mut().cpsr_irq_enabled = enabled;
    }

    pub fn send_interrupt(&self, flag: InterruptFlag) {
        let mut inner = self.inner.borrow_mut();
        inner.irf |= 1 << flag as u8;
        debug_println!(
            "{:?} send interrupt {:?} {:x} {:x} {:x} {}",
            CPU,
            flag,
            inner.ie,
            inner.irf,
            inner.ime,
            inner.cpsr_irq_enabled
        );
        if (inner.ie & inner.irf) != 0 {
            if inner.ime != 0 && inner.cpsr_irq_enabled {
                debug_println!("{:?} schedule send interrupt {:?}", CPU, flag);
                self.schedule_interrupt();
            } else if CPU == CpuType::ARM7 || inner.ime != 0 {
                debug_println!("{:?} unhalt send interrupt {:?}", CPU, flag);
                *self.halt.borrow_mut() &= !1;
            }
        }
    }

    pub fn set_halt_cnt(&self, value: u8) {
        let mut inner = self.inner.borrow_mut();
        inner.halt_cnt = value & 0xC0;

        match inner.halt_cnt {
            1 => {
                todo!("gba mode")
            }
            2 => {
                todo!("halt")
            }
            _ => {}
        }
    }
}

struct InterruptEvent<const CPU: CpuType> {
    inner: Rc<RefCell<CpuRegsInner<CPU>>>,
    halt: Rc<RefCell<u8>>,
}

impl<const CPU: CpuType> InterruptEvent<CPU> {
    fn new(inner: Rc<RefCell<CpuRegsInner<CPU>>>, halt: Rc<RefCell<u8>>) -> Self {
        InterruptEvent { inner, halt }
    }
}

impl<const CPU: CpuType> CycleEvent for InterruptEvent<CPU> {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, _: u16) {
        let inner = self.inner.borrow();
        if inner.ime != 0 && (inner.ie & inner.irf) != 0 && inner.cpsr_irq_enabled {
            debug_println!("{:?} interrupt {:x} {:x}", CPU, inner.ie, inner.irf);
            let bios_context = inner.bios_context.clone().unwrap();
            let mut bios_context = bios_context.borrow_mut();
            let cp15_context = inner.cp15_context.clone().unwrap();
            let (exception_addr, dtcm_addr) = {
                let cp15_context = cp15_context.borrow();
                (cp15_context.exception_addr, cp15_context.dtcm_addr)
            };
            drop(inner);
            exception_handler::handle::<CPU, false>(
                Some(exception_addr),
                Some(dtcm_addr),
                bios_context.deref_mut(),
                0,
                ExceptionVector::NormalInterrupt,
            );
            *self.halt.borrow_mut() &= !1;
        }
    }
}

pub unsafe extern "C" fn cpu_regs_halt<const CPU: CpuType>(cpu_regs: *const CpuRegs<CPU>, bit: u8) {
    cpu_regs.as_ref().unwrap().halt(bit)
}

pub struct CpuRegsContainer {
    cpu_regs_arm9: Rc<CpuRegs<{ CpuType::ARM9 }>>,
    cpu_regs_arm7: Rc<CpuRegs<{ CpuType::ARM7 }>>,
}

impl CpuRegsContainer {
    pub fn new(
        cpu_regs_arm9: Rc<CpuRegs<{ CpuType::ARM9 }>>,
        cpu_regs_arm7: Rc<CpuRegs<{ CpuType::ARM7 }>>,
    ) -> Self {
        CpuRegsContainer {
            cpu_regs_arm9,
            cpu_regs_arm7,
        }
    }

    pub fn send_interrupt<const CPU: CpuType>(&self, flag: InterruptFlag) {
        match CPU {
            CpuType::ARM9 => self.cpu_regs_arm9.send_interrupt(flag),
            CpuType::ARM7 => self.cpu_regs_arm7.send_interrupt(flag),
        }
    }

    pub fn send_interrupt_other<const CPU: CpuType>(&self, flag: InterruptFlag) {
        match CPU {
            CpuType::ARM9 => self.cpu_regs_arm7.send_interrupt(flag),
            CpuType::ARM7 => self.cpu_regs_arm9.send_interrupt(flag),
        }
    }
}
