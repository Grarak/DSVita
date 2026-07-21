use crate::core::graphics::gpu_3d::renderer_3d::{Gpu3DRenderer, WidescreenOption};
use crate::key_bindings::KeyBinding;
use crate::screen_layouts::{ScreenLayout, ScreenLayouts};
use ini::Ini;
use lazy_static::lazy_static;
use std::convert::Into;
use std::fmt::{Debug, Display, Formatter};
use std::hint::unreachable_unchecked;
use std::path::PathBuf;
use std::str::FromStr;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumCount, EnumIter, EnumString, FromRepr, IntoStaticStr};

fn framelimit_value() -> SettingValue {
    const VALUES: [&str; 10] = ["off", "100%", "125%", "150%", "175%", "200%", "250%", "300%", "400%", "500%"];
    SettingValue::List(ListInner::new(1, VALUES.into_iter().map(|value| value.to_string()).collect()))
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, EnumIter, EnumString, Eq, IntoStaticStr, PartialEq)]
pub enum Arm7Emu {
    #[default]
    AccurateLle = 0,
    SoundHle = 1,
    Hle = 2,
}

impl From<u8> for Arm7Emu {
    fn from(value: u8) -> Self {
        debug_assert!(value <= Arm7Emu::Hle as u8);
        unsafe { std::mem::transmute(value) }
    }
}

impl From<Arm7Emu> for u8 {
    fn from(value: Arm7Emu) -> Self {
        value as u8
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, EnumIter, EnumString, Eq, IntoStaticStr, PartialEq)]
pub enum Language {
    Japanese = 0,
    #[default]
    English = 1,
    French = 2,
    German = 3,
    Italian = 4,
    Spanish = 5,
}

impl From<u8> for Language {
    fn from(value: u8) -> Self {
        debug_assert!(value <= Language::Spanish as u8);
        unsafe { std::mem::transmute(value) }
    }
}

impl From<Language> for u8 {
    fn from(value: Language) -> Self {
        value as u8
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, EnumIter, EnumString, Eq, IntoStaticStr, PartialEq)]
pub enum RightStickMode {
    #[default]
    Off = 0,
    #[strum(serialize = "Touch camera")]
    TouchCamera = 1,
    #[strum(serialize = "L and R triggers")]
    TriggerLR = 2,
}

impl From<u8> for RightStickMode {
    fn from(value: u8) -> Self {
        debug_assert!(value <= RightStickMode::TriggerLR as u8);
        unsafe { std::mem::transmute(value) }
    }
}

impl From<RightStickMode> for u8 {
    fn from(value: RightStickMode) -> Self {
        value as u8
    }
}

#[derive(Clone)]
pub struct ListInner {
    pub selection: usize,
    pub values: Vec<String>,
    initial_selection: String,
}

impl ListInner {
    pub fn new(selection: usize, values: Vec<String>) -> Self {
        ListInner {
            initial_selection: if selection >= values.len() { "".to_string() } else { values[selection].clone() },
            selection,
            values,
        }
    }

    fn reset_to_initial_selection(&mut self) {
        self.selection = self.values.iter().position(|value| self.initial_selection == *value).unwrap_or(0)
    }
}

#[derive(Clone)]
pub struct SliderInner {
    pub value: i32,
    pub min: i32,
    pub max: i32,
}

impl SliderInner {
    pub const fn new(value: i32, min: i32, max: i32) -> Self {
        SliderInner { value, min, max }
    }
}

#[derive(Clone)]
pub enum SettingValue {
    Bool(bool),
    List(ListInner),
    Int(usize),
    Slider(SliderInner),
}

impl<D: Default + Into<u8> + Sized + Into<&'static str>, T: Iterator<Item = D>> From<T> for SettingValue {
    fn from(value: T) -> Self {
        SettingValue::List(ListInner::new(
            Into::<u8>::into(D::default()) as usize,
            value.map(|d| Into::<&'static str>::into(d).to_string()).collect(),
        ))
    }
}

