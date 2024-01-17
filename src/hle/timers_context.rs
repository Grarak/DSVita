use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::CpuType;
use crate::utils;
use crate::utils::FastCell;
use bilge::prelude::*;
use std::rc::Rc;
use std::sync::Arc;

const CHANNEL_COUNT: usize = 4;

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
    fn is_count_up(&self, channel: usize) -> bool {
        channel != 0 && bool::from(self.count_up())
    }
}

#[derive(Copy, Clone, Default)]
struct TimerChannel {
    cnt_l: i16,
    cnt_h: u16,
    current_value: i16,
    current_shift: u8,
    scheduled_cycle: u64,
}

pub struct TimersContext<const CPU: CpuType> {
    cycle_manager: Arc<CycleManager>,
    channels: [Rc<FastCell<TimerChannel>>; CHANNEL_COUNT],
}

impl<const CPU: CpuType> TimersContext<CPU> {
    pub fn new(cycle_manager: Arc<CycleManager>) -> Self {
        TimersContext {
            cycle_manager,
            channels: [
                Rc::new(FastCell::new(TimerChannel::default())),
                Rc::new(FastCell::new(TimerChannel::default())),
                Rc::new(FastCell::new(TimerChannel::default())),
                Rc::new(FastCell::new(TimerChannel::default())),
            ],
        }
    }

    pub fn get_cnt_l(&self, channel_num: usize) -> u16 {
        let mut channel = self.channels[channel_num].borrow_mut();
        let cnt = TimerCntH::from(channel.cnt_h);
        if bool::from(cnt.start()) && cnt.is_count_up(channel_num) {
            let current_cycle_count = self.cycle_manager.get_cycle_count::<CPU>();
            let diff = if channel.scheduled_cycle > current_cycle_count {
                channel.scheduled_cycle - current_cycle_count
            } else {
                0
            };
            channel.current_value = -((diff >> channel.current_shift) as i16);
        }
        channel.current_value as u16
    }

    pub fn set_cnt_l(&mut self, channel: usize, mask: u16, value: u16) {
        let mut channel = self.channels[channel].borrow_mut();
        channel.cnt_l = utils::negative((channel.cnt_l as u16 & !mask) | (value & mask)) as i16;
    }

    pub fn set_cnt_h(&mut self, channel_num: usize, mut mask: u16, value: u16) {
        let mut channel = self.channels[channel_num].borrow_mut();
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
            let shift = if u8::from(cnt.prescaler()) == 0 || cnt.is_count_up(channel_num) {
                0
            } else {
                4 + u8::from(cnt.prescaler()) * 2
            };
            if channel.current_shift != shift {
                channel.current_shift = shift;
                update = true;
            }
        }

        if update && u8::from(cnt.prescaler()) == 0 && !cnt.is_count_up(channel_num) {
            let remaining_cycles =
                (-((channel.current_value as i32) << channel.current_shift)) as u32;
            channel.scheduled_cycle = self.cycle_manager.schedule::<CPU, _>(
                remaining_cycles,
                Box::new(TimersEvent::new(self.channels[channel_num].clone())),
            );
        }
    }
}

struct TimersEvent {
    channel: Rc<FastCell<TimerChannel>>,
    scheduled_at: u64,
}

impl TimersEvent {
    fn new(channel: Rc<FastCell<TimerChannel>>) -> Self {
        TimersEvent {
            channel,
            scheduled_at: 0,
        }
    }
}

impl CycleEvent for TimersEvent {
    fn scheduled(&mut self, timestamp: &u64) {
        self.scheduled_at = *timestamp;
    }

    fn trigger(&mut self, _: u16) {
        let channel = self.channel.borrow();
        if self.scheduled_at != channel.scheduled_cycle {
            return;
        }

        todo!()
    }
}
