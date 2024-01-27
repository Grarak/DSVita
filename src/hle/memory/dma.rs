use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::memory::mem_handler::MemHandler;
use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::utils;
use bilge::prelude::*;
use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

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
                        DmaTransferMode::WirelessInterrupt as u8 - ((channel_num as u8 & 1) << 1),
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
    channels: [Rc<RefCell<DmaChannel>>; CHANNEL_COUNT],
    mem_handler: Option<Rc<MemHandler<CPU>>>,
    cycle_manager: Rc<CycleManager>,
}

impl<const CPU: CpuType> Dma<CPU> {
    pub fn new(cycle_manager: Rc<CycleManager>) -> Self {
        Dma {
            channels: [
                Rc::new(RefCell::new(DmaChannel::default())),
                Rc::new(RefCell::new(DmaChannel::default())),
                Rc::new(RefCell::new(DmaChannel::default())),
                Rc::new(RefCell::new(DmaChannel::default())),
            ],
            mem_handler: None,
            cycle_manager,
        }
    }

    pub fn get_cnt<const CHANNEL_NUM: usize>(&self) -> u32 {
        self.channels[CHANNEL_NUM].borrow().cnt
    }

    pub fn get_fill<const CHANNEL_NUM: usize>(&self) -> u32 {
        self.channels[CHANNEL_NUM].borrow().fill
    }

    pub fn set_mem_handler(&mut self, mem_handler: Rc<MemHandler<CPU>>) {
        self.mem_handler = Some(mem_handler)
    }

    pub fn set_sad<const CHANNEL_NUM: usize>(&mut self, mut mask: u32, value: u32) {
        mask &= if CPU == CpuType::ARM9 || CHANNEL_NUM != 0 {
            0x0FFFFFFF
        } else {
            0x07FFFFFF
        };
        let mut channel = self.channels[CHANNEL_NUM].borrow_mut();
        channel.sad = (channel.sad & !mask) | (value & mask);
    }

    pub fn set_dad<const CHANNEL_NUM: usize>(&mut self, mut mask: u32, value: u32) {
        mask &= if CPU == CpuType::ARM9 || CHANNEL_NUM != 0 {
            0x0FFFFFFF
        } else {
            0x07FFFFFF
        };
        let mut channel = self.channels[CHANNEL_NUM].borrow_mut();
        channel.dad = (channel.dad & !mask) | (value & mask);
    }

    pub fn set_cnt<const CHANNEL_NUM: usize>(&mut self, mut mask: u32, value: u32) {
        let mut channel = self.channels[CHANNEL_NUM].borrow_mut();
        let was_enabled = bool::from(DmaCntArm9::from(channel.cnt).enable());

        mask &= match CPU {
            CpuType::ARM9 => 0xFFFFFFFF,
            CpuType::ARM7 => {
                if CHANNEL_NUM == 3 {
                    0xF7E0FFFF
                } else {
                    0xF7E03FFF
                }
            }
        };

        channel.cnt = (channel.cnt & !mask) | value & mask;

        let transfer_type = DmaTransferMode::from_cnt::<CPU>(channel.cnt, CHANNEL_NUM);

        if transfer_type == DmaTransferMode::GeometryCmdFifo {
            todo!()
        }

        let dma_cnt = DmaCntArm9::from(channel.cnt);
        if !was_enabled && bool::from(dma_cnt.enable()) {
            channel.current_src = channel.sad;
            channel.current_dest = channel.dad;
            channel.current_count = u32::from(dma_cnt.word_count());

            if transfer_type == DmaTransferMode::StartImm {
                debug_println!(
                    "{:?} dma schedule imm {:x} {:x} {:x} {:x}",
                    CPU,
                    channel.cnt,
                    channel.current_dest,
                    channel.current_src,
                    channel.current_count
                );
                self.cycle_manager.schedule::<CPU, _>(
                    1,
                    Box::new(DmaEvent::<CPU, CHANNEL_NUM>::new(
                        self.channels[CHANNEL_NUM].clone(),
                        self.mem_handler.clone().unwrap(),
                    )),
                );
            }
        }
    }

    pub fn set_fill<const CHANNEL_NUM: usize>(&mut self, mask: u32, value: u32) {
        let mut channel = self.channels[CHANNEL_NUM].borrow_mut();
        channel.fill = (channel.fill & !mask) | (value & mask);
    }

    pub fn trigger_all(&self, mode: DmaTransferMode) {
        self.trigger(mode, 0xF);
    }

    pub fn trigger(&self, mode: DmaTransferMode, channels: u8) {
        for (index, channel) in self.channels.iter().enumerate() {
            if channels & (1 << index) != 0 {
                let channel = channel.borrow();
                if bool::from(DmaCntArm9::from(channel.cnt).enable())
                    && DmaTransferMode::from_cnt::<CPU>(channel.cnt, index) == mode
                {
                    todo!()
                }
            }
        }
    }
}

struct DmaEvent<const CPU: CpuType, const CHANNEL_NUM: usize> {
    channel: Rc<RefCell<DmaChannel>>,
    mem_handler: Rc<MemHandler<CPU>>,
}

impl<const CPU: CpuType, const CHANNEL_NUM: usize> DmaEvent<CPU, CHANNEL_NUM> {
    fn new(channel: Rc<RefCell<DmaChannel>>, mem_handler: Rc<MemHandler<CPU>>) -> Self {
        DmaEvent {
            channel,
            mem_handler,
        }
    }

    fn do_transfer<T: utils::Convert>(
        mem_handler: Rc<MemHandler<CPU>>,
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
}

impl<const CPU: CpuType, const CHANNEL_NUM: usize> CycleEvent for DmaEvent<CPU, CHANNEL_NUM> {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, _: u16) {
        let (cnt, mode, dest, src, count) = {
            let channel = self.channel.borrow();
            (
                DmaCntArm9::from(channel.cnt),
                DmaTransferMode::from_cnt::<CPU>(channel.cnt, CHANNEL_NUM),
                channel.current_dest,
                channel.current_src,
                channel.current_count,
            )
        };

        if bool::from(cnt.transfer_type()) {
            Self::do_transfer::<u32>(self.mem_handler.clone(), dest, src, count, &cnt, mode);
        } else {
            Self::do_transfer::<u16>(self.mem_handler.clone(), dest, src, count, &cnt, mode);
        }

        if mode == DmaTransferMode::GeometryCmdFifo {
            todo!()
        }

        if bool::from(cnt.repeat()) && mode != DmaTransferMode::StartImm {
            todo!()
        } else {
            self.channel.borrow_mut().cnt &= !(1 << 31);
        }

        if bool::from(cnt.irq_at_end()) {
            todo!()
        }
    }
}
