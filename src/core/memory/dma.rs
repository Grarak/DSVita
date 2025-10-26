use crate::core::cpu_regs::InterruptFlag;
use crate::core::cycle_manager::ImmEventType;
use crate::core::emu::Emu;
use crate::core::CpuType;
use crate::logging::debug_println;
use crate::utils;
use bilge::prelude::*;
use std::cmp::min;
use std::hint::assert_unchecked;
use std::{mem, slice};
use CpuType::{ARM7, ARM9};

const CHANNEL_COUNT: usize = 4;

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum DmaAddrCtrl {
    Increment = 0,
    Decrement = 1,
    Fixed = 2,
    IncrementReload = 3,
}

impl From<u8> for DmaAddrCtrl {
    fn from(value: u8) -> Self {
        debug_assert!(value <= DmaAddrCtrl::IncrementReload as u8);
        unsafe { mem::transmute(value) }
    }
}

#[bitsize(32)]
#[derive(FromBits)]
struct DmaCntArm9 {
    word_count: u21,
    dest_addr_ctrl: u2,
    src_addr_ctrl: u2,
    repeat: bool,
    transfer_type: bool,
    transfer_mode: u3,
    irq_at_end: bool,
    enable: bool,
}

#[bitsize(32)]
#[derive(FromBits)]
struct DmaCntArm7 {
    word_count: u16,
    not_used: u5,
    dest_addr_ctrl: u2,
    src_addr_ctrl: u2,
    repeat: bool,
    transfer_type: bool,
    not_used1: u1,
    transfer_mode: u2,
    irq_at_end: bool,
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
                    3 => DmaTransferMode::from(DmaTransferMode::WirelessInterrupt as u8 - ((channel_num as u8 & 1) << 1)),
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
    channels: [DmaChannel; CHANNEL_COUNT],
    src_buf: Vec<u8>,
}

impl Dma {
    pub fn new() -> Self {
        Dma {
            channels: [DmaChannel::default(); CHANNEL_COUNT],
            src_buf: Vec::new(),
        }
    }
}

impl Emu {
    pub fn dma_get_sad(&self, cpu: CpuType, channel_num: usize) -> u32 {
        self.dma[cpu].channels[channel_num].sad
    }

    pub fn dma_get_dad(&self, cpu: CpuType, channel_num: usize) -> u32 {
        self.dma[cpu].channels[channel_num].dad
    }

    pub fn dma_get_cnt(&self, cpu: CpuType, channel_num: usize) -> u32 {
        self.dma[cpu].channels[channel_num].cnt
    }

    pub fn dma_get_fill(&self, cpu: CpuType, channel_num: usize) -> u32 {
        self.dma[cpu].channels[channel_num].fill
    }

    pub fn dma_set_sad(&mut self, cpu: CpuType, channel_num: usize, mut mask: u32, value: u32) {
        mask &= if cpu == ARM9 || channel_num != 0 { 0x0FFFFFFF } else { 0x07FFFFFF };
        self.dma[cpu].channels[channel_num].sad = (self.dma[cpu].channels[channel_num].sad & !mask) | (value & mask);
    }

    pub fn dma_set_dad(&mut self, cpu: CpuType, channel_num: usize, mut mask: u32, value: u32) {
        mask &= if cpu == ARM9 || channel_num != 0 { 0x0FFFFFFF } else { 0x07FFFFFF };
        self.dma[cpu].channels[channel_num].dad = (self.dma[cpu].channels[channel_num].dad & !mask) | (value & mask);
    }

    pub fn dma_set_cnt(&mut self, cpu: CpuType, channel_num: usize, mut mask: u32, value: u32) {
        let dma = &mut self.dma[cpu];

        let channel = &mut dma.channels[channel_num];
        let was_enabled = DmaCntArm9::from(channel.cnt).enable();

        mask &= match cpu {
            ARM9 => 0xFFFFFFFF,
            ARM7 => {
                if channel_num == 3 {
                    0xF7E0FFFF
                } else {
                    0xF7E03FFF
                }
            }
        };

        channel.cnt = (channel.cnt & !mask) | value & mask;

        let transfer_type = DmaTransferMode::from_cnt(cpu, channel.cnt, channel_num);

        let dma_cnt = DmaCntArm9::from(channel.cnt);
        if transfer_type == DmaTransferMode::GeometryCmdFifo && dma_cnt.enable() && !self.gpu.gpu_3d_regs.is_cmd_fifo_half_full() {
            debug_println!(
                "{cpu:?} dma schedule imm {:x} {:x} {:x} {:x}",
                channel.cnt,
                channel.current_dest,
                channel.current_src,
                channel.current_count
            );

            self.breakout_imm = true;

            self.cm.schedule_imm(ImmEventType::dma(cpu, channel_num as u8));
        }

        if !was_enabled && dma_cnt.enable() {
            channel.current_src = channel.sad;
            channel.current_dest = channel.dad;
            channel.current_count = u32::from(dma_cnt.word_count());

            if transfer_type == DmaTransferMode::StartImm {
                debug_println!(
                    "{cpu:?} dma schedule imm {:x} {:x} {:x} {:x}",
                    channel.cnt,
                    channel.current_dest,
                    channel.current_src,
                    channel.current_count
                );

                self.breakout_imm = true;

                self.cm.schedule_imm(ImmEventType::dma(cpu, channel_num as u8));
            }
        }
    }

