use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

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
    key_input: u16,
    ext_key_in: u16,
    key_map: Arc<AtomicU32>,
}

impl Input {
    pub fn new(key_map: Arc<AtomicU32>) -> Self {
        Input {
            key_input: 0x3FF,
            ext_key_in: 0x007F,
            key_map,
        }
    }

    pub fn get_key_input(&self) -> u16 {
        let key_map = self.key_map.load(Ordering::Relaxed);
        (self.key_input & !0x3FF) | (key_map & 0x3FF) as u16
    }

    pub fn get_ext_key_in(&self) -> u16 {
        let key_map = self.key_map.load(Ordering::Relaxed);
        (self.ext_key_in & !0x43) | ((key_map >> 10) & 0x43) as u16
    }
}
