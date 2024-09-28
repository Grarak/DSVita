use ini::Ini;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::str::FromStr;

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

    fn parse_str(&mut self, str: &str) {
        match self {
            SettingValue::Bool(_) => {
                if let Ok(value) = bool::from_str(str) {
                    *self = SettingValue::Bool(value)
                }
            }
        }
    }

    fn to_string(&self) -> String {
        match self {
            SettingValue::Bool(value) => value.to_string(),
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
    pub description: &'static str,
    pub value: SettingValue,
}

impl Setting {
    const fn new(title: &'static str, description: &'static str, value: SettingValue) -> Self {
        Setting { title, description, value }
    }
}

#[derive(Clone)]
pub struct Settings {
    values: [Setting; 3],
}

impl Settings {
    pub fn framelimit(&self) -> bool {
        self.values[0].value.as_bool().unwrap()
    }

    pub fn audio(&self) -> bool {
        self.values[1].value.as_bool().unwrap()
    }

    pub fn arm7_hle(&self) -> bool {
        self.values[2].value.as_bool().unwrap()
    }

    pub fn setting_framelimit_mut(&mut self) -> &mut Setting {
        &mut self.values[0]
    }

    pub fn setting_audio_mut(&mut self) -> &mut Setting {
        &mut self.values[1]
    }

    pub fn setting_arm7_hle_mut(&mut self) -> &mut Setting {
        &mut self.values[2]
    }

    pub fn get_all_mut(&mut self) -> &mut [Setting; 3] {
        &mut self.values
    }
}

pub const DEFAULT_SETTINGS: Settings = Settings {
    values: [
        Setting::new("Framelimit", "Limits gamespeed to 60fps", SettingValue::Bool(true)),
        Setting::new("Audio", "Disabling audio can give a performance boost", SettingValue::Bool(true)),
        Setting::new(
            "Arm7 HLE",
            "Enabling Arm7 HLE increases performance by a\nlot, however at the cost of lower compatibility.\nDisable this if the game gets stuck, doesn't boot\nor crashes",
            SettingValue::Bool(false),
        ),
    ],
};

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
                section.set(setting.title, setting.value.to_string());
            }
            ini.write_to_file(&self.settings_file_path).unwrap();
            self.dirty = false;
        }
    }
}
