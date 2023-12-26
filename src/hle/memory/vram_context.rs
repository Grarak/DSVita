use crate::logging::debug_println;
use crate::utils::{FastCell, HeapMem};
use bilge::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

const BANK_SIZE: usize = 9;

#[bitsize(8)]
#[derive(FromBits)]
struct VramCnt {
    mst: u3,
    ofs: u2,
    not_used: u2,
    enable: u1,
}

struct VramInner {
    stat: Arc<AtomicU8>,
    cnt: [u8; BANK_SIZE],
    vram_a: HeapMem<{ 128 * 1024 }>,
}

impl VramInner {
    fn new(stat: Arc<AtomicU8>) -> Self {
        VramInner {
            stat,
            cnt: [0u8; BANK_SIZE],
            vram_a: HeapMem::new(),
        }
    }

    pub fn set_cnt(&mut self, bank: usize, value: u8) {
        debug_println!("Set vram cnt {:x} to {:x}", bank, value);
        let masks = [0x9B, 0x9B, 0x9F, 0x9F, 0x87, 0x9F, 0x9F, 0x83, 0x83];
        let value = value & masks[bank];
        if self.cnt[bank] == value {
            return;
        }
        self.cnt[bank] = value;

        let cnt_a = VramCnt::from(self.cnt[0]);
        if bool::from(cnt_a.enable()) {
            todo!()
        }

        let cnt_b = VramCnt::from(self.cnt[1]);
        if bool::from(cnt_b.enable()) {
            todo!()
        }

        let cnt_c = VramCnt::from(self.cnt[2]);
        if bool::from(cnt_c.enable()) {
            todo!()
        }

        let cnt_d = VramCnt::from(self.cnt[3]);
        if bool::from(cnt_d.enable()) {
            todo!()
        }

        let cnt_e = VramCnt::from(self.cnt[4]);
        if bool::from(cnt_e.enable()) {
            todo!()
        }

        let cnt_f = VramCnt::from(self.cnt[5]);
        if bool::from(cnt_f.enable()) {
            todo!()
        }

        let cnt_g = VramCnt::from(self.cnt[6]);
        if bool::from(cnt_g.enable()) {
            todo!()
        }

        let cnt_h = VramCnt::from(self.cnt[7]);
        if bool::from(cnt_h.enable()) {
            todo!()
        }

        let cnt_i = VramCnt::from(self.cnt[8]);
        if bool::from(cnt_i.enable()) {
            todo!()
        }
    }
}

pub struct VramContext {
    stat: Arc<AtomicU8>,
    inner: FastCell<VramInner>,
}

impl VramContext {
    pub fn new() -> Self {
        let stat = Arc::new(AtomicU8::new(0));
        VramContext {
            stat: stat.clone(),
            inner: FastCell::new(VramInner::new(stat)),
        }
    }

    pub fn get_stat(&self) -> u8 {
        self.stat.load(Ordering::Relaxed)
    }

    pub fn set_cnt(&self, bank: usize, value: u8) {
        self.inner.borrow_mut().set_cnt(bank, value);
    }
}
