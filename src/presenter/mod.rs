pub use self::platform::*;
pub mod ui;

pub(crate) mod imgui {
    #![allow(warnings, unused)]
    include!(concat!(env!("OUT_DIR"), "/imgui_bindings.rs"));
}

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;

#[cfg(target_os = "vita")]
#[path = "vita.rs"]
mod platform;

pub const PRESENTER_SCREEN_WIDTH: u32 = 960;
pub const PRESENTER_SCREEN_HEIGHT: u32 = 544;

pub enum PresentEvent {
    Inputs {
        keymap: u32,
        touch: Option<(i16, i16)>,
    },
    CycleScreenLayout {
        offset: i8,
        swap: bool,
        top_screen_scale_offset: i8,
        bottom_screen_scale_offset: i8,
    },
    Pause,
    Quit,
}

pub const PRESENTER_AUDIO_OUT_SAMPLE_RATE: usize = 48000;
pub const PRESENTER_AUDIO_OUT_BUF_SIZE: usize = 1024;

pub const PRESENTER_AUDIO_IN_SAMPLE_RATE: usize = 16000;
pub const PRESENTER_AUDIO_IN_BUF_SIZE: usize = 256;
