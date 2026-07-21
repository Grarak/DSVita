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
        // Synthetic touch from the right stick camera feature, already in DS touchscreen
        // space (x in 0..256, y in 0..192). Real touch input takes priority over it.
        stick_touch: Option<(i16, i16)>,
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

/// Maps a right stick deflection (each axis in -1..1) to a synthetic touch point in
/// DS touchscreen coordinates, dragging around the configured pivot. Returns None
/// when the feature is off or the stick rests inside the deadzone, which the caller
/// treats as pen up.
pub fn stick_touch_point(stick_x: f32, stick_y: f32, settings: &crate::settings::Settings) -> Option<(i16, i16)> {
    const DEADZONE: f32 = 0.2;
    /// Drag distance from the pivot at full deflection and 100% sensitivity, in DS
    /// touchscreen pixels.
    const RADIUS: f32 = 70.0;

    if !settings.right_stick_touch() {
        return None;
    }
    let len = (stick_x * stick_x + stick_y * stick_y).sqrt();
    if len < DEADZONE {
        return None;
    }
    // Rescale so the drag offset grows from zero right outside the deadzone
    let scale = ((len - DEADZONE) / (1.0 - DEADZONE)).min(1.0) / len * RADIUS * settings.right_stick_touch_sensitivity();
    let (pivot_x, pivot_y) = settings.right_stick_touch_pivot();
    let x = (pivot_x as f32 + stick_x * scale).round().clamp(0.0, crate::core::graphics::gpu::DISPLAY_WIDTH as f32 - 1.0);
    let y = (pivot_y as f32 + stick_y * scale).round().clamp(0.0, crate::core::graphics::gpu::DISPLAY_HEIGHT as f32 - 1.0);
    Some((x as i16, y as i16))
}

pub const PRESENTER_AUDIO_OUT_SAMPLE_RATE: usize = 48000;
pub const PRESENTER_AUDIO_OUT_BUF_SIZE: usize = 1024;

pub const PRESENTER_AUDIO_IN_SAMPLE_RATE: usize = 16000;
pub const PRESENTER_AUDIO_IN_BUF_SIZE: usize = 256;
