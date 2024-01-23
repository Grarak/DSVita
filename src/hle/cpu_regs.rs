use crate::hle::bios_context::BiosContext;
use crate::hle::cp15_context::Cp15Context;
use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::exception_handler::ExceptionVector;
use crate::hle::{exception_handler, CpuType};
use crate::logging::debug_println;
use crate::utils::FastCell;
use std::ops::DerefMut;
use std::rc::Rc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, RwLock};

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
    inner: Arc<RwLock<CpuRegsInner<CPU>>>,
    halt: Arc<AtomicU8>,
    cycle_manager: Arc<CycleManager>,
    interrupt_mutex: Arc<Mutex<()>>,
}

impl<const CPU: CpuType> CpuRegs<CPU> {
    pub fn new(cycle_manager: Arc<CycleManager>) -> Self {
        CpuRegs {
            inner: Arc::new(RwLock::new(CpuRegsInner::new())),
            halt: Arc::new(AtomicU8::new(0)),
            cycle_manager,
            interrupt_mutex: Arc::new(Mutex::new(())),
        }
    }

    pub fn set_bios_context(&self, bios_context: Rc<FastCell<BiosContext<CPU>>>) {
        self.inner.write().unwrap().bios_context = Some(bios_context);
    }

    pub fn set_cp15_context(&self, cp15_context: Rc<FastCell<Cp15Context>>) {
        self.inner.write().unwrap().cp15_context = Some(cp15_context);
    }

    pub fn get_ime(&self) -> u8 {
        self.inner.read().unwrap().ime
    }

    pub fn get_ie(&self) -> u32 {
        self.inner.read().unwrap().ie
    }

    pub fn get_irf(&self) -> u32 {
        self.inner.read().unwrap().irf
    }

    pub fn set_ime(&self, value: u8) {
        let mut inner = self.inner.write().unwrap();
        inner.set_ime(value);
        if inner.ime != 0 && (inner.ie & inner.irf) != 0 && inner.cpsr_irq_enabled {
            self.schedule_interrupt();
        }
    }

    pub fn set_ie(&self, mask: u32, value: u32) {
        let mut inner = self.inner.write().unwrap();
        inner.set_ie(mask, value);
        if inner.ime != 0 && (inner.ie & inner.irf) != 0 && inner.cpsr_irq_enabled {
            self.schedule_interrupt();
        }
    }

    fn schedule_interrupt(&self) {
        self.cycle_manager.schedule::<CPU, _>(
            1,
            Box::new(InterruptEvent::new(
                self.inner.clone(),
                self.halt.clone(),
                self.interrupt_mutex.clone(),
            )),
        );
    }

    pub fn set_irf(&self, mask: u32, value: u32) {
        debug_println!("{:?} set irf {:x} {:x}", CPU, mask, value);
        self.inner.write().unwrap().set_irf(mask, value);
    }

    pub fn set_post_flg(&self, value: u8) {
        self.inner.write().unwrap().set_post_flg(value);
    }

    pub fn halt(&self, bit: u8) {
        debug_println!("{:?} halt with bit {}", CPU, bit);
        self.halt.fetch_or(1 << bit, Ordering::Relaxed);
    }

    pub fn is_halted(&self) -> bool {
        self.halt.load(Ordering::Relaxed) != 0
    }

    pub fn set_cpsr_irq_enabled(&self, enabled: bool) {
        self.inner.write().unwrap().cpsr_irq_enabled = enabled;
    }

    pub fn send_interrupt(&self, flag: InterruptFlag) {
        let _guard = self.interrupt_mutex.lock().unwrap();
        let mut inner = self.inner.write().unwrap();
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
                debug_println!("{:?} schedule interrupt {:?}", CPU, flag);
                self.schedule_interrupt();
            } else if CPU == CpuType::ARM7 || inner.ime != 0 {
                self.halt.fetch_and(!1, Ordering::Relaxed);
            }
        }
    }
}

struct InterruptEvent<const CPU: CpuType> {
    inner: Arc<RwLock<CpuRegsInner<CPU>>>,
    halt: Arc<AtomicU8>,
    interrupt_mutex: Arc<Mutex<()>>,
}

impl<const CPU: CpuType> InterruptEvent<CPU> {
    fn new(
        inner: Arc<RwLock<CpuRegsInner<CPU>>>,
        halt: Arc<AtomicU8>,
        interrupt_mutex: Arc<Mutex<()>>,
    ) -> Self {
        InterruptEvent {
            inner,
            halt,
            interrupt_mutex,
        }
    }
}

impl<const CPU: CpuType> CycleEvent for InterruptEvent<CPU> {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, _: u16) {
        let guard = self.interrupt_mutex.lock().unwrap();
        let inner = self.inner.read().unwrap();
        if inner.ime != 0 && (inner.ie & inner.irf) != 0 && inner.cpsr_irq_enabled {
            let bios_context = inner.bios_context.clone().unwrap();
            let mut bios_context = bios_context.borrow_mut();
            let cp15_context = inner.cp15_context.clone().unwrap();
            let (exception_addr, dtcm_addr) = {
                let cp15_context = cp15_context.borrow();
                (cp15_context.exception_addr, cp15_context.dtcm_addr)
            };
            drop(inner);
            drop(guard);
            exception_handler::handle::<CPU, false>(
                Some(exception_addr),
                Some(dtcm_addr),
                bios_context.deref_mut(),
                0,
                ExceptionVector::NormalInterrupt,
            );
            self.halt.fetch_and(!1, Ordering::Relaxed);
        }
    }
}

pub unsafe extern "C" fn cpu_regs_halt<const CPU: CpuType>(
    cpu_regs: *const Arc<CpuRegs<CPU>>,
    bit: u8,
) {
    cpu_regs.as_ref().unwrap().halt(bit)
}
