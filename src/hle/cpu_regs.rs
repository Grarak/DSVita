use crate::hle::CpuType;

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

pub struct CpuRegs {
    cpu_type: CpuType,
    pub ime: u8,
    pub ie: u32,
    pub irf: u32,
    pub post_flg: u8,
}

impl CpuRegs {
    pub fn new(cpu_type: CpuType) -> Self {
        CpuRegs {
            cpu_type,
            ime: 0,
            ie: 0,
            irf: 0,
            post_flg: 0,
        }
    }

    pub fn set_ime(&mut self, value: u8) {
        self.ime = value & 0x1;

        if self.ime == 1 && (self.ie & self.irf) != 0 {
            todo!()
        }
    }

    pub fn set_ie(&mut self, mut mask: u32, value: u32) {
        mask &= match self.cpu_type {
            CpuType::ARM9 => 0x003F3F7F,
            CpuType::ARM7 => 0x01FF3FFF,
        };
        self.ie = (self.ie & !mask) & (value & mask);

        if self.ime == 1 && (self.ie & self.irf) != 0 {
            todo!()
        }
    }

    pub fn set_irf(&mut self, mask: u32, value: u32) {
        self.irf &= !(value & mask);
    }

    pub fn set_post_flg(&mut self, value: u8) {
        self.post_flg |= value & 0x1;
        if self.cpu_type == CpuType::ARM9 {
            self.post_flg = (self.post_flg & !0x2) | (value & 0x2);
        }
    }
}
