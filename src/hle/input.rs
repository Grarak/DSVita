#[repr(u8)]
#[derive(Copy, Clone)]
pub enum Keycode {
    A = 0,
    B = 1,
    Select = 2,
    Start = 3,
    Right = 4,
    Left = 5,
    Up = 6,
    Down = 7,
    TriggerR = 8,
    TriggerL = 9,
    X = 10,
    Y = 11,
}

pub struct Input {
    pub key_input: u16,
    pub ext_key_in: u16,
}

impl Input {
    pub fn new() -> Self {
        Input {
            key_input: 0x3FF,
            ext_key_in: 0x007F,
        }
    }

    pub fn update_key_map(&mut self, keys: u16) {
        self.key_input = (self.key_input & !0x1FF) | (keys & 0x1FF);
        self.ext_key_in = (self.ext_key_in & !0x3) | ((keys >> 10) & 0x3);
    }
}
