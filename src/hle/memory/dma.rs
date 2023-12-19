use crate::hle::CpuType;
use bilge::prelude::*;

const CHANNEL_COUNT: usize = 4;

#[bitsize(32)]
#[derive(FromBits)]
struct DmaCntArm9 {
    word_count: u21,
    dest_addr_ctrl: u2,
    src_addr_ctrl: u2,
    repeat: u1,
    transfer_type_size: u1,
    transfer_type: u3,
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
    transfer_type_size: u1,
    unused1: u1,
    transfer_type: u2,
    irq_at_end: u1,
    enable: u1,
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

    pub fn set_sad(&mut self, channel: u8, value: u32) {
        let addr_mask =
            ((self.cpu_type == CpuType::ARM9 || channel != 0) as u32 * 0x8000000) | 0x07FFFFFF;
        let channel = &mut self.channels[channel as usize];
        channel.sad = (channel.sad & !addr_mask) | (value & addr_mask);
    }

    pub fn set_dad(&mut self, channel: u8, value: u32) {
        let addr_mask =
            ((self.cpu_type == CpuType::ARM9 || channel != 0) as u32 * 0x8000000) | 0x07FFFFFF;
        let channel = &mut self.channels[channel as usize];
        channel.dad = (channel.dad & !addr_mask) | (value & addr_mask);
    }

    pub fn set_cnt(&mut self, channel: u8, value: u32) {
        todo!()
    }
}
