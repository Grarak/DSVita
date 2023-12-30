use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::scheduler::IO_SCHEDULER;
use crate::utils;
use crate::utils::FastCell;
use bilge::prelude::*;
use std::mem;
use std::sync::{mpsc, Arc};

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
        debug_assert!(value <= DmaAddrCtrl::ReloadProhibited as u8);
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
    not_used: u5,
    dest_addr_ctrl: u2,
    src_addr_ctrl: u2,
    repeat: u1,
    transfer_type: u1,
    not_used1: u1,
    transfer_mode: u2,
    irq_at_end: u1,
    enable: u1,
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum DmaTransferMode {
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
    fn from_cnt<const CPU: CpuType>(cnt: u32, channel_num: usize) -> Self {
        match CPU {
            CpuType::ARM9 => DmaTransferMode::from(u8::from(DmaCntArm9::from(cnt).transfer_mode())),
            CpuType::ARM7 => {
                let mode = u8::from(DmaCntArm7::from(cnt).transfer_mode());
                match mode {
                    2 => DmaTransferMode::DsCartSlot,
                    3 => DmaTransferMode::from(
                        DmaTransferMode::WirelessInterrupt as u8 - (channel_num as u8 % 2) * 2,
                    ),
                    _ => DmaTransferMode::from(mode),
                }
            }
        }
    }
}

impl From<u8> for DmaTransferMode {
    fn from(value: u8) -> Self {
        debug_assert!(value <= DmaTransferMode::WirelessInterrupt as u8);
        unsafe { mem::transmute(value) }
    }
}

#[derive(Copy, Clone, Default)]
struct DmaChannel {
    cnt: u32,
    sad: u32,
    dad: u32,
    fill: u32,
    current_src: u32,
    current_dest: u32,
    current_count: u32,
}

pub struct Dma<const CPU: CpuType> {
    channels: [Arc<FastCell<DmaChannel>>; CHANNEL_COUNT],
    mem_handler: Option<Arc<MemHandler<CPU>>>,
    cycle_manager: Arc<CycleManager>,
}

impl<const CPU: CpuType> Dma<CPU> {
    pub fn new(cycle_manager: Arc<CycleManager>) -> Self {
        Dma {
            channels: [
                Arc::new(FastCell::new(DmaChannel::default())),
                Arc::new(FastCell::new(DmaChannel::default())),
                Arc::new(FastCell::new(DmaChannel::default())),
                Arc::new(FastCell::new(DmaChannel::default())),
            ],
            mem_handler: None,
            cycle_manager,
        }
    }

    pub fn get_cnt(&self, channel_num: usize) -> u32 {
        self.channels[channel_num].borrow().cnt
    }

    pub fn get_fill(&self, channel_num: usize) -> u32 {
        self.channels[channel_num].borrow().fill
    }

    pub fn set_mem_handler(&mut self, mem_handler: Arc<MemHandler<CPU>>) {
        self.mem_handler = Some(mem_handler)
    }

    pub fn set_sad(&mut self, channel_num: usize, mut mask: u32, value: u32) {
        mask &= ((CPU == CpuType::ARM9 || channel_num != 0) as u32 * 0x8000000) | 0x07FFFFFF;
        let mut channel = self.channels[channel_num].borrow_mut();
        channel.sad = (channel.sad & !mask) | (value & mask);
    }

    pub fn set_dad(&mut self, channel_num: usize, mut mask: u32, value: u32) {
        mask &= ((CPU == CpuType::ARM9 || channel_num != 0) as u32 * 0x8000000) | 0x07FFFFFF;
        let mut channel = self.channels[channel_num].borrow_mut();
        channel.dad = (channel.dad & !mask) | (value & mask);
    }

