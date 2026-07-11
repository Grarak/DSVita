use crate::core::cpu_regs::InterruptFlag;
use crate::core::emu::Emu;
use crate::core::CpuType::ARM7;
use crate::savestate::Savestate;
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
    // Virtual keys, never reach guest registers directly (see input_process_hotkeys)
    BlowMic = 12,
    Lid = 13,
}

#[derive(Savestate)]
pub struct Input {
    key_input: u16,
    ext_key_in: u16,
    #[savestate(skip)]
    key_map: Arc<AtomicU32>,
    #[savestate(skip)]
    prev_lid_btn: bool,
}

impl Input {
    pub fn new(key_map: Arc<AtomicU32>) -> Self {
        Input {
            key_input: 0x3FF,
            ext_key_in: 0x007F,
            key_map,
            prev_lid_btn: false,
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

impl Emu {
    // Runs once per frame at the vblank hook on the cpu thread: virtual hotkeys that need
    // core-side state. Holding the blow key loops the canned blow clip through the mic;
    // a lid key press toggles the hinge state (EXTKEYIN bit 7) and opening fires the
    // hinge irq that wakes the arm7 from sleep.
    pub fn input_process_hotkeys(&mut self) {
        let key_map = self.input.key_map.load(Ordering::Relaxed);

        let blow_held = key_map & (1 << Keycode::BlowMic as u8) == 0;
        if blow_held && !self.spi.is_blow_mic_active() {
            self.spi.start_blow_mic();
        }

        let lid_btn = key_map & (1 << Keycode::Lid as u8) == 0;
        if lid_btn && !self.input.prev_lid_btn {
            let was_closed = self.input.ext_key_in & 0x80 != 0;
            if was_closed {
                self.input.ext_key_in &= !0x80;
                self.cpu_send_interrupt(ARM7, InterruptFlag::ScreensUnfolding);
            } else {
                self.input.ext_key_in |= 0x80;
            }
        }
        self.input.prev_lid_btn = lid_btn;
    }
}
