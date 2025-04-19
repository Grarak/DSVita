use ini::Ini;
use std::fmt::{Debug, Display, Formatter};
use std::path::PathBuf;
use std::str::FromStr;
use strum_macros::{EnumIter, EnumString, IntoStaticStr};

#[repr(u8)]
#[derive(Copy, Clone, Debug, EnumIter, EnumString, Eq, IntoStaticStr, PartialEq)]
pub enum Arm7Emu {
    AccurateLle = 0,
    PartialHle = 1,
    PartialSoundHle = 2,
    Hle = 3,
}

impl From<u8> for Arm7Emu {
    fn from(value: u8) -> Self {
        debug_assert!(value <= Arm7Emu::Hle as u8);
        unsafe { std::mem::transmute(value) }
    }
}

#[derive(Clone)]
pub enum SettingValue {
    Bool(bool),
    Arm7Emu(Arm7Emu),
}

impl SettingValue {
    pub fn next(&mut self) {
        *self = match self {
            SettingValue::Bool(value) => SettingValue::Bool(!*value),
            SettingValue::Arm7Emu(value) => SettingValue::Arm7Emu(Arm7Emu::from((value.clone() as u8 + 1) % (Arm7Emu::Hle as u8 + 1))),
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            SettingValue::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_arm7_emu(&self) -> Option<Arm7Emu> {
        match self {
            SettingValue::Arm7Emu(value) => Some(value.clone()),
            _ => None,
        }
    }

    fn parse_str(&mut self, str: &str) {
        match self {
            SettingValue::Bool(value) => *value = bool::from_str(str).unwrap_or(false),
            SettingValue::Arm7Emu(value) => *value = Arm7Emu::from_str(str).unwrap_or(Arm7Emu::AccurateLle),
        }
    }

    fn to_parse_string(&self) -> String {
        match self {
            SettingValue::Bool(value) => value.to_string(),
            SettingValue::Arm7Emu(value) => Into::<&str>::into(value).to_string(),
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
                SettingValue::Arm7Emu(value) => {
                    value.into()
                }
            }
        )
    }
}

#[derive(Clone)]
pub struct Setting {
    pub title: &'static str,
    pub description: &'static str,
    pub value: SettingValue,
}

impl Setting {
    const fn new(title: &'static str, description: &'static str, value: SettingValue) -> Self {
        Setting { title, description, value }
    }
}

pub const DEFAULT_SETTINGS: Settings = Settings {
    values: [
        Setting::new("Rotate Screens", "Will simulate vertical holding, \n\
        for games like Brain Age", SettingValue::Bool(false)),
        Setting::new("Framelimit", "Limits gamespeed to 60fps", SettingValue::Bool(true)),
        Setting::new("Audio", "Disabling audio can give a performance boost", SettingValue::Bool(true)),
        Setting::new(
            "Arm7 Emulation",
            "AccurateLle: Slowest, best compatibility\n\
        PartialHle: Slightly faster, similar compatibility\nto AccurateLle\n\
        PartialSoundHle: ~10%% faster, reduced\ncompatibility\n\
        Hle: ~15-20%% faster, worst compatibility\n\
        Use AccurateLle if game crashes, gets stuck or\nbugs occur.",
            SettingValue::Arm7Emu(Arm7Emu::AccurateLle),
        ),
    ],
};

#[derive(Clone)]
pub struct Settings {
    values: [Setting; 4],
}

impl Settings {
    pub fn rotate_screens(&self) -> bool {
        unsafe { self.values[0].value.as_bool().unwrap_unchecked() }
    }

    pub fn framelimit(&self) -> bool {
        unsafe { self.values[1].value.as_bool().unwrap_unchecked() }
    }

    pub fn audio(&self) -> bool {
        unsafe { self.values[2].value.as_bool().unwrap_unchecked() }
    }

    pub fn arm7_hle(&self) -> Arm7Emu {
        unsafe { self.values[3].value.as_arm7_emu().unwrap_unchecked() }
    }

    pub fn setting_rotate_screens(&mut self) -> &mut Setting {
        &mut self.values[0]
    }
    
    pub fn setting_framelimit_mut(&mut self) -> &mut Setting {
        &mut self.values[1]
    }

    pub fn setting_audio_mut(&mut self) -> &mut Setting {
        &mut self.values[2]
    }

    pub fn setting_arm7_hle_mut(&mut self) -> &mut Setting {
        &mut self.values[3]
    }

    pub fn get_all_mut(&mut self) -> &mut [Setting; 4] {
        &mut self.values
    }
}

impl Debug for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_map();
        for setting in &self.values {
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
