const CHANNEL_COUNT: usize = 16;

#[derive(Copy, Clone, Default)]
struct SpuChannel {
    cnt: u32,
    sad: u32,
    tmr: u16,
    pnt: u16,
    len: u32,
}

pub struct SpuContext {
    channels: [SpuChannel; CHANNEL_COUNT],
    pub main_sound_cnt: u16,
    sound_bias: u16,
}

impl SpuContext {
    pub fn new() -> Self {
        SpuContext {
            channels: [SpuChannel::default(); CHANNEL_COUNT],
            main_sound_cnt: 0,
            sound_bias: 0,
        }
    }

    pub fn get_cnt(&self, channel: usize) -> u32 {
        self.channels[channel].cnt
    }

    pub fn set_cnt(&mut self, channel: usize, mask: u32, value: u32) {
        self.channels[channel].cnt = (self.channels[channel].cnt & !mask) | (value & mask);
    }

    pub fn set_sad(&mut self, channel: usize, mask: u32, value: u32) {
        self.channels[channel].sad = (self.channels[channel].sad & !mask) | (value & mask);
    }

    pub fn set_tmr(&mut self, channel: usize, mask: u16, value: u16) {
        self.channels[channel].tmr = (self.channels[channel].tmr & !mask) | (value & mask);
    }

    pub fn set_pnt(&mut self, channel: usize, mask: u16, value: u16) {
        self.channels[channel].pnt = (self.channels[channel].pnt & !mask) | (value & mask);
    }

    pub fn set_len(&mut self, channel: usize, mask: u32, value: u32) {
        self.channels[channel].len = (self.channels[channel].len & !mask) | (value & mask);
    }

    pub fn set_main_sound_cnt(&mut self, mask: u16, value: u16) {
        self.main_sound_cnt = (self.main_sound_cnt & !mask) | (value & mask);
    }

    pub fn set_sound_bias(&mut self, mask: u16, value: u16) {
        self.sound_bias = (self.sound_bias & !mask) | (value & mask);
    }
}
