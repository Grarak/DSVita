use crate::hle::cpu_regs::{CpuRegs, InterruptFlag};
use crate::hle::cycle_manager::{CycleEvent, CycleManager};
use crate::hle::CpuType;
use bilge::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

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

pub struct TimersContext<const CPU: CpuType> {
    cycle_manager: Rc<CycleManager>,
    cpu_regs: Rc<CpuRegs<CPU>>,
    channels: Rc<RefCell<[TimerChannel; CHANNEL_COUNT]>>,
}

impl<const CPU: CpuType> TimersContext<CPU> {
    pub fn new(cycle_manager: Rc<CycleManager>, cpu_regs: Rc<CpuRegs<CPU>>) -> Self {
        TimersContext {
            cycle_manager,
            cpu_regs,
            channels: Rc::new(RefCell::new([TimerChannel::default(); CHANNEL_COUNT])),
        }
    }

    pub fn get_cnt_l<const CHANNEL_NUM: usize>(&self) -> u16 {
        let mut channels = self.channels.borrow_mut();
        let channel = &mut channels[CHANNEL_NUM];
        let cnt = TimerCntH::from(channel.cnt_h);
        if bool::from(cnt.start()) && !cnt.is_count_up(CHANNEL_NUM) {
            let current_cycle_count = self.cycle_manager.get_cycle_count();
            let diff = if channel.scheduled_cycle > current_cycle_count {
                channel.scheduled_cycle - current_cycle_count
            } else {
                0
            } as u32;
            channel.current_value = ((TIME_OVERFLOW - diff) >> channel.current_shift) as u16;
        }
        channel.current_value
    }

    pub fn get_cnt_h<const CHANNEL_NUM: usize>(&self) -> u16 {
        self.channels.borrow()[CHANNEL_NUM].cnt_h
    }

    pub fn set_cnt_l<const CHANNEL_NUM: usize>(&mut self, mask: u16, value: u16) {
        let mut channels = self.channels.borrow_mut();
        let channel = &mut channels[CHANNEL_NUM];
        channel.cnt_l = (channel.cnt_l & !mask) | (value & mask);
    }

    pub fn set_cnt_h<const CHANNEL_NUM: usize>(&mut self, mut mask: u16, value: u16) {
        let mut channels = self.channels.borrow_mut();
        let channel = &mut channels[CHANNEL_NUM];
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
            let event = TimersEvent::<CPU, CHANNEL_NUM>::new(
                self.channels.clone(),
                self.cycle_manager.clone(),
                self.cpu_regs.clone(),
            );
            channel.scheduled_cycle = self
                .cycle_manager
                .schedule(remaining_cycles, Box::new(event));
        }
    }
}

#[derive(Clone)]
struct TimersEvent<const CPU: CpuType, const CHANNEL_NUM: usize> {
    channels: Rc<RefCell<[TimerChannel; CHANNEL_COUNT]>>,
    scheduled_at: u64,
    cycle_manager: Rc<CycleManager>,
    cpu_regs: Rc<CpuRegs<CPU>>,
}

impl<const CPU: CpuType, const CHANNEL_NUM: usize> TimersEvent<CPU, CHANNEL_NUM> {
    fn new(
        channels: Rc<RefCell<[TimerChannel; CHANNEL_COUNT]>>,
        cycle_manager: Rc<CycleManager>,
        cpu_regs: Rc<CpuRegs<CPU>>,
    ) -> Self {
        TimersEvent {
            channels,
            scheduled_at: 0,
            cycle_manager,
            cpu_regs,
        }
    }

    fn overflow(&self, count_up_num: usize) {
        {
            let mut channels = self.channels.borrow_mut();
            let channel = &mut channels[count_up_num];
            let cnt = TimerCntH::from(channel.cnt_h);
            if !bool::from(cnt.start()) {
                return;
            }
            channel.current_value = channel.cnt_l;
            if !cnt.is_count_up(count_up_num) {
                let remaining_cycles =
                    (TIME_OVERFLOW - channel.current_value as u32) << channel.current_shift;
                channel.scheduled_cycle = self
                    .cycle_manager
                    .schedule(remaining_cycles, Box::new(self.clone()))
            }

            if bool::from(cnt.irq_enable()) {
                self.cpu_regs.send_interrupt(InterruptFlag::from(
                    InterruptFlag::Timer0Overflow as u8 + count_up_num as u8,
                ))
            }
        }
        if count_up_num < 3 {
            let mut overflow = false;
            {
                let mut channels = self.channels.borrow_mut();
                let channel = &mut channels[count_up_num + 1];
                let cnt = TimerCntH::from(channel.cnt_h);
                if bool::from(cnt.count_up()) {
                    channel.current_value += 1;
                    overflow = channel.current_value == 0;
                }
            }
            if overflow {
                self.overflow(count_up_num + 1);
            }
        }
    }
}

impl<const CPU: CpuType, const CHANNEL_NUM: usize> CycleEvent for TimersEvent<CPU, CHANNEL_NUM> {
    fn scheduled(&mut self, timestamp: &u64) {
        self.scheduled_at = *timestamp;
    }

    fn trigger(&mut self, _: u16) {
        {
            let mut channels = self.channels.borrow_mut();
            let channel = &mut channels[CHANNEL_NUM];
            if self.scheduled_at != channel.scheduled_cycle {
                return;
            }
        }
        self.overflow(CHANNEL_NUM);
    }
}
