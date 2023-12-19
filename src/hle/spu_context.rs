const CHANNEL_COUNT: usize = 16;

#[derive(Copy, Clone, Default)]
struct SpuChannel {
    cnt: u32,
}

pub struct SpuContext {
    channels: [SpuChannel; CHANNEL_COUNT],
    sound_bias: u16,
}

impl SpuContext {
    pub fn new() -> Self {
        SpuContext {
            channels: [SpuChannel::default(); CHANNEL_COUNT],
            sound_bias: 0,
        }
    }

    pub fn get_cnt(&self, channel: u8) -> u32 {
        self.channels[channel as usize].cnt
    }

    // TODO
    pub fn set_sound_bias(&mut self, value: u16) {
        self.sound_bias = value;
    }
}
