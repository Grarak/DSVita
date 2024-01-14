use crate::hle::bios_context::BiosContext;
use crate::hle::cp15_context::Cp15Context;
use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::exception_handler::ExceptionVector;
use crate::hle::{exception_handler, CpuType};
use crate::utils::FastCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

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

#[derive(Default)]
struct CpuRegsInner<const CPU: CpuType> {
    ime: u8,
    ie: u32,
    irf: u32,
    post_flg: u8,
    cpsr_irq_enabled: bool,
    bios_context: Option<Rc<FastCell<BiosContext<CPU>>>>,
    cp15_context: Option<Rc<FastCell<Cp15Context>>>,
}

impl<const CPU: CpuType> CpuRegsInner<CPU> {
    fn new() -> Self {
        CpuRegsInner::default()
    }

    fn set_ime(&mut self, value: u8) {
        self.ime = value & 0x1;

        if self.ime != 0 && (self.ie & self.irf) != 0 {
            todo!()
        }
    }

    fn set_ie(&mut self, mut mask: u32, value: u32) {
        mask &= match CPU {
            CpuType::ARM9 => 0x003F3F7F,
            CpuType::ARM7 => 0x01FF3FFF,
        };
        self.ie = (self.ie & !mask) | (value & mask);

        if self.ime != 0 && (self.ie & self.irf) != 0 {
            todo!()
        }
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
    inner: Rc<FastCell<CpuRegsInner<CPU>>>,
    halt: Arc<AtomicU8>,
    cycle_manager: Arc<CycleManager>,
}

impl<const CPU: CpuType> CpuRegs<CPU> {
    pub fn new(cycle_manager: Arc<CycleManager>) -> Self {
        CpuRegs {
            inner: Rc::new(FastCell::new(CpuRegsInner::new())),
            halt: Arc::new(AtomicU8::new(0)),
            cycle_manager,
        }
    }

    pub fn set_bios_context(&self, bios_context: Rc<FastCell<BiosContext<CPU>>>) {
        self.inner.borrow_mut().bios_context = Some(bios_context);
    }

    pub fn set_cp15_context(&self, cp15_context: Rc<FastCell<Cp15Context>>) {
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

    pub fn set_ime(&self, value: u8) {
        self.inner.borrow_mut().set_ime(value);
    }

    pub fn set_ie(&self, mask: u32, value: u32) {
        self.inner.borrow_mut().set_ie(mask, value);
    }

    pub fn set_irf(&self, mask: u32, value: u32) {
        self.inner.borrow_mut().set_irf(mask, value);
    }

    pub fn set_post_flg(&self, value: u8) {
        self.inner.borrow_mut().set_post_flg(value);
    }

    pub fn halt(&self, bit: u8) {
        self.halt.fetch_or(1 << bit, Ordering::Relaxed);
    }

    pub fn is_halted(&self) -> bool {
        self.halt.load(Ordering::Relaxed) != 0
    }

    pub fn set_cpsr_irq_enabled(&self, enabled: bool) {
        self.inner.borrow_mut().cpsr_irq_enabled = enabled;
    }

    pub fn send_interrupt(&self, flag: InterruptFlag) {
        let mut inner = self.inner.borrow_mut();
        inner.irf |= 1 << flag as u8;
        if (inner.ie & inner.irf) != 0 {
            if inner.ime != 0 && inner.cpsr_irq_enabled {
                self.cycle_manager.schedule::<CPU>(
                    1,
                    Box::new(InterruptEvent::new(self.inner.clone(), self.halt.clone())),
                );
            } else if CPU == CpuType::ARM7 || inner.ime != 0 {
                self.halt.fetch_and(!1, Ordering::Relaxed);
            }
        }
    }
}

struct InterruptEvent<const CPU: CpuType> {
    inner: Rc<FastCell<CpuRegsInner<CPU>>>,
    halt: Arc<AtomicU8>,
}

impl<const CPU: CpuType> InterruptEvent<CPU> {
    fn new(inner: Rc<FastCell<CpuRegsInner<CPU>>>, halt: Arc<AtomicU8>) -> Self {
        InterruptEvent { inner, halt }
    }
}

impl<const CPU: CpuType> CycleEvent for InterruptEvent<CPU> {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, _: u16) {
        let inner = self.inner.borrow();
        if inner.ime != 0 && (inner.ie & inner.irf) != 0 && inner.cpsr_irq_enabled {
            let bios_context = inner.bios_context.clone().unwrap();
            let mut bios_context = bios_context.borrow_mut();
            let cp15_context = inner.cp15_context.clone().unwrap();
            let cp15_context = cp15_context.borrow_mut();
            drop(inner);
            exception_handler::handle::<CPU, false>(
                Some(cp15_context.deref()),
                bios_context.deref_mut(),
                0,
                ExceptionVector::NormalInterrupt,
            );
            self.halt.fetch_and(!1, Ordering::Relaxed);
        }
    }
}
