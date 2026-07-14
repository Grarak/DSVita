// Debug-only runtime command surface, shared by the Linux TCP port (DSVITA_DBG_PORT in
// sdl.rs) and the Android broadcast receiver (nativeDebugCmd in android.rs). Commands
// mutate a DebugState; the platform poll_event folds it into each frame's inputs.
//
// Newline/broadcast-delimited text commands:
//   press/release <btn> | buttons [<btn>...] | touch <x> <y> | touch off |
//   framelimit <0..9> | savestate | inst-log | quit
// btn: a b x y up down left right start select l r. inst-log arms --inst-log-lazy
// capture. Replies "ok" or "err: ...".

use crate::core::input;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

// Held-button bits are DS input::Keycode positions.
pub struct DebugState {
    pub held_buttons: AtomicU32,
    pub touch: AtomicI32,              // (x << 16) | y, or -1 for released
    pub pending_framelimit: AtomicI32, // -1 = none, else 0..=9
    pub quit: AtomicBool,
}

impl DebugState {
    pub const fn new() -> Self {
        DebugState {
            held_buttons: AtomicU32::new(0),
            touch: AtomicI32::new(-1),
            pending_framelimit: AtomicI32::new(-1),
            quit: AtomicBool::new(false),
        }
    }
}

pub fn handle_debug_cmd(state: &DebugState, line: &str) -> String {
    let mut it = line.split_whitespace();
    let cmd = it.next().unwrap_or("");
    match cmd {
        "" => "ok".to_owned(),
        "press" | "release" => {
            let Some(code) = it.next().and_then(debug_input_key) else {
                return "err: unknown button".to_owned();
            };
            let bit = 1u32 << code as u8;
            if cmd == "press" {
                state.held_buttons.fetch_or(bit, Ordering::Relaxed);
            } else {
                state.held_buttons.fetch_and(!bit, Ordering::Relaxed);
            }
            "ok".to_owned()
        }
        "buttons" => {
            let mut mask = 0u32;
            for tok in it {
                match debug_input_key(tok) {
                    Some(code) => mask |= 1 << code as u8,
                    None => return format!("err: unknown button '{tok}'"),
                }
            }
            state.held_buttons.store(mask, Ordering::Relaxed);
            "ok".to_owned()
        }
        "touch" => {
            match it.next() {
                Some("off") | None => state.touch.store(-1, Ordering::Relaxed),
                Some(x) => {
                    let (Ok(x), Some(Ok(y))) = (x.parse::<i32>(), it.next().map(str::parse::<i32>)) else {
                        return "err: touch <x> <y> | touch off".to_owned();
                    };
                    if !(0..256).contains(&x) || !(0..192).contains(&y) {
                        return "err: touch out of range (x 0..256, y 0..192)".to_owned();
                    }
                    state.touch.store((x << 16) | y, Ordering::Relaxed);
                }
            }
            "ok".to_owned()
        }
        "framelimit" => match it.next().and_then(|s| s.parse::<i32>().ok()) {
            Some(n @ 0..=9) => {
                state.pending_framelimit.store(n, Ordering::Relaxed);
                "ok".to_owned()
            }
            _ => "err: framelimit <0..9>".to_owned(),
        },
        "savestate" => {
            crate::savestate::request_save();
            "ok".to_owned()
        }
        "inst-log" => {
            crate::debug_inst_log::arm_lazy();
            "ok".to_owned()
        }
        "quit" => {
            state.quit.store(true, Ordering::Relaxed);
            "ok".to_owned()
        }
        other => format!("err: unknown cmd '{other}'"),
    }
}

// Debug-only DS button names → input::Keycode.
fn debug_input_key(name: &str) -> Option<input::Keycode> {
    use input::Keycode::*;
    Some(match name {
        "a" => A,
        "b" => B,
        "x" => X,
        "y" => Y,
        "up" => Up,
        "down" => Down,
        "left" => Left,
        "right" => Right,
        "start" => Start,
        "select" => Select,
        "l" => TriggerL,
        "r" => TriggerR,
        _ => return None,
    })
}
