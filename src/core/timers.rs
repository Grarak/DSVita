use crate::core::cpu_regs::InterruptFlag;
use crate::core::cycle_manager::{CycleManager, EventType};
use crate::core::emu::{get_cm_mut, get_cpu_regs_mut, io_timers, io_timers_mut, Emu};
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use bilge::prelude::*;

const CHANNEL_COUNT: usize = 4;
const TIME_OVERFLOW: u32 = 0x10000;

#[bitsize(16)]
#[derive(FromBits)]
struct TimerCntH {
    prescaler: u2,
    count_up: u1,
    not_used: u3,
    irq_enable: u1,
    start: u1,
    not_used1: u8,
}

impl TimerCntH {
    fn is_count_up(&self, channel_num: usize) -> bool {
        channel_num != 0 && bool::from(self.count_up())
    }
}

#[derive(Copy, Clone, Default)]
struct TimerChannel {
    cnt_l: u16,
    cnt_h: u16,
    current_value: u16,
    current_shift: u8,
    id: u16,
    scheduled_cycle: u64,
}

impl TimerChannel {
    fn increment_id(&mut self) {
        self.id += 1;
        self.id &= 0x3FF;
    }
}

pub struct Timers {
    cpu_type: CpuType,
    channels: [TimerChannel; CHANNEL_COUNT],
}

impl Timers {
    pub fn new(cpu_type: CpuType) -> Self {
        Timers {
            cpu_type,
            channels: [TimerChannel::default(); CHANNEL_COUNT],
        }
    }

    pub fn get_cnt_l<const CHANNEL_NUM: usize>(&mut self, cycle_manager: &CycleManager) -> u16 {
        let channel = &mut self.channels[CHANNEL_NUM];
        let cnt = TimerCntH::from(channel.cnt_h);
        if bool::from(cnt.start()) && !cnt.is_count_up(CHANNEL_NUM) {
            let current_cycle_count = cycle_manager.get_cycles();
            let diff = channel.scheduled_cycle.wrapping_sub(current_cycle_count);
            channel.current_value = (TIME_OVERFLOW - (diff >> channel.current_shift) as u32) as u16;
        }
        channel.current_value
    }

    pub fn get_cnt_h<const CHANNEL_NUM: usize>(&self) -> u16 {
        self.channels[CHANNEL_NUM].cnt_h
    }

    pub fn set_cnt_l<const CHANNEL_NUM: usize>(&mut self, mask: u16, value: u16) {
        self.channels[CHANNEL_NUM].cnt_l = (self.channels[CHANNEL_NUM].cnt_l & !mask) | (value & mask);
    }

    pub fn set_cnt_h<const CHANNEL_NUM: usize>(&mut self, mut mask: u16, value: u16, emu: &mut Emu) {
        let channel = &mut self.channels[CHANNEL_NUM];
        let current_cnt = TimerCntH::from(channel.cnt_h);

        mask &= 0xC7;
        channel.cnt_h = (channel.cnt_h & !mask) | (value & mask);
        let cnt = TimerCntH::from(channel.cnt_h);

        let mut update = if !bool::from(current_cnt.start()) && bool::from(cnt.start()) {
            channel.current_value = channel.cnt_l;
            true
        } else {
            false
        };

        if (mask & 0xFF) != 0 {
            let shift = if u8::from(cnt.prescaler()) == 0 || cnt.is_count_up(CHANNEL_NUM) {
                0
            } else {
                4 + (u8::from(cnt.prescaler()) << 1)
            };
            if channel.current_shift != shift {
                channel.current_shift = shift;
                update = true;
            }
        }

        if update && bool::from(cnt.start()) && !cnt.is_count_up(CHANNEL_NUM) {
            let remaining_cycles = (TIME_OVERFLOW - channel.current_value as u32) << channel.current_shift;
            let cm = get_cm_mut!(emu);
            channel.scheduled_cycle = cm.get_cycles() + remaining_cycles as u64;
            channel.increment_id();
            cm.schedule(
                remaining_cycles,
                match self.cpu_type {
                    ARM9 => EventType::TimerArm9,
                    ARM7 => EventType::TimerArm7,
                },
                (channel.id << 2) | CHANNEL_NUM as u16,
            )
        }
    }

    fn overflow<const CPU: CpuType>(channel_num: usize, emu: &mut Emu) {
        {
            let channel = &mut io_timers_mut!(emu, CPU).channels[channel_num];
            let cnt = TimerCntH::from(channel.cnt_h);
            if !bool::from(cnt.start()) {
                return;
            }
            channel.current_value = channel.cnt_l;
            if !cnt.is_count_up(channel_num) {
                let remaining_cycles = (TIME_OVERFLOW - channel.current_value as u32) << channel.current_shift;
                let cm = get_cm_mut!(emu);
                channel.scheduled_cycle = cm.get_cycles() + remaining_cycles as u64;
                channel.increment_id();
                cm.schedule(
                    remaining_cycles,
                    match CPU {
                        ARM9 => EventType::TimerArm9,
                        ARM7 => EventType::TimerArm7,
                    },
                    (channel.id << 2) | channel_num as u16,
                )
            }

            if bool::from(cnt.irq_enable()) {
                get_cpu_regs_mut!(emu, CPU).send_interrupt(InterruptFlag::from(InterruptFlag::Timer0Overflow as u8 + channel_num as u8), emu);
            }
        }
        if channel_num < 3 {
            let mut overflow = false;
            {
                let channel = &mut io_timers_mut!(emu, CPU).channels[channel_num];
                let cnt = TimerCntH::from(channel.cnt_h);
                if bool::from(cnt.count_up()) {
                    channel.current_value += 1;
                    overflow = channel.current_value == 0;
                }
            }
            if overflow {
                Self::overflow::<CPU>(channel_num + 1, emu);
            }
        }
    }

    pub fn on_overflow_event<const CPU: CpuType>(_: &mut CycleManager, emu: &mut Emu, id_channel_num: u16) {
        let channel_num = id_channel_num & 0x3;
        let id = id_channel_num >> 2;
        if id == io_timers!(emu, CPU).channels[channel_num as usize].id {
            Self::overflow::<CPU>(channel_num as usize, emu);
        }
    }
}