impl SettingValue {
    pub fn next(&mut self) {
        match self {
            SettingValue::Bool(value) => *value ^= true,
            SettingValue::List(inner) => inner.selection = (inner.selection + 1) % inner.values.len(),
            SettingValue::Int(_) => {}
            SettingValue::Slider(_) => {}
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            SettingValue::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool_mut(&mut self) -> Option<&mut bool> {
        match self {
            SettingValue::Bool(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<(usize, &Vec<String>)> {
        match self {
            SettingValue::List(inner) => Some((inner.selection, &inner.values)),
            _ => None,
        }
    }

    pub fn as_list_mut(&mut self) -> Option<(&mut usize, &mut Vec<String>)> {
        match self {
            SettingValue::List(inner) => Some((&mut inner.selection, &mut inner.values)),
            _ => None,
        }
    }

    pub fn as_slider(&self) -> Option<&SliderInner> {
        match self {
            SettingValue::Slider(inner) => Some(inner),
            _ => None,
        }
    }

    fn parse_str(&mut self, str: &str) {
        match self {
            SettingValue::Bool(value) => *value = bool::from_str(str).unwrap_or(false),
            SettingValue::List(inner) => {
                inner.initial_selection = str.to_string();
                inner.reset_to_initial_selection();
            }
            SettingValue::Int(_) => {}
            SettingValue::Slider(inner) => {
                if let Ok(value) = i32::from_str(str) {
                    inner.value = value.clamp(inner.min, inner.max);
                }
            }
        }
    }

    fn to_parse_string(&self) -> String {
        match self {
            SettingValue::Bool(value) => value.to_string(),
            SettingValue::List(inner) => inner.values[inner.selection].to_string(),
            SettingValue::Int(value) => value.to_string(),
            SettingValue::Slider(inner) => inner.value.to_string(),
        }
    }
}

impl Display for SettingValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SettingValue::Bool(value) => if *value { "on" } else { "off" }.to_string(),
                SettingValue::List(inner) => inner.values[inner.selection].clone(),
                SettingValue::Int(value) => value.to_string(),
                SettingValue::Slider(inner) => inner.value.to_string(),
            }
        )
    }
}

#[derive(Copy, Clone, EnumIter, Eq, IntoStaticStr, PartialEq)]
pub enum SettingGroup {
    Emulation,
    Graphics,
    Screen,
    System,
}

#[derive(Clone)]
pub struct Setting {
    pub title: &'static str,
    pub description: &'static str,
    pub value: SettingValue,
    pub runtime: bool,
    pub group: SettingGroup,
}

impl Setting {
    const fn new(title: &'static str, description: &'static str, value: SettingValue, runtime: bool, group: SettingGroup) -> Self {
        Setting {
            title,
            description,
            value,
            runtime,
            group,
        }
    }
}

lazy_static! {
    // Built straight from SettingId, so the array order always matches the index
    // enum: element `i` is `SettingId::from_repr(i)`. Add/reorder a setting in one
    // place (the enum + its `definition()` arm) and everything stays in sync.
    pub static ref DEFAULT_SETTINGS: Settings = Settings(std::array::from_fn(|i| SettingId::from_repr(i).unwrap().definition()));
}

#[derive(Clone)]
pub struct Settings([Setting; SettingId::COUNT]);

#[repr(usize)]
#[derive(Copy, Clone, EnumCount, FromRepr)]
pub(crate) enum SettingId {
    Arm7Emu,
    HleOsIrqHandler,
    Framelimit,
    Audio,
    AudioStretching,
    Geometry3DSkip,
    Upscale3DFactor,
    ScreenLayout,
    Widescreen,
    SwapScreen,
    TopScreenScale,
    BottomScreenScale,
    TapCornerToSwap,
    StreamTopScreen,
    Language,
    Controls,
    JoystickAsDpad,
    RightStickMode,
    RightStickTouchSensitivity,
    RightStickTouchPivotX,
    RightStickTouchPivotY,
    RearTouch,
    ShowDebugStatistics,
    Retroachievements,
}

impl SettingId {
    pub(crate) fn definition(self) -> Setting {
        match self {
            SettingId::Arm7Emu => Setting::new(
                "Arm7 Emulation",
                "How the ARM7 co-processor is emulated. AccurateLle is slowest but most compatible. SoundHle is ~10%% faster and Hle ~15-20%% faster, but both reduce compatibility. Use AccurateLle if a game crashes, freezes or misbehaves.",
                Arm7Emu::iter().into(),
                false,
                SettingGroup::Emulation,
            ),
            SettingId::HleOsIrqHandler => Setting::new(
                "HLE OS irq handler",
                "Emulates the system interrupt handler at a higher level for extra speed. May cause crashes in some games.",
                SettingValue::Bool(true),
                false,
                SettingGroup::Emulation,
            ),
            SettingId::Framelimit => Setting::new("Framelimit", "Caps the emulation speed relative to real hardware. Set to 'off' to run as fast as possible.", framelimit_value(), true, SettingGroup::Emulation),
            SettingId::Audio => Setting::new("Audio", "Turn audio off for a small performance boost.", SettingValue::Bool(true), true, SettingGroup::Emulation),
            SettingId::AudioStretching => Setting::new("Audio stretching", "Stretches audio to prevent crackling when a game runs below full speed. Adds a little latency.", SettingValue::Bool(true), true, SettingGroup::Emulation),
            SettingId::Geometry3DSkip => Setting::new(
                "Geometry 3D frameskip",
                "Skips redundant 3D frames for better performance at the cost of some latency. Turn off if a game has 3D glitches or renders 3D on both screens.",
                SettingValue::Bool(true),
                true,
                SettingGroup::Graphics,
            ),
            SettingId::Upscale3DFactor => Setting::new("Upscale 3D factor", "Renders 3D graphics at a higher internal resolution. Higher values look sharper but run slower.", Gpu3DRenderer::upscale_factor_settings_value(), true, SettingGroup::Graphics),
            SettingId::ScreenLayout => Setting::new("Screen Layout", "How the two screens are arranged on the display. In-game: PS + L or PS + R cycles through layouts.", SettingValue::List(ListInner::new(0, vec![])), true, SettingGroup::Screen),
            SettingId::Widescreen => Setting::new("Wide 3D screen", "Experimental widescreen hack for 3D. Can cause glitches. Only available with the single, focus-overlap or custom layouts.", Gpu3DRenderer::widescreen_settings_value(), true, SettingGroup::Graphics),
            SettingId::SwapScreen => Setting::new("Swap screens", "Swaps the top and bottom screens. In-game: PS + Cross.", SettingValue::Bool(false), true, SettingGroup::Screen),
            SettingId::TopScreenScale => Setting::new("Top screen scale", "Size of the top screen. In-game: PS + Square cycles sizes.", ScreenLayout::scale_settings_value(), true, SettingGroup::Screen),
            SettingId::BottomScreenScale => Setting::new("Bottom screen scale", "Size of the bottom screen. In-game: PS + Circle cycles sizes.", ScreenLayout::scale_settings_value(), true, SettingGroup::Screen),
            SettingId::TapCornerToSwap => Setting::new("Tap corner to swap screens", "Tap the bottom-right corner of the screen to swap the large and small screens (same as PS + Cross).", SettingValue::Bool(false), true, SettingGroup::Screen),
            SettingId::StreamTopScreen => Setting::new("Stream top screen", "Stream top screen over a modified version of udcd-uvc. You need to have CapUnlocker and a custom version of udcd-uvc installed. You can find the instructions in the Readme.", SettingValue::Bool(false), false, SettingGroup::Screen),
            SettingId::Language => Setting::new("Language", "Preferred in-game language. Only applies if the game actually includes it.", Language::iter().into(), false, SettingGroup::System),
            SettingId::Controls => Setting::new("Controls", "Custom button mapping to use. Create profiles under Global settings.", SettingValue::List(ListInner::new(0, vec![])), true, SettingGroup::System),
            SettingId::JoystickAsDpad => Setting::new("Joystick as D-Pad", "Use the left analog stick as the D-Pad.", SettingValue::Bool(true), true, SettingGroup::System),
            SettingId::RightStickMode => Setting::new(
                "Right stick function",
                if cfg!(target_os = "vita") {
                    "What the right analog stick does. 'Touch camera' drags the touchscreen around the pivot point below, for camera control in 3D games with touch aiming. 'L and R triggers' holds L when the stick moves left and R when it moves right."
                } else {
                    "What the emulated right stick (hold the right mouse button and drag) does. 'Touch camera' drags the touchscreen around the pivot point below, for camera control in 3D games with touch aiming. 'L and R triggers' holds L when the stick moves left and R when it moves right."
                },
                RightStickMode::iter().into(),
                true,
                SettingGroup::System,
            ),
            SettingId::RightStickTouchSensitivity => Setting::new(
                "Touch camera sensitivity",
                "How far the touch drag moves from the pivot at full stick deflection, in percent. Higher values turn the camera faster.",
                SettingValue::Slider(SliderInner::new(100, 25, 200)),
                true,
                SettingGroup::System,
            ),
            SettingId::RightStickTouchPivotX => Setting::new(
                "Touch camera pivot X",
                "Horizontal center of the right stick touch drag, in DS touchscreen pixels.",
                SettingValue::Slider(SliderInner::new(128, 0, 255)),
                true,
                SettingGroup::System,
            ),
            SettingId::RightStickTouchPivotY => Setting::new(
                "Touch camera pivot Y",
                "Vertical center of the right stick touch drag, in DS touchscreen pixels.",
                SettingValue::Slider(SliderInner::new(96, 0, 191)),
                true,
                SettingGroup::System,
            ),
            SettingId::RearTouch => Setting::new(
                "Rear touch as touchscreen",
                if cfg!(target_os = "vita") {
                    "Maps the rear touchpad to the DS touchscreen, so you can tap and drag without covering the display. The whole pad covers the whole touchscreen; the front touchscreen keeps working."
                } else {
                    "Maps the window to the DS touchscreen while the middle mouse button is held, emulating the Vita's rear touchpad."
                },
                SettingValue::Bool(false),
                true,
                SettingGroup::System,
            ),
            SettingId::ShowDebugStatistics => Setting::new("Show debug statistics", "Show FPS and other debug information while playing.", SettingValue::Bool(true), true, SettingGroup::System),
            SettingId::Retroachievements => Setting::new("Retroachievements", "Enables RetroAchievements. Log in first via Global settings.", SettingValue::Bool(true), false, SettingGroup::System),
        }
    }
}

impl Settings {
    pub fn screen_layout(&self, screen_layouts: &ScreenLayouts) -> ScreenLayout {
        unsafe {
            ScreenLayout::new(
                screen_layouts,
                self.0[SettingId::ScreenLayout as usize].value.as_list().unwrap_unchecked().0,
                self.0[SettingId::SwapScreen as usize].value.as_bool().unwrap_unchecked(),
                self.0[SettingId::TopScreenScale as usize].value.as_list().unwrap_unchecked().0,
                self.0[SettingId::BottomScreenScale as usize].value.as_list().unwrap_unchecked().0,
            )
        }
    }

