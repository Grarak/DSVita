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

#[derive(Default)]
pub struct TimersContext {
    cnt_l: [u16; CHANNEL_COUNT],
    cnt_h: [u16; CHANNEL_COUNT],
}

impl TimersContext {
    pub fn new() -> Self {
        TimersContext::default()
    }

    pub fn set_cnt_l(&mut self, channel: usize, mask: u16, value: u16) {
        self.cnt_l[channel] = (self.cnt_l[channel] & !mask) | (value & mask);
    }

    pub fn set_cnt_h(&mut self, channel: usize, mask: u16, value: u16) {
        let current_cnt = TimerCntH::from(self.cnt_h[channel]);

        if bool::from(current_cnt.start()) {
            todo!()
        }

        todo!()
    }
}
