use crate::core::cpu_regs::InterruptFlag;
use crate::core::cycle_manager::EventType;
use crate::core::emu::Emu;
use crate::core::CpuType;
use bilge::prelude::*;

const CHANNEL_COUNT: usize = 4;
const TIME_OVERFLOW: u32 = 0x10000;

#[bitsize(16)]
#[derive(FromBits)]
struct TimerCntH {
    prescaler: u2,
    count_up: bool,
    not_used: u3,
    irq_enable: bool,
    start: bool,
    not_used1: u8,
}

impl TimerCntH {
    fn is_count_up(&self, channel_num: usize) -> bool {
        channel_num != 0 && self.count_up()
    }
}

#[derive(Copy, Clone, Default)]
pub struct TimerChannel {
    cnt_l: u16,
    cnt_h: u16,
    current_value: u16,
    current_shift: u8,
    pub scheduled_cycle: u32,
}

pub struct Timers {
    pub channels: [TimerChannel; CHANNEL_COUNT],
}

impl Timers {
    pub fn new() -> Self {
        Timers {
            channels: [TimerChannel::default(); CHANNEL_COUNT],
        }
    }
}

impl Emu {
    pub fn timers_get_cnt_l(&mut self, cpu: CpuType, channel_num: usize) -> u16 {
        let timers = &mut self.timers[cpu];
        let channel = &mut timers.channels[channel_num];
        let cnt = TimerCntH::from(channel.cnt_h);
        if cnt.start() && !cnt.is_count_up(channel_num) {
            let current_cycle_count = self.cm.get_cycles();
            let diff = channel.scheduled_cycle.wrapping_sub(current_cycle_count);
            channel.current_value = (TIME_OVERFLOW - (diff >> channel.current_shift)) as u16;
        }
        channel.current_value
    }

    pub fn timers_get_cnt_h(&self, cpu: CpuType, channel_num: usize) -> u16 {
        self.timers[cpu].channels[channel_num].cnt_h
    }

    pub fn timers_set_cnt_l(&mut self, cpu: CpuType, channel_num: usize, mask: u16, value: u16) {
        let timers = &mut self.timers[cpu];
        timers.channels[channel_num].cnt_l = (timers.channels[channel_num].cnt_l & !mask) | (value & mask);
    }

    pub fn timers_set_cnt_h(&mut self, cpu: CpuType, channel_num: usize, mut mask: u16, value: u16) {
        let timers = &mut self.timers[cpu];
        let channel = &mut timers.channels[channel_num];
        let current_cnt = TimerCntH::from(channel.cnt_h);

        mask &= 0xC7;
        channel.cnt_h = (channel.cnt_h & !mask) | (value & mask);
        let cnt = TimerCntH::from(channel.cnt_h);

        let mut update = if !current_cnt.start() && cnt.start() {
            channel.current_value = channel.cnt_l;
            true
        } else {
            false
        };

        if (mask & 0xFF) != 0 {
            let shift = if u8::from(cnt.prescaler()) == 0 || cnt.is_count_up(channel_num) {
                0
            } else {
                4 + (u8::from(cnt.prescaler()) << 1)
            };
            if channel.current_shift != shift {
                channel.current_shift = shift;
                update = true;
            }
        }

        if update && cnt.start() && !cnt.is_count_up(channel_num) {
            let remaining_cycles = (TIME_OVERFLOW - channel.current_value as u32) << channel.current_shift;
            channel.scheduled_cycle = self.cm.get_cycles() + remaining_cycles;
            self.cm.schedule(remaining_cycles, EventType::timer(cpu, channel_num as u8))
        }
    }

    fn timers_overflow(&mut self, channel_num: usize, cpu: CpuType) {
        {
            let channel = &mut self.timers[cpu].channels[channel_num];
            let cnt = TimerCntH::from(channel.cnt_h);
            if !cnt.start() {
                return;
            }
            channel.current_value = channel.cnt_l;
            if !cnt.is_count_up(channel_num) {
                let remaining_cycles = (TIME_OVERFLOW - channel.current_value as u32) << channel.current_shift;
                channel.scheduled_cycle = self.cm.get_cycles() + remaining_cycles;
                self.cm.schedule(remaining_cycles, EventType::timer(cpu, channel_num as u8))
            }

            if cnt.irq_enable() {
                self.cpu_send_interrupt(cpu, InterruptFlag::from(InterruptFlag::Timer0Overflow as u8 + channel_num as u8));
            }
        }
        if channel_num < 3 {
            let mut overflow = false;
            {
                let channel = &mut self.timers[cpu].channels[channel_num];
                let cnt = TimerCntH::from(channel.cnt_h);
                if cnt.count_up() {
                    channel.current_value += 1;
                    overflow = channel.current_value == 0;
                }
            }
            if overflow {
                self.timers_overflow(channel_num + 1, cpu);
            }
        }
    }

    pub fn timers_on_overflow_event<const CPU: CpuType, const CHANNEL_NUM: u8>(&mut self) {
        self.timers_overflow(CHANNEL_NUM as usize, CPU);
    }
}