    pub fn set_cnt(&mut self, channel_num: usize, mut mask: u32, value: u32) {
        let mut channel = self.channels[channel_num].borrow_mut();
        let was_enabled = bool::from(DmaCntArm9::from(channel.cnt).enable());

        mask &= match CPU {
            CpuType::ARM9 => 0xFFFFFFFF,
            CpuType::ARM7 => ((channel_num == 3) as u32 * 0xC000) | 0xF7E03FFF,
        };

        channel.cnt = (channel.cnt & !mask) | value & mask;

        let transfer_type = DmaTransferMode::from_cnt::<CPU>(channel.cnt, channel_num);

        if transfer_type == DmaTransferMode::GeometryCmdFifo {
            todo!()
        }

        let dma_cnt = DmaCntArm9::from(channel.cnt);
        if !was_enabled && bool::from(dma_cnt.enable()) {
            channel.current_src = channel.sad;
            channel.current_dest = channel.dad;
            channel.current_count = u32::from(dma_cnt.word_count());

            if transfer_type == DmaTransferMode::StartImm {
                self.cycle_manager.schedule::<CPU>(
                    1,
                    Box::new(DmaEvent::new(
                        self.channels[channel_num].clone(),
                        channel_num,
                        self.mem_handler.clone().unwrap(),
                    )),
                );
            }
        }
    }

    pub fn set_fill(&mut self, channel_num: usize, mask: u32, value: u32) {
        let mut channel = self.channels[channel_num].borrow_mut();
        channel.fill = (channel.fill & !mask) | (value & mask);
    }
}

struct DmaEvent<const CPU: CpuType> {
    channel: Arc<FastCell<DmaChannel>>,
    channel_num: usize,
    mem_handler: Arc<MemHandler<CPU>>,
}

impl<const CPU: CpuType> DmaEvent<CPU> {
    fn new(
        channel: Arc<FastCell<DmaChannel>>,
        channel_num: usize,
        mem_handler: Arc<MemHandler<CPU>>,
    ) -> Self {
        DmaEvent {
            channel,
            channel_num,
            mem_handler,
        }
    }

    fn do_transfer<T: utils::Convert>(
        mem_handler: Arc<MemHandler<CPU>>,
        mut dest_addr: u32,
        mut src_addr: u32,
        count: u32,
        cnt: &DmaCntArm9,
        mode: DmaTransferMode,
    ) {
        let dest_addr_ctrl = DmaAddrCtrl::from(u8::from(cnt.dest_addr_ctrl()));
        let src_addr_ctrl = DmaAddrCtrl::from(u8::from(cnt.src_addr_ctrl()));

        let step_size = mem::size_of::<T>() as u32;
        for _ in 0..count {
            debug_println!(
                "{:?} dma transfer from {:x} to {:x}",
                CPU,
                src_addr,
                dest_addr
            );

            let src = mem_handler.read_lock::<false, T>(src_addr);
            mem_handler.write_lock::<false, T>(dest_addr, src);

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
}

struct DmaChannelWrapper(Arc<FastCell<DmaChannel>>);

unsafe impl Send for DmaChannelWrapper {}

impl<const CPU: CpuType> CycleEvent for DmaEvent<CPU> {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, _: u16) {
        let mem_handler = self.mem_handler.clone();
        let channel_num = self.channel_num;

        let channel = DmaChannelWrapper(self.channel.clone());
        let (tx, rc) = mpsc::channel::<()>();
        IO_SCHEDULER.schedule(move || {
            let _lock = mem_handler.dma_transfer_lock.write().unwrap();
            tx.send(()).unwrap();

            let channel = channel;
            let (cnt, mode, dest, src, count) = {
                let channel = channel.0.borrow();
                (
                    DmaCntArm9::from(channel.cnt),
                    DmaTransferMode::from_cnt::<CPU>(channel.cnt, channel_num),
                    channel.current_dest,
                    channel.current_src,
                    channel.current_count,
                )
            };

            if bool::from(cnt.transfer_type()) {
                Self::do_transfer::<u32>(mem_handler.clone(), dest, src, count, &cnt, mode);
            } else {
                Self::do_transfer::<u16>(mem_handler.clone(), dest, src, count, &cnt, mode);
            }

            if mode == DmaTransferMode::GeometryCmdFifo {
                todo!()
            }

            if bool::from(cnt.repeat()) && mode != DmaTransferMode::StartImm {
                todo!()
            } else {
                channel.0.borrow_mut().cnt &= !(1 << 31);
            }

            if bool::from(cnt.irq_at_end()) {
                todo!()
            }
        });
        rc.recv().unwrap();
    }
}
