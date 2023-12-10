pub struct SpuContext {
    sound_bias: u16,
}

impl SpuContext {
    pub fn new() -> Self {
        SpuContext { sound_bias: 0 }
    }

    // TODO
    pub fn set_sound_bias(&mut self, value: u16) -> u16 {
        self.sound_bias = value;
        self.sound_bias
    }
}