    pub fn populate_screen_layouts(&mut self, layouts: &ScreenLayouts) {
        let (_, values) = unsafe { self.0[SettingId::ScreenLayout as usize].value.as_list_mut().unwrap_unchecked() };
        let first_population = values.is_empty();
        values.clear();
        for i in 0..layouts.len() {
            values.push(layouts.get_name(i).to_string());
        }
        if first_population {
            match &mut self.0[SettingId::ScreenLayout as usize].value {
                SettingValue::List(inner) => inner.reset_to_initial_selection(),
                _ => unsafe { unreachable_unchecked() },
            }
        }
    }

    pub fn joystick_as_dpad(&self) -> bool {
        unsafe { self.0[SettingId::JoystickAsDpad as usize].value.as_bool().unwrap_unchecked() }
    }

    pub fn right_stick_mode(&self) -> RightStickMode {
        unsafe { RightStickMode::from(self.0[SettingId::RightStickMode as usize].value.as_list().unwrap_unchecked().0 as u8) }
    }

    /// Sensitivity of the right-stick touch drag as a factor (setting is in percent).
    pub fn right_stick_touch_sensitivity(&self) -> f32 {
        unsafe { self.0[SettingId::RightStickTouchSensitivity as usize].value.as_slider().unwrap_unchecked().value as f32 / 100.0 }
    }

