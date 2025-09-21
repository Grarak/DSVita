use ini::Ini;
use lazy_static::lazy_static;
use std::convert::Into;
use std::fmt::{Debug, Display, Formatter};
use std::path::PathBuf;
use std::str::FromStr;
use strum::IntoEnumIterator;
use strum_macros::{EnumIter, EnumString, IntoStaticStr};

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

#[derive(Copy, Clone, Debug, Default, EnumIter, EnumString, Eq, IntoStaticStr, PartialEq)]
pub enum ScreenMode {
    #[default]
    Regular,
    Rotated,
    Resized,
}

impl From<u8> for ScreenMode {
    fn from(value: u8) -> Self {
        debug_assert!(value <= ScreenMode::Resized as u8);
        unsafe { std::mem::transmute(value) }
    }
}

impl From<ScreenMode> for u8 {
    fn from(value: ScreenMode) -> Self {
        value as u8
    }
}

#[derive(Copy, Clone, Debug, Default, EnumIter, EnumString, Eq, IntoStaticStr, PartialEq)]
#[repr(u8)]
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

#[derive(Clone)]
pub enum SettingValue {
    Bool(bool),
    List(usize, Vec<&'static str>),
}

impl<D: Default + Into<u8> + Sized + Into<&'static str>, T: Iterator<Item = D>> From<T> for SettingValue {
    fn from(value: T) -> Self {
        SettingValue::List(Into::<u8>::into(D::default()) as usize, value.map(|d| d.into()).collect())
    }
}

impl SettingValue {
    pub fn next(&mut self) {
        match self {
            SettingValue::Bool(value) => *value ^= true,
            SettingValue::List(selection, values) => *selection = (*selection + 1) % values.len(),
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

    pub fn as_list(&self) -> Option<(usize, &Vec<&'static str>)> {
        match self {
            SettingValue::List(selection, values) => Some((*selection, values)),
            _ => None,
        }
    }

    pub fn as_list_mut(&mut self) -> Option<(&mut usize, &mut Vec<&'static str>)> {
        match self {
            SettingValue::List(selection, values) => Some((selection, values)),
            _ => None,
        }
    }

    fn parse_str(&mut self, str: &str) {
        match self {
            SettingValue::Bool(value) => *value = bool::from_str(str).unwrap_or(false),
            SettingValue::List(selection, values) => {
                if let Some(index) = values.iter().position(|value| *value == str) {
                    *selection = index;
                }
            }
        }
    }

    fn to_parse_string(&self) -> String {
        match self {
            SettingValue::Bool(value) => value.to_string(),
            SettingValue::List(selection, values) => values[*selection].to_string(),
        }
    }
}

impl Display for SettingValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SettingValue::Bool(value) => {
                    if *value {
                        "on"
                    } else {
                        "off"
                    }
                }
                SettingValue::List(selection, values) => &values[*selection],
            }
        )
    }
}

#[derive(Clone)]
pub struct Setting {
    pub title: &'static str,
    pub description: &'static str,
    pub value: SettingValue,
    pub runtime: bool,
}

impl Setting {
    const fn new(title: &'static str, description: &'static str, value: SettingValue, runtime: bool) -> Self {
        Setting { title, description, value, runtime }
    }
}

lazy_static! {
    pub static ref DEFAULT_SETTINGS: Settings = Settings(
        [
            Setting::new(
                "Screen Mode",
                "Can be used to simulate vertical holding, for games like Brain Age",
                ScreenMode::iter().into(),
                true,
            ),
            Setting::new("Framelimit", "Limits gamespeed to 60fps", SettingValue::Bool(true), true),
            Setting::new("Audio", "Disabling audio can give a performance boost", SettingValue::Bool(true), true),
            Setting::new(
                "Arm7 Emulation",
                "AccurateLle: Slowest, best compatibility, SoundHle: ~10%% faster, reduced compatibility,\nHle: ~15-20%% faster, worst compatibility. Use AccurateLle if game crashes, gets stuck or bugs occur.",
                Arm7Emu::iter().into(),
                false,
            ),
            Setting::new(
                "Arm7 jit block validation",
                "Only needed for nds homebrew. Commercial games usually don't need to have this enabled.",
                SettingValue::Bool(false),
                false,
            ),
            Setting::new(
                "Language",
                "Language of the game. Some ROMs only come with one language. Make sure yours is multilingual.",
                Language::iter().into(),
                false,
            ),
        ],
    );
}

#[derive(Clone)]
pub struct Settings([Setting; 6]);

impl Settings {
    pub fn screenmode(&self) -> ScreenMode {
        unsafe { ScreenMode::from(self.0[0].value.as_list().unwrap_unchecked().0 as u8) }
    }

    pub fn framelimit(&self) -> bool {
        unsafe { self.0[1].value.as_bool().unwrap_unchecked() }
    }

    pub fn audio(&self) -> bool {
        unsafe { self.0[2].value.as_bool().unwrap_unchecked() }
    }

    pub fn arm7_hle(&self) -> Arm7Emu {
        unsafe { Arm7Emu::from(self.0[3].value.as_list().unwrap_unchecked().0 as u8) }
    }

    pub fn arm7_block_validation(&self) -> bool {
        unsafe { self.0[4].value.as_bool().unwrap_unchecked() }
    }

    pub fn language(&self) -> Language {
        unsafe { Language::from(self.0[5].value.as_list().unwrap_unchecked().0 as u8) }
    }

    pub fn set_framelimit(&mut self, value: bool) {
        *self.0[1].value.as_bool_mut().unwrap() = value;
    }

    pub fn set_audio(&mut self, value: bool) {
        *self.0[2].value.as_bool_mut().unwrap() = value;
    }

    pub fn set_arm7(&mut self, value: Arm7Emu) {
        *self.0[3].value.as_list_mut().unwrap().0 = value as usize
    }

    pub fn set_arm7_block_validation(&mut self, value: bool) {
        *self.0[4].value.as_bool_mut().unwrap() = value;
    }

    pub fn get_all_mut(&mut self) -> &mut [Setting; 6] {
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
