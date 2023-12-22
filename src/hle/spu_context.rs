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

    pub fn get_cnt(&self, channel: u8) -> u32 {
        self.channels[channel as usize].cnt
    }

    pub fn set_cnt(&mut self, channel: u8, value: u32) {
        self.channels[channel as usize].cnt = value;
    }

    pub fn set_sad(&mut self, channel: u8, value: u32) {
        self.channels[channel as usize].sad = value;
    }

    pub fn set_pnt(&mut self, channel: u8, value: u16) {
        self.channels[channel as usize].pnt = value;
    }

    pub fn set_tmr(&mut self, channel: u8, value: u16) {
        self.channels[channel as usize].tmr = value;
    }

    pub fn set_len(&mut self, channel: u8, value: u32) {
        self.channels[channel as usize].len = value;
    }

    pub fn set_main_sound_cnt(&mut self, value: u16) {
        self.main_sound_cnt = value
    }

    pub fn set_sound_bias(&mut self, value: u16) {
        self.sound_bias = value;
    }
}
