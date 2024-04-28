use std::fmt::{Display, Formatter};

#[derive(Clone)]
pub enum SettingValue {
    Bool(bool),
}

impl SettingValue {
    pub fn next(&mut self) {
        *self = match self {
            SettingValue::Bool(value) => SettingValue::Bool(!*value),
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            SettingValue::Bool(value) => Some(*value),
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
            }
        )
    }
}

#[derive(Clone)]
pub struct Setting {
    pub title: &'static str,
    pub value: SettingValue,
}

impl Setting {
    const fn new(title: &'static str, value: SettingValue) -> Self {
        Setting { title, value }
    }
}

pub type Settings = [Setting; 2];

pub const FRAMESKIP_SETTING: usize = 0;
pub const AUDIO_SETTING: usize = 1;
pub const DEFAULT_SETTINGS: Settings = [
    Setting::new("Frameskip", SettingValue::Bool(false)),
    Setting::new("Audio", SettingValue::Bool(true)),
];

pub fn create_settings_mut() -> Settings {
    DEFAULT_SETTINGS.clone()
}