    /// Pivot of the right-stick touch drag, in DS touchscreen pixels.
    pub fn right_stick_touch_pivot(&self) -> (i16, i16) {
        unsafe {
            (
                self.0[SettingId::RightStickTouchPivotX as usize].value.as_slider().unwrap_unchecked().value as i16,
                self.0[SettingId::RightStickTouchPivotY as usize].value.as_slider().unwrap_unchecked().value as i16,
            )
        }
    }

    pub fn rear_touch(&self) -> bool {
        unsafe { self.0[SettingId::RearTouch as usize].value.as_bool().unwrap_unchecked() }
    }

    pub fn tap_corner_to_swap(&self) -> bool {
        unsafe { self.0[SettingId::TapCornerToSwap as usize].value.as_bool().unwrap_unchecked() }
    }

    pub fn controls_index(&self) -> usize {
        unsafe { self.0[SettingId::Controls as usize].value.as_list().unwrap_unchecked().0 }
    }

    pub fn populate_controls(&mut self, default_binding: &KeyBinding, bindings: &[KeyBinding]) {
        let (_, values) = unsafe { self.0[SettingId::Controls as usize].value.as_list_mut().unwrap_unchecked() };
        let first_population = values.is_empty();
        values.clear();
        values.push(default_binding.name.clone());
        for binding in bindings {
            values.push(binding.name.clone());
        }
        if first_population {
            match &mut self.0[SettingId::Controls as usize].value {
                SettingValue::List(inner) => inner.reset_to_initial_selection(),
                _ => unsafe { unreachable_unchecked() },
            }
        }
    }

    pub fn framelimit(&self) -> u8 {
        unsafe { self.0[SettingId::Framelimit as usize].value.as_list().unwrap_unchecked().0 as u8 }
    }

    pub fn audio(&self) -> bool {
        unsafe { self.0[SettingId::Audio as usize].value.as_bool().unwrap_unchecked() }
    }

