use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::{memory, CpuType};
use crate::jit::MemoryAmount;
use crate::logging::debug_println;
use crate::scheduler::IO_SCHEDULER;
use bilge::prelude::*;
use std::mem;
use std::sync::{Arc, RwLock};

const CHANNEL_COUNT: usize = 4;

#[repr(u8)]
enum DmaAddrCtrl {
    Increment = 0,
    Decrement = 1,
    Fixed = 2,
    ReloadProhibited = 3,
}

impl From<u8> for DmaAddrCtrl {
    fn from(value: u8) -> Self {
        assert!(value <= DmaAddrCtrl::ReloadProhibited as u8);
        unsafe { mem::transmute(value) }
    }
}

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

#[derive(Copy, Clone, Eq, PartialEq)]
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

impl DmaTransferMode {
    fn from_cnt(cpu_type: CpuType, cnt: u32, channel_num: u8) -> Self {
        match cpu_type {
            CpuType::ARM9 => DmaTransferMode::from(u8::from(DmaCntArm9::from(cnt).transfer_mode())),
            CpuType::ARM7 => {
                let mode = u8::from(DmaCntArm7::from(cnt).transfer_mode());
                match mode {
                    2 => DmaTransferMode::DsCartSlot,
                    3 => DmaTransferMode::from(
                        DmaTransferMode::WirelessInterrupt as u8 - (channel_num % 2) * 2,
                    ),
                    _ => DmaTransferMode::from(mode),
                }
            }
        }
    }
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
    mem_handler: Option<Arc<RwLock<MemHandler>>>,
}

impl Dma {
    pub fn new(cpu_type: CpuType) -> Self {
        Dma {
            cpu_type,
            channels: [DmaChannel::default(); CHANNEL_COUNT],
            mem_handler: None,
        }
    }

    pub fn get_cnt(&self, channel_num: u8) -> u32 {
        self.channels[channel_num as usize].cnt
    }

    pub fn set_mem_handler(&mut self, mem_handler: Arc<RwLock<MemHandler>>) {
        self.mem_handler = Some(mem_handler)
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

        let transfer_type = DmaTransferMode::from_cnt(self.cpu_type, channel.cnt, channel_num);

        if transfer_type == DmaTransferMode::GeometryCmdFifo {
            todo!()
        }

        if !was_enabled && bool::from(DmaCntArm9::from(channel.cnt).enable()) {
            let dma_transfer = DmaScheduledTransfer::new(
                self.cpu_type,
                *channel,
                channel_num,
                self.mem_handler.clone().unwrap(),
            );
            IO_SCHEDULER.schedule(move || {
                dma_transfer.transfer();
            });
        }
    }
}

struct DmaScheduledTransfer {
    cpu_type: CpuType,
    channel: DmaChannel,
    channel_num: u8,
    mem_handler: Arc<RwLock<MemHandler>>,
}

impl DmaScheduledTransfer {
    fn new(
        cpu_type: CpuType,
        channel: DmaChannel,
        channel_num: u8,
        mem_handler: Arc<RwLock<MemHandler>>,
    ) -> Self {
        DmaScheduledTransfer {
            cpu_type,
            channel,
            channel_num,
            mem_handler,
        }
    }

    fn do_transfer<T: memory::Convert>(
        cpu_type: CpuType,
        dest_addr_ctrl: DmaAddrCtrl,
        src_addr_ctrl: DmaAddrCtrl,
        mut dest_addr: u32,
        mut src_addr: u32,
        count: u32,
        mode: DmaTransferMode,
        mem_handler: Arc<RwLock<MemHandler>>,
    ) {
        let step_size = mem::size_of::<T>() as u32;
        for _ in 0..count {
            debug_println!(
                "{:?} dma transfer from {:x} to {:x}",
                cpu_type,
                src_addr,
                dest_addr
            );

            let mut mem_handler = mem_handler.write().unwrap();
            let src = mem_handler.read::<T>(src_addr);
            mem_handler.write(dest_addr, src);

            match src_addr_ctrl {
                DmaAddrCtrl::Increment => src_addr += step_size,
                DmaAddrCtrl::Decrement => src_addr -= step_size,
                _ => {}
            }

            match dest_addr_ctrl {
                DmaAddrCtrl::Increment | DmaAddrCtrl::ReloadProhibited => dest_addr += step_size,
                DmaAddrCtrl::Decrement => dest_addr -= step_size,
                DmaAddrCtrl::Fixed => {}
            }

            if mode == DmaTransferMode::GeometryCmdFifo {
                todo!()
            }
        }
    }

    fn transfer(&self) {
        let cnt = DmaCntArm9::from(self.channel.cnt);
        let dest_addr_ctrl = DmaAddrCtrl::from(u8::from(cnt.dest_addr_ctrl()));
        let src_addr_ctrl = DmaAddrCtrl::from(u8::from(cnt.src_addr_ctrl()));
        let mode = DmaTransferMode::from_cnt(self.cpu_type, self.channel.cnt, self.channel_num);

        let amount =
            if bool::from(cnt.transfer_type()) {
                MemoryAmount::WORD
            } else {
                MemoryAmount::HALF
            };

        let count = u32::from(cnt.word_count());

        let dest_addr = self.channel.dad;
        let src_addr = self.channel.sad;

        match amount {
            MemoryAmount::HALF => DmaScheduledTransfer::do_transfer::<u16>(
                self.cpu_type,
                dest_addr_ctrl,
                src_addr_ctrl,
                dest_addr,
                src_addr,
                count,
                mode,
                self.mem_handler.clone(),
            ),
            MemoryAmount::WORD => DmaScheduledTransfer::do_transfer::<u32>(
                self.cpu_type,
                dest_addr_ctrl,
                src_addr_ctrl,
                dest_addr,
                src_addr,
                count,
                mode,
                self.mem_handler.clone(),
            ),
            _ => {}
        }

        if mode == DmaTransferMode::GeometryCmdFifo {
            todo!()
        }

        if bool::from(cnt.repeat()) && mode != DmaTransferMode::StartImm {
            todo!()
        } else {
            self.mem_handler
                .write()
                .unwrap()
                .io_ports
                .dma
                .borrow_mut()
                .channels[self.channel_num as usize]
                .cnt &= !(1 << 31);
        }

        if bool::from(cnt.irq_at_end()) {
            todo!()
        }
    }
}
