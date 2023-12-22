use crate::hle::CpuType;
use bilge::prelude::*;
use std::mem;

const CHANNEL_COUNT: usize = 4;

#[bitsize(32)]
#[derive(FromBits)]
struct DmaCntArm9 {
    word_count: u21,
    dest_addr_ctrl: u2,
    src_addr_ctrl: u2,
    repeat: u1,
    transfer_type: u1,
    transfer_mode: u3,
    irq_at_end: u1,
    enable: u1,
}

#[bitsize(32)]
#[derive(FromBits)]
struct DmaCntArm7 {
    word_count: u16,
    unused: u5,
    dest_addr_ctrl: u2,
    src_addr_ctrl: u2,
    repeat: u1,
    transfer_type: u1,
    unused1: u1,
    transfer_mode: u2,
    irq_at_end: u1,
    enable: u1,
}

#[derive(Eq, PartialEq)]
#[repr(u8)]
enum DmaTransferMode {
    StartImm = 0,
    StartAtVBlank = 1,
    StartAtHBlank = 2,
    SyncToStartDisplay = 3,
    MainMemDisplay = 4,
    DsCartSlot = 5,
    GbaCartSlot = 6,
    GeometryCmdFifo = 7,
    WirelessInterrupt = 8,
}

impl From<u8> for DmaTransferMode {
    fn from(value: u8) -> Self {
        assert!(value <= DmaTransferMode::WirelessInterrupt as u8);
        unsafe { mem::transmute(value) }
    }
}

#[derive(Copy, Clone, Default)]
struct DmaChannel {
    cnt: u32,
    sad: u32,
    dad: u32,
}

pub struct Dma {
    cpu_type: CpuType,
    channels: [DmaChannel; CHANNEL_COUNT],
}

impl Dma {
    pub fn new(cpu_type: CpuType) -> Self {
        Dma {
            cpu_type,
            channels: [DmaChannel::default(); CHANNEL_COUNT],
        }
    }

    pub fn set_sad(&mut self, channel_num: u8, value: u32) {
        let addr_mask =
            ((self.cpu_type == CpuType::ARM9 || channel_num != 0) as u32 * 0x8000000) | 0x07FFFFFF;
        let channel = &mut self.channels[channel_num as usize];
        channel.sad = (channel.sad & !addr_mask) | (value & addr_mask);
    }

    pub fn set_dad(&mut self, channel_num: u8, value: u32) {
        let addr_mask =
            ((self.cpu_type == CpuType::ARM9 || channel_num != 0) as u32 * 0x8000000) | 0x07FFFFFF;
        let channel = &mut self.channels[channel_num as usize];
        channel.dad = (channel.dad & !addr_mask) | (value & addr_mask);
    }

    pub fn set_cnt(&mut self, channel_num: u8, value: u32) {
        let channel = &mut self.channels[channel_num as usize];

        let was_enabled = bool::from(DmaCntArm9::from(channel.cnt).enable());

        let mask = match self.cpu_type {
            CpuType::ARM9 => 0xFFFFFFFF,
            CpuType::ARM7 => ((channel_num == 3) as u32 * 0xC000) | 0xF7E03FFF,
        };

        channel.cnt = (channel.cnt & !mask) | value & mask;

        let transfer_type = match self.cpu_type {
            CpuType::ARM9 => {
                DmaTransferMode::from(u8::from(DmaCntArm9::from(channel.cnt).transfer_mode()))
            }
            CpuType::ARM7 => {
                let mode = u8::from(DmaCntArm7::from(channel.cnt).transfer_mode());
                match mode {
                    2 => DmaTransferMode::DsCartSlot,
                    3 => DmaTransferMode::from(
                        DmaTransferMode::WirelessInterrupt as u8 - (channel_num % 2) * 2,
                    ),
                    _ => DmaTransferMode::from(mode),
                }
            }
        };

        if transfer_type == DmaTransferMode::GeometryCmdFifo {
            todo!()
        }

        if !was_enabled && bool::from(DmaCntArm9::from(channel.cnt).enable()) {
            todo!()
        }
    }
}
