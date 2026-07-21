pub use self::platform::*;
// CJK font download + ImGui atlas merge — Android's Activity UI renders CJK natively.
#[cfg(not(target_os = "android"))]
pub mod cjk_font;
// Android renders no native UI at all — menus/dialogs are the Activity's job (Java),
// so the ImGui-based ui module and its bindings only exist off-Android.
#[cfg(not(target_os = "android"))]
pub mod ui;

#[cfg(not(target_os = "android"))]
pub(crate) mod imgui {
    #![allow(warnings, unused)]
    include!(concat!(env!("OUT_DIR"), "/imgui_bindings.rs"));
}

/// What the platform pause surface (ImGui menu, or the Android Activity) tells the main
/// loop to do next. Lives here because the android build has no `ui` module.
#[derive(Eq, PartialEq)]
pub enum UiPauseMenuReturn {
    Resume,
    BlowMic,
    Quit,
    QuitApp,
}

// Runtime debug commands (buttons/touch/framelimit/savestate/…), shared by the Linux
// TCP debug port and the Android broadcast receiver.
#[cfg(all(debug_assertions, not(target_os = "vita")))]
pub(crate) mod dbg_cmds;

#[cfg(target_os = "linux")]
#[path = "sdl.rs"]
mod platform;

// Native presenter: plain Activity + JNI, EGL, AAudio — no SDL on Android.
#[cfg(target_os = "android")]
#[path = "android.rs"]
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
        // Synthetic touch from the right stick camera or the rear touchpad, already in DS
        // touchscreen space (x in 0..256, y in 0..192). Real touch input takes priority.
        ds_touch: Option<(i16, i16)>,
        // Debug-only synthetic touch in DS screen space (x in 0..256, y in 0..192), set by the
        // keyboard tap-grid (DSVITA_DBG_TOUCH) so headless profiling can drive touch-gated titles.
        #[cfg(debug_assertions)]
        debug_touch: Option<(i16, i16)>,
    },
    CycleScreenLayout {
        offset: i8,
        swap: bool,
        top_screen_scale_offset: i8,
        bottom_screen_scale_offset: i8,
    },
    SetFramelimit(u8),
    Pause,
    Quit,
}

/// Synthetic swipe state for the 'Touch camera' right stick mode.
///
/// Games read camera drags as per-frame pen movement, so a parked pen stops the
/// camera even at full stick deflection. A held stick therefore produces repeated
/// swipes: the pen strokes from the pivot outward in the stick direction, and when
/// the stroke reaches its travel bound the pen lifts for two polls and a new stroke
/// starts at the pivot.
pub struct StickTouchState {
    // Pen offset from the pivot while a stroke is active
    offset: Option<(f32, f32)>,
    // Polls left to keep the pen lifted between strokes, so games register the
    // release instead of seeing one long backwards jump to the pivot
    lift: u8,
}

impl StickTouchState {
    pub const fn new() -> Self {
        StickTouchState { offset: None, lift: 0 }
    }

    /// Advances the swipe by one poll with the current stick deflection (each axis
    /// in -1..1) and returns the pen position in DS touchscreen coordinates, or None
    /// for pen up.
    pub fn update(&mut self, stick_x: f32, stick_y: f32, settings: &crate::settings::Settings) -> Option<(i16, i16)> {
        const DEADZONE: f32 = 0.2;
        /// Stroke length from the pivot before the pen lifts, in DS touchscreen pixels.
        const TRAVEL: f32 = 70.0;
        /// Pen speed in pixels per poll at full deflection and 100% sensitivity.
        const SPEED: f32 = 4.0;

        if settings.right_stick_mode() != crate::settings::RightStickMode::TouchCamera {
            self.offset = None;
            self.lift = 0;
            return None;
        }
        let len = (stick_x * stick_x + stick_y * stick_y).sqrt();
        if len < DEADZONE {
            self.offset = None;
            self.lift = 0;
            return None;
        }
        if self.lift > 0 {
            self.lift -= 1;
            return None;
        }

        // Rescale so the pen speed grows from zero right outside the deadzone
        let scale = ((len - DEADZONE) / (1.0 - DEADZONE)).min(1.0) / len * SPEED * settings.right_stick_touch_sensitivity();
        let offset = self.offset.get_or_insert((0.0, 0.0));
        offset.0 += stick_x * scale;
        offset.1 += stick_y * scale;

        if offset.0 * offset.0 + offset.1 * offset.1 > TRAVEL * TRAVEL {
            // Stroke exhausted: lift the pen and start a new stroke at the pivot
            self.offset = None;
            self.lift = 1;
            return None;
        }

        let (pivot_x, pivot_y) = settings.right_stick_touch_pivot();
        let x = (pivot_x as f32 + offset.0).round().clamp(0.0, crate::core::graphics::gpu::DISPLAY_WIDTH as f32 - 1.0);
        let y = (pivot_y as f32 + offset.1).round().clamp(0.0, crate::core::graphics::gpu::DISPLAY_HEIGHT as f32 - 1.0);
        Some((x as i16, y as i16))
    }
}

/// Keymap mask for the 'L and R triggers' right stick mode: clears (presses) the
/// TriggerL bit while the stick points left and the TriggerR bit while it points
/// right. All bits set when the mode is off or the stick rests in the deadzone.
pub fn stick_trigger_keymap(stick_x: f32, settings: &crate::settings::Settings) -> u32 {
    const THRESHOLD: f32 = 0.5;
    let mut keymap = 0xFFFFFFFF;
    if settings.right_stick_mode() == crate::settings::RightStickMode::TriggerLR {
        if stick_x < -THRESHOLD {
            keymap &= !(1 << crate::core::input::Keycode::TriggerL as u8);
        } else if stick_x > THRESHOLD {
            keymap &= !(1 << crate::core::input::Keycode::TriggerR as u8);
        }
    }
    keymap
}

pub const PRESENTER_AUDIO_OUT_SAMPLE_RATE: usize = 48000;
pub const PRESENTER_AUDIO_OUT_BUF_SIZE: usize = 1024;

pub const PRESENTER_AUDIO_IN_SAMPLE_RATE: usize = 16000;
pub const PRESENTER_AUDIO_IN_BUF_SIZE: usize = 256;