    pub fn arm7_emu(&self) -> Arm7Emu {
        unsafe { Arm7Emu::from(self.0[SettingId::Arm7Emu as usize].value.as_list().unwrap_unchecked().0 as u8) }
    }

    pub fn hle_os_irq_handler(&self) -> bool {
        unsafe { self.0[SettingId::HleOsIrqHandler as usize].value.as_bool().unwrap_unchecked() }
    }

    pub fn geometry_3d_skip(&self) -> bool {
        unsafe { self.0[SettingId::Geometry3DSkip as usize].value.as_bool().unwrap_unchecked() }
    }

    pub fn upscale_3d_factor(&self) -> u8 {
        unsafe { self.0[SettingId::Upscale3DFactor as usize].value.as_list().unwrap_unchecked().0 as u8 }
    }

    pub fn widescreen(&self) -> WidescreenOption {
        unsafe { WidescreenOption::from(self.0[SettingId::Widescreen as usize].value.as_list().unwrap_unchecked().0 as u8) }
    }

    pub fn stream_top_screen(&self) -> bool {
        unsafe { self.0[SettingId::StreamTopScreen as usize].value.as_bool().unwrap_unchecked() }
    }

    pub fn audio_stretching(&self) -> bool {
        unsafe { self.0[SettingId::AudioStretching as usize].value.as_bool().unwrap_unchecked() }
    }

    pub fn language(&self) -> Language {
        unsafe { Language::from(self.0[SettingId::Language as usize].value.as_list().unwrap_unchecked().0 as u8) }
    }

    pub fn show_debug_stats(&self) -> bool {
        unsafe { self.0[SettingId::ShowDebugStatistics as usize].value.as_bool().unwrap_unchecked() }
    }

    pub fn retroachievements(&self) -> bool {
        unsafe { self.0[SettingId::Retroachievements as usize].value.as_bool().unwrap_unchecked() }
    }

    pub fn set_screen_layout(&mut self, screen_layout: &ScreenLayout) {
        *self.0[SettingId::ScreenLayout as usize].value.as_list_mut().unwrap().0 = screen_layout.index;
        *self.0[SettingId::SwapScreen as usize].value.as_bool_mut().unwrap() = screen_layout.swap;
    }

    pub fn set_framelimit(&mut self, value: u8) {
        *self.0[SettingId::Framelimit as usize].value.as_list_mut().unwrap().0 = value as usize;
    }

    pub fn set_audio(&mut self, value: bool) {
        *self.0[SettingId::Audio as usize].value.as_bool_mut().unwrap() = value;
    }

    pub fn set_arm7_emu(&mut self, value: Arm7Emu) {
        *self.0[SettingId::Arm7Emu as usize].value.as_list_mut().unwrap().0 = value as usize
    }

    pub fn set_hle_os_irq_handler(&mut self, value: bool) {
        *self.0[SettingId::HleOsIrqHandler as usize].value.as_bool_mut().unwrap() = value;
    }

    pub fn set_retroachievements(&mut self, value: bool) {
        unsafe { *self.0[SettingId::Retroachievements as usize].value.as_bool_mut().unwrap_unchecked() = value };
    }

    pub fn get_all_mut(&mut self) -> &mut [Setting] {
        &mut self.0
    }
}

impl Debug for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_map();
        for setting in &self.0 {
            list.key(&setting.title).value(&setting.value.to_string());
        }
        list.finish()
    }
}

pub struct SettingsConfig {
    pub settings: Settings,
    pub settings_file_path: PathBuf,
    pub dirty: bool,
}

impl From<Settings> for SettingsConfig {
    fn from(value: Settings) -> Self {
        SettingsConfig {
            settings: value,
            settings_file_path: PathBuf::new(),
            dirty: false,
        }
    }
}

impl SettingsConfig {
    pub fn new(path: PathBuf) -> Self {
        let mut settings = DEFAULT_SETTINGS.clone();

        if let Ok(ini) = Ini::load_from_file(&path) {
            if let Some(section) = ini.section(None::<String>) {
                for setting in settings.get_all_mut() {
                    if let Some(value) = section.get(setting.title) {
                        setting.value.parse_str(value);
                    }
                }
            }
        }

        SettingsConfig {
            settings,
            settings_file_path: path,
            dirty: false,
        }
    }

    pub fn flush(&mut self) {
        if self.dirty {
            let mut ini = Ini::new();
            let mut section = ini.with_section(None::<String>);
            for setting in self.settings.get_all_mut() {
                section.set(setting.title, setting.value.to_parse_string());
            }
            ini.write_to_file(&self.settings_file_path).unwrap();
            self.dirty = false;
        }
    }
}