    pub fn dma_set_fill(&mut self, cpu: CpuType, channel_num: usize, mask: u32, value: u32) {
        self.dma[cpu].channels[channel_num].fill = (self.dma[cpu].channels[channel_num].fill & !mask) | (value & mask);
    }

    #[inline(never)]
    pub fn dma_trigger_all(&mut self, cpu: CpuType, mode: DmaTransferMode) {
        self.dma_trigger(cpu, mode, 0xF);
    }

    pub fn dma_trigger(&mut self, cpu: CpuType, mode: DmaTransferMode, channels: u8) {
        for (index, channel) in self.dma[cpu].channels.iter().enumerate() {
            if channels & (1 << index) != 0 && DmaCntArm9::from(channel.cnt).enable() && DmaTransferMode::from_cnt(cpu, channel.cnt, index) == mode {
                debug_println!(
                    "{cpu:?} dma trigger {:?} {:x} {:x} {:x} {:x}",
                    mode,
                    channel.cnt,
                    channel.current_dest,
                    channel.current_src,
                    channel.current_count
                );
                self.cm.schedule_imm(ImmEventType::dma(cpu, index as u8));
            }
        }
    }

    pub fn dma_trigger_imm(&mut self, cpu: CpuType, mode: DmaTransferMode, channels: u8) {
        for i in 0..CHANNEL_COUNT {
            let channel = &self.dma[cpu].channels[i];
            if channels & (1 << i) != 0 && DmaCntArm9::from(channel.cnt).enable() && DmaTransferMode::from_cnt(cpu, channel.cnt, i) == mode {
                match cpu {
                    ARM9 => self.dma_on_event::<{ ARM9 }>(i as u16),
                    ARM7 => self.dma_on_event::<{ ARM7 }>(i as u16),
                }
            }
        }
    }

    pub fn dma_is_scheduled(&self, cpu: CpuType, mode: DmaTransferMode, channels: u8) -> bool {
        for (index, channel) in self.dma[cpu].channels.iter().enumerate() {
            if channels & (1 << index) != 0 && DmaCntArm9::from(channel.cnt).enable() && DmaTransferMode::from_cnt(cpu, channel.cnt, index) == mode {
                return true;
            }
        }
        false
    }

    fn dma_do_transfer<const CPU: CpuType, T: utils::Convert>(&mut self, dest_addr: &mut u32, src_addr: &mut u32, count: u32, cnt: &DmaCntArm9, mode: DmaTransferMode) {
        let dest_addr_ctrl = DmaAddrCtrl::from(u8::from(cnt.dest_addr_ctrl()));
        let src_addr_ctrl = DmaAddrCtrl::from(u8::from(cnt.src_addr_ctrl()));

        let step_size = size_of::<T>() as u32;
        let count = if mode == DmaTransferMode::GeometryCmdFifo { min(112, count) } else { count };
        debug_println!("{CPU:?} dma transfer {mode:?} from {src_addr:x} {src_addr_ctrl:?} to {dest_addr:x} {dest_addr_ctrl:?} with size {count}");

        let dma = &mut self.dma[CPU];
        let total_size = count << (step_size >> 1);
        if dma.src_buf.len() < total_size as usize {
            dma.src_buf.reserve(total_size as usize - dma.src_buf.len());
            unsafe { dma.src_buf.set_len(total_size as usize) };
        }

        match (src_addr_ctrl, dest_addr_ctrl) {
            (DmaAddrCtrl::Increment, DmaAddrCtrl::Fixed) => {
                let mut slice = unsafe { slice::from_raw_parts_mut(dma.src_buf.as_mut_ptr() as *mut T, count as usize) };
                let aligned_addr = *src_addr & !(size_of::<T>() as u32 - 1);
                let aligned_addr = aligned_addr & 0x0FFFFFFF;
                let shm_offset = self.get_shm_offset::<CPU, false, false>(aligned_addr);
                if shm_offset != 0 {
                    slice = unsafe { slice::from_raw_parts_mut(self.mem.shm.as_ptr().add(shm_offset) as *mut T, count as usize) };
                } else {
                    self.mem_read_multiple_slice::<CPU, false, false, T>(aligned_addr, slice);
                }
                if *dest_addr >= 0x4000400 && *dest_addr < 0x4000440 {
                    let slice = unsafe { slice::from_raw_parts(slice.as_ptr() as *const u32, (total_size as usize) >> 2) };
                    self.regs_3d_set_gx_fifo_multiple(slice);
                } else {
                    self.mem_write_fixed_slice::<CPU, false, T>(*dest_addr, slice);
                }
                *src_addr += total_size;
            }
            (DmaAddrCtrl::Increment, DmaAddrCtrl::Increment | DmaAddrCtrl::IncrementReload) => {
                let mut slice = unsafe { slice::from_raw_parts_mut(dma.src_buf.as_mut_ptr() as *mut T, count as usize) };
                let aligned_addr = *src_addr & !(size_of::<T>() as u32 - 1);
                let aligned_addr = aligned_addr & 0x0FFFFFFF;
                let shm_offset = self.get_shm_offset::<CPU, false, false>(aligned_addr);
                if shm_offset != 0 {
                    slice = unsafe { slice::from_raw_parts_mut(self.mem.shm.as_ptr().add(shm_offset) as *mut T, count as usize) };
                } else {
                    self.mem_read_multiple_slice::<CPU, false, false, T>(aligned_addr, slice);
                }
                self.mem_write_multiple_slice::<CPU, false, T>(*dest_addr, slice);
                *src_addr += total_size;
                *dest_addr += total_size;
            }
            (DmaAddrCtrl::Fixed, DmaAddrCtrl::Increment | DmaAddrCtrl::IncrementReload) => {
                let slice = unsafe { slice::from_raw_parts_mut(dma.src_buf.as_mut_ptr() as *mut T, count as usize) };
                self.mem_read_fixed_slice::<CPU, false, T>(*src_addr, slice);
                self.mem_write_multiple_slice::<CPU, false, T>(*dest_addr, slice);
                *dest_addr += total_size;
            }
            _ => {
                for _ in 0..count {
                    let src = self.mem_read_no_tcm::<CPU, T>(*src_addr);
                    self.mem_write_no_tcm::<CPU, T>(*dest_addr, src);

                    match src_addr_ctrl {
                        DmaAddrCtrl::Increment => *src_addr += step_size,
                        DmaAddrCtrl::Decrement => *src_addr -= step_size,
                        _ => {}
                    }

                    match dest_addr_ctrl {
                        DmaAddrCtrl::Increment | DmaAddrCtrl::IncrementReload => *dest_addr += step_size,
                        DmaAddrCtrl::Decrement => *dest_addr -= step_size,
                        DmaAddrCtrl::Fixed => {}
                    }
                }
            }
        }
    }

