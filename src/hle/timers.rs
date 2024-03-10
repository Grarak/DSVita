use crate::hle::cpu_regs::InterruptFlag;
use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::hle::{get_cm, get_cpu_regs_mut, io_timers, io_timers_mut, Hle};
use crate::hle::CpuType;
use crate::hle::CpuType::{ARM7, ARM9};
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
    scheduled_cycle: u64,
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
            let current_cycle_count = cycle_manager.get_cycle_count();
            let diff = if channel.scheduled_cycle > current_cycle_count {
                channel.scheduled_cycle - current_cycle_count
            } else {
                0
            } as u32;
            channel.current_value = (TIME_OVERFLOW - (diff >> channel.current_shift)) as u16;
        }
        channel.current_value
    }

    pub fn get_cnt_h<const CHANNEL_NUM: usize>(&self) -> u16 {
        self.channels[CHANNEL_NUM].cnt_h
    }

    pub fn set_cnt_l<const CHANNEL_NUM: usize>(&mut self, mask: u16, value: u16) {
        self.channels[CHANNEL_NUM].cnt_l =
            (self.channels[CHANNEL_NUM].cnt_l & !mask) | (value & mask);
    }

    pub fn set_cnt_h<const CHANNEL_NUM: usize>(
        &mut self,
        mut mask: u16,
        value: u16,
        hle: &mut Hle,
    ) {
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
            let remaining_cycles =
                (TIME_OVERFLOW - channel.current_value as u32) << channel.current_shift;
            channel.scheduled_cycle = match self.cpu_type {
                ARM9 => get_cm!(hle).schedule(
                    remaining_cycles,
                    Box::new(TimersEvent::<{ ARM9 }, CHANNEL_NUM>::new()),
                ),
                ARM7 => get_cm!(hle).schedule(
                    remaining_cycles,
                    Box::new(TimersEvent::<{ ARM7 }, CHANNEL_NUM>::new()),
                ),
            };
        }
    }
}

#[derive(Clone)]
struct TimersEvent<const CPU: CpuType, const CHANNEL_NUM: usize> {
    scheduled_at: u64,
}

impl<const CPU: CpuType, const CHANNEL_NUM: usize> TimersEvent<CPU, CHANNEL_NUM> {
    fn new() -> Self {
        TimersEvent { scheduled_at: 0 }
    }

    fn overflow(hle: &mut Hle, count_up_num: usize) {
        {
            let channel = &mut io_timers_mut!(hle, CPU).channels[count_up_num];
            let cnt = TimerCntH::from(channel.cnt_h);
            if !bool::from(cnt.start()) {
                return;
            }
            channel.current_value = channel.cnt_l;
            if !cnt.is_count_up(count_up_num) {
                let remaining_cycles =
                    (TIME_OVERFLOW - channel.current_value as u32) << channel.current_shift;
                channel.scheduled_cycle = get_cm!(hle).schedule(
                    remaining_cycles,
                    Box::new(TimersEvent::<CPU, CHANNEL_NUM>::new()),
                )
            }

            if bool::from(cnt.irq_enable()) {
                get_cpu_regs_mut!(hle, CPU).send_interrupt(
                    InterruptFlag::from(InterruptFlag::Timer0Overflow as u8 + count_up_num as u8),
                    get_cm!(hle),
                );
            }
        }
        if count_up_num < 3 {
            let mut overflow = false;
            {
                let channel = &mut io_timers_mut!(hle, CPU).channels[count_up_num];
                let cnt = TimerCntH::from(channel.cnt_h);
                if bool::from(cnt.count_up()) {
                    channel.current_value += 1;
                    overflow = channel.current_value == 0;
                }
            }
            if overflow {
                Self::overflow(hle, count_up_num + 1);
            }
        }
    }
}

impl<const CPU: CpuType, const CHANNEL_NUM: usize> CycleEvent for TimersEvent<CPU, CHANNEL_NUM> {
    fn scheduled(&mut self, timestamp: &u64) {
        self.scheduled_at = *timestamp;
    }

    fn trigger(&mut self, _: u16, hle: &mut Hle) {
        if self.scheduled_at == io_timers!(hle, CPU).channels[CHANNEL_NUM].scheduled_cycle {
            Self::overflow(hle, CHANNEL_NUM);
        }
    }
}
