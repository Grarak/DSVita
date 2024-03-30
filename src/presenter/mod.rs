pub use self::platform::*;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;

pub mod menu;
#[cfg(target_os = "vita")]
#[path = "vita.rs"]
mod platform;

pub const PRESENTER_SCREEN_WIDTH: u32 = 960;
pub const PRESENTER_SCREEN_HEIGHT: u32 = 544;

pub enum PresentEvent {
    Keymap(u16),
    Quit,
}
