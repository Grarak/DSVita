use bilge::prelude::*;

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

#[derive(Default)]
struct TimerChannel {
    reload: i16,
    cnt_h: u16,
    current_value: i16,
    current_shift: u8,
    current_value_shifted: i32,
}

#[derive(Default)]
pub struct TimersContext {
    channels: [TimerChannel; CHANNEL_COUNT],
}

impl TimersContext {
    pub fn new() -> Self {
        TimersContext::default()
    }

    pub fn get_cnt_l(&self, channel: usize) -> u16 {
        self.channels[channel].current_value as u16
    }

    pub fn set_cnt_l(&mut self, channel: usize, mask: u16, value: u16) {
        self.channels[channel].reload =
            ((self.channels[channel].reload as u16 & !mask) | (value & mask)) as i16;
    }

    pub fn set_cnt_h(&mut self, channel_num: usize, mut mask: u16, value: u16) {
        let channel = &mut self.channels[channel_num];
        let current_cnt = TimerCntH::from(channel.cnt_h);

        mask &= 0xC7;
        channel.cnt_h = (channel.cnt_h & !mask) | (value & mask);
        let cnt = TimerCntH::from(channel.cnt_h);

        let mut update =
            if !bool::from(current_cnt.start()) && bool::from(cnt.start()) {
                channel.current_value = channel.reload;
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
            channel.current_value_shifted = (channel.current_value as i32) << channel.current_shift;
        }
    }

    pub fn on_cycle_update(&mut self, cycles: u16) {
        for (index, channel) in self.channels.iter_mut().enumerate() {
            let cnt = TimerCntH::from(channel.cnt_h);
            if bool::from(cnt.start()) && !cnt.is_count_up(index) {
                channel.current_value_shifted += cycles as i32;
                channel.current_value =
                    (channel.current_value_shifted >> channel.current_shift) as i16;

                if channel.current_value_shifted >= 0 {
                    todo!()
                }
            }
        }
    }
}