    pub fn dma_on_event0<const CPU: CpuType>(&mut self) {
        self.dma_on_event::<CPU>(0);
    }

    pub fn dma_on_event1<const CPU: CpuType>(&mut self) {
        self.dma_on_event::<CPU>(1);
    }

    pub fn dma_on_event2<const CPU: CpuType>(&mut self) {
        self.dma_on_event::<CPU>(2);
    }

    pub fn dma_on_event3<const CPU: CpuType>(&mut self) {
        self.dma_on_event::<CPU>(3);
    }

    fn dma_on_event<const CPU: CpuType>(&mut self, channel_num: u16) {
        let channel_num = channel_num as usize;
        unsafe { assert_unchecked(channel_num < CHANNEL_COUNT) };

        let channel = &mut self.dma[CPU].channels[channel_num];

        let (cnt, mode, mut dest, mut src, count) = {
            (
                DmaCntArm9::from(channel.cnt),
                DmaTransferMode::from_cnt(CPU, channel.cnt, channel_num),
                channel.current_dest,
                channel.current_src,
                channel.current_count,
            )
        };

        if cnt.transfer_type() {
            self.dma_do_transfer::<CPU, u32>(&mut dest, &mut src, count, &cnt, mode)
        } else {
            self.dma_do_transfer::<CPU, u16>(&mut dest, &mut src, count, &cnt, mode)
        };

        let channel = &mut self.dma[CPU].channels[channel_num];
        channel.current_dest = dest;
        channel.current_src = src;

        if mode == DmaTransferMode::GeometryCmdFifo && count > 112 {
            channel.current_count -= 112;
            if !self.gpu.gpu_3d_regs.is_cmd_fifo_half_full() {
                self.cm.schedule_imm(ImmEventType::dma(CPU, channel_num as u8));
            }
            return;
        }

        if cnt.repeat() && mode != DmaTransferMode::StartImm {
            channel.current_count = u32::from(DmaCntArm9::from(channel.cnt).word_count());
            if DmaAddrCtrl::from(u8::from(cnt.dest_addr_ctrl())) == DmaAddrCtrl::IncrementReload {
                channel.current_dest = channel.dad;
            }

            if mode == DmaTransferMode::GeometryCmdFifo && !self.gpu.gpu_3d_regs.is_cmd_fifo_half_full() {
                self.cm.schedule_imm(ImmEventType::dma(CPU, channel_num as u8));
            }
        } else {
            channel.cnt &= !(1 << 31);
        }

        if cnt.irq_at_end() {
            self.cpu_send_interrupt(CPU, InterruptFlag::from(InterruptFlag::Dma0 as u8 + channel_num as u8));
        }
    }
}
