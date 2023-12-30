use crate::hle::CpuType;
use crate::utils::FastCell;
use std::sync::atomic::{AtomicU8, Ordering};

enum InterruptFlags {
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

struct Regs {
    cpu_type: CpuType,
    ime: u8,
    ie: u32,
    irf: u32,
    post_flg: u8,
}

impl Regs {
    fn new(cpu_type: CpuType) -> Self {
        Regs {
            cpu_type,
            ime: 0,
            ie: 0,
            irf: 0,
            post_flg: 0,
        }
    }

    fn set_ime(&mut self, value: u8) {
        self.ime = value & 0x1;

        if self.ime == 1 && (self.ie & self.irf) != 0 {
            todo!()
        }
    }

    fn set_ie(&mut self, mut mask: u32, value: u32) {
        mask &= match self.cpu_type {
            CpuType::ARM9 => 0x003F3F7F,
            CpuType::ARM7 => 0x01FF3FFF,
        };
        self.ie = (self.ie & !mask) & (value & mask);

        if self.ime == 1 && (self.ie & self.irf) != 0 {
            todo!()
        }
    }

    fn set_irf(&mut self, mask: u32, value: u32) {
        self.irf &= !(value & mask);
    }

    fn set_post_flg(&mut self, value: u8) {
        self.post_flg |= value & 0x1;
        if self.cpu_type == CpuType::ARM9 {
            self.post_flg = (self.post_flg & !0x2) | (value & 0x2);
        }
    }
}

pub struct CpuRegs {
    inner: FastCell<Regs>,
    halt: AtomicU8,
}

impl CpuRegs {
    pub fn new(cpu_type: CpuType) -> Self {
        CpuRegs {
            inner: FastCell::new(Regs::new(cpu_type)),
            halt: AtomicU8::new(0),
        }
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

    pub fn unhalt(&self, bit: u8) {
        self.halt.fetch_and(!(1 << bit), Ordering::Relaxed);
    }

    pub fn is_halted(&self) -> bool {
        self.halt.load(Ordering::Relaxed) != 0
    }
}
