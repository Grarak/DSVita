use std::sync::atomic::AtomicU8;
use std::sync::Arc;

pub struct VramContext {
    stat: Arc<AtomicU8>,
    cnt: u8,
}

impl VramContext {
    pub fn new(stat: Arc<AtomicU8>) -> Self {
        VramContext { stat, cnt: 0 }
    }

    fn set_cnt(&self, bank: usize, value: u8) {
        let masks = [0x9B, 0x9B, 0x9F, 0x9F, 0x87, 0x9F, 0x9F, 0x83, 0x83];
        let value = value & masks[bank];
        if self.cnt == value {
            return;
        }

        todo!()
    }
}
