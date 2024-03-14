use crate::hle::cpu_regs::InterruptFlag;
use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::hle::{get_cm, get_cpu_regs_mut, io_dma, io_dma_mut, Hle};
use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::utils;
use bilge::prelude::*;
use std::mem;
use CpuType::{ARM7, ARM9};

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
    fn from_cnt(cpu_type: CpuType, cnt: u32, channel_num: usize) -> Self {
        match cpu_type {
            ARM9 => DmaTransferMode::from(u8::from(DmaCntArm9::from(cnt).transfer_mode())),
            ARM7 => {
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

    pub fn get_sad<const CHANNEL_NUM: usize>(&self) -> u32 {
        self.channels[CHANNEL_NUM].sad
    }

    pub fn get_dad<const CHANNEL_NUM: usize>(&self) -> u32 {
        self.channels[CHANNEL_NUM].dad
    }

    pub fn get_cnt<const CHANNEL_NUM: usize>(&self) -> u32 {
        self.channels[CHANNEL_NUM].cnt
    }

    pub fn get_fill<const CHANNEL_NUM: usize>(&self) -> u32 {
        self.channels[CHANNEL_NUM].fill
    }

    pub fn set_sad<const CHANNEL_NUM: usize>(&mut self, mut mask: u32, value: u32) {
        mask &= if self.cpu_type == ARM9 || CHANNEL_NUM != 0 {
            0x0FFFFFFF
        } else {
            0x07FFFFFF
        };
        self.channels[CHANNEL_NUM].sad = (self.channels[CHANNEL_NUM].sad & !mask) | (value & mask);
    }

    pub fn set_dad<const CHANNEL_NUM: usize>(&mut self, mut mask: u32, value: u32) {
        mask &= if self.cpu_type == ARM9 || CHANNEL_NUM != 0 {
            0x0FFFFFFF
        } else {
            0x07FFFFFF
        };
        self.channels[CHANNEL_NUM].dad = (self.channels[CHANNEL_NUM].dad & !mask) | (value & mask);
    }

    pub fn set_cnt<const CHANNEL_NUM: usize>(&mut self, mut mask: u32, value: u32, hle: &mut Hle) {
        let channel = &mut self.channels[CHANNEL_NUM];
        let was_enabled = bool::from(DmaCntArm9::from(channel.cnt).enable());

        mask &= match self.cpu_type {
            ARM9 => 0xFFFFFFFF,
            ARM7 => {
                if CHANNEL_NUM == 3 {
                    0xF7E0FFFF
                } else {
                    0xF7E03FFF
                }
            }
        };

        channel.cnt = (channel.cnt & !mask) | value & mask;

        let transfer_type = DmaTransferMode::from_cnt(self.cpu_type, channel.cnt, CHANNEL_NUM);

        if transfer_type == DmaTransferMode::GeometryCmdFifo {
            // TODO 3d
        }

        let dma_cnt = DmaCntArm9::from(channel.cnt);
        if !was_enabled && bool::from(dma_cnt.enable()) {
            channel.current_src = channel.sad;
            channel.current_dest = channel.dad;
            channel.current_count = u32::from(dma_cnt.word_count());

            if transfer_type == DmaTransferMode::StartImm {
                debug_println!(
                    "{:?} dma schedule imm {:x} {:x} {:x} {:x}",
                    self.cpu_type,
                    channel.cnt,
                    channel.current_dest,
                    channel.current_src,
                    channel.current_count
                );

                get_cm!(hle).schedule(1, Box::new(DmaEvent::new(self.cpu_type, CHANNEL_NUM)));
            }
        }
    }

    pub fn set_fill<const CHANNEL_NUM: usize>(&mut self, mask: u32, value: u32) {
        self.channels[CHANNEL_NUM].fill =
            (self.channels[CHANNEL_NUM].fill & !mask) | (value & mask);
    }

    pub fn trigger_all(&self, mode: DmaTransferMode, cycle_manager: &CycleManager) {
        self.trigger(mode, 0xF, cycle_manager);
    }

    pub fn trigger(&self, mode: DmaTransferMode, channels: u8, cycle_manager: &CycleManager) {
        for (index, channel) in self.channels.iter().enumerate() {
            if channels & (1 << index) != 0
                && bool::from(DmaCntArm9::from(channel.cnt).enable())
                && DmaTransferMode::from_cnt(self.cpu_type, channel.cnt, index) == mode
            {
                cycle_manager.schedule(1, Box::new(DmaEvent::new(self.cpu_type, index)));
            }
        }
    }
}

struct DmaEvent {
    cpu_type: CpuType,
    channel_num: usize,
}

impl DmaEvent {
    fn new(cpu_type: CpuType, channel_num: usize) -> Self {
        DmaEvent {
            cpu_type,
            channel_num,
        }
    }

    fn do_transfer<const CPU: CpuType, T: utils::Convert>(
        hle: &mut Hle,
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

            let src = hle.mem_read_no_tcm::<CPU, T>(src_addr);
            hle.mem_write_no_tcm::<CPU, T>(dest_addr, src);

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

impl CycleEvent for DmaEvent {
    fn scheduled(&mut self, _: &u64) {}

    fn trigger(&mut self, _: u16, hle: &mut Hle) {
        let (cnt, mode, dest, src, count) = {
            let channel = &io_dma!(hle, self.cpu_type).channels[self.channel_num];
            (
                DmaCntArm9::from(channel.cnt),
                DmaTransferMode::from_cnt(self.cpu_type, channel.cnt, self.channel_num),
                channel.current_dest,
                channel.current_src,
                channel.current_count,
            )
        };

        if bool::from(cnt.transfer_type()) {
            match self.cpu_type {
                ARM9 => Self::do_transfer::<{ ARM9 }, u32>(hle, dest, src, count, &cnt, mode),
                ARM7 => Self::do_transfer::<{ ARM7 }, u32>(hle, dest, src, count, &cnt, mode),
            }
        } else {
            match self.cpu_type {
                ARM9 => Self::do_transfer::<{ ARM9 }, u16>(hle, dest, src, count, &cnt, mode),
                ARM7 => Self::do_transfer::<{ ARM7 }, u16>(hle, dest, src, count, &cnt, mode),
            }
        };

        if mode == DmaTransferMode::GeometryCmdFifo {
            todo!()
        }

        if !bool::from(cnt.repeat()) || mode == DmaTransferMode::StartImm {
            io_dma_mut!(hle, self.cpu_type).channels[self.channel_num].cnt &= !(1 << 31);
        } else if mode == DmaTransferMode::GeometryCmdFifo {
            todo!()
        }

        if bool::from(cnt.irq_at_end()) {
            get_cpu_regs_mut!(hle, self.cpu_type).send_interrupt(
                InterruptFlag::from(InterruptFlag::Dma0 as u8 + self.channel_num as u8),
                get_cm!(hle),
            );
        }
    }
}
