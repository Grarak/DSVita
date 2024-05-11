use crate::emu::cpu_regs::InterruptFlag;
use crate::emu::cycle_manager::{CycleManager, EventType};
use crate::emu::emu::{get_cpu_regs_mut, io_dma, io_dma_mut, Emu, get_cm_mut};
use crate::emu::CpuType;
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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

    pub fn set_cnt<const CHANNEL_NUM: usize>(&mut self, mut mask: u32, value: u32, emu: &mut Emu) {
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

                get_cm_mut!(emu).schedule(
                    1,
                    match self.cpu_type {
                        ARM9 => EventType::DmaArm9(CHANNEL_NUM as u8),
                        ARM7 => EventType::DmaArm7(CHANNEL_NUM as u8),
                    },
                );
            }
        }
    }

    pub fn set_fill<const CHANNEL_NUM: usize>(&mut self, mask: u32, value: u32) {
        self.channels[CHANNEL_NUM].fill =
            (self.channels[CHANNEL_NUM].fill & !mask) | (value & mask);
    }

    pub fn trigger_all(&self, mode: DmaTransferMode, cycle_manager: &mut CycleManager) {
        self.trigger(mode, 0xF, cycle_manager);
    }

    pub fn trigger(&self, mode: DmaTransferMode, channels: u8, cycle_manager: &mut CycleManager) {
        for (index, channel) in self.channels.iter().enumerate() {
            if channels & (1 << index) != 0
                && bool::from(DmaCntArm9::from(channel.cnt).enable())
                && DmaTransferMode::from_cnt(self.cpu_type, channel.cnt, index) == mode
            {
                debug_println!(
                    "{:?} dma trigger {:?} {:x} {:x} {:x} {:x}",
                    self.cpu_type,
                    mode,
                    channel.cnt,
                    channel.current_dest,
                    channel.current_src,
                    channel.current_count
                );
                cycle_manager.schedule(
                    1,
                    match self.cpu_type {
                        ARM9 => EventType::DmaArm9(index as u8),
                        ARM7 => EventType::DmaArm7(index as u8),
                    },
                );
            }
        }
    }

    fn do_transfer<const CPU: CpuType, T: utils::Convert>(
        emu: &mut Emu,
        dest_addr: &mut u32,
        src_addr: &mut u32,
        count: u32,
        cnt: &DmaCntArm9,
        mode: DmaTransferMode,
    ) {
        let dest_addr_ctrl = DmaAddrCtrl::from(u8::from(cnt.dest_addr_ctrl()));
        let src_addr_ctrl = DmaAddrCtrl::from(u8::from(cnt.src_addr_ctrl()));

        let step_size = mem::size_of::<T>() as u32;
        for _ in 0..count {
            debug_println!(
                "{:?} dma transfer {:?} from {:x} to {:x}",
                CPU,
                mode,
                src_addr,
                dest_addr
            );

            let src = emu.mem_read_no_tcm::<CPU, T>(*src_addr);
            emu.mem_write_no_tcm::<CPU, T>(*dest_addr, src);

            match src_addr_ctrl {
                DmaAddrCtrl::Increment => *src_addr += step_size,
                DmaAddrCtrl::Decrement => *src_addr -= step_size,
                _ => {}
            }

            match dest_addr_ctrl {
                DmaAddrCtrl::Increment | DmaAddrCtrl::ReloadProhibited => *dest_addr += step_size,
                DmaAddrCtrl::Decrement => *dest_addr -= step_size,
                DmaAddrCtrl::Fixed => {}
            }

            if mode == DmaTransferMode::GeometryCmdFifo {
                todo!()
            }
        }
    }

    pub fn on_event<const CPU: CpuType>(channel_num: u8, emu: &mut Emu) {
        let channel_num = channel_num as usize;
        let (cnt, mode, mut dest, mut src, count) = {
            let channel = &io_dma!(emu, CPU).channels[channel_num];
            (
                DmaCntArm9::from(channel.cnt),
                DmaTransferMode::from_cnt(CPU, channel.cnt, channel_num),
                channel.current_dest,
                channel.current_src,
                channel.current_count,
            )
        };

        if bool::from(cnt.transfer_type()) {
            Self::do_transfer::<CPU, u32>(emu, &mut dest, &mut src, count, &cnt, mode)
        } else {
            Self::do_transfer::<CPU, u16>(emu, &mut dest, &mut src, count, &cnt, mode)
        };

        {
            let channel = &mut io_dma_mut!(emu, CPU).channels[channel_num];
            channel.current_dest = dest;
            channel.current_src = src;
        }

        if mode == DmaTransferMode::GeometryCmdFifo {
            todo!()
        }

        if !bool::from(cnt.repeat()) || mode == DmaTransferMode::StartImm {
            io_dma_mut!(emu, CPU).channels[channel_num].cnt &= !(1 << 31);
        } else if mode == DmaTransferMode::GeometryCmdFifo {
            todo!()
        }

        if bool::from(cnt.irq_at_end()) {
            get_cpu_regs_mut!(emu, CPU).send_interrupt(
                InterruptFlag::from(InterruptFlag::Dma0 as u8 + channel_num as u8),
                get_cm_mut!(emu),
            );
        }
    }
}
