use ini::{Properties, SectionSetter};
use std::ffi::CString;

pub const NUM_KEYS: usize = 12;

/// DS button names in editor row order. Used as the ini keys and the editor row
/// labels. Missing ini keys parse to 0 (unbound), so profiles saved before a key
/// existed stay valid.
pub const DS_KEY_NAMES: [&str; NUM_KEYS] = ["A", "B", "X", "Y", "Right", "Left", "Up", "Down", "R", "L", "Select", "Start"];

pub const NUM_HOTKEYS: usize = 7;

/// Actions triggered by holding the PS button together with the bound button.
/// Values index `KeyBinding::hotkeys` and `HOTKEY_NAMES`.
#[repr(usize)]
#[derive(Copy, Clone)]
pub enum Hotkey {
    PreviousLayout = 0,
    NextLayout = 1,
    SwapScreens = 2,
    ScaleTopScreen = 3,
    ScaleBottomScreen = 4,
    BlowMic = 5,
    ToggleLid = 6,
}

/// Hotkey editor row labels and ini keys, indexed by `Hotkey`. Missing ini keys
/// parse to the default binding, so profiles saved before hotkeys were
/// customizable keep the built-in shortcuts.
pub const HOTKEY_NAMES: [&str; NUM_HOTKEYS] = ["Previous layout", "Next layout", "Swap screens", "Scale top screen", "Scale bottom screen", "Blow mic", "Toggle lid"];

/// A named custom controls profile: for each DS key and each hotkey, the host
/// (Vita) button bits that trigger it. Vita-specific in meaning (the values are
/// `SCE_CTRL_*` bits) but stored as plain `u32`, so this module stays
/// platform-agnostic.
#[derive(Clone)]
pub struct KeyBinding {
    pub name: String,
    pub buttons: [u32; NUM_KEYS],
    pub hotkeys: [u32; NUM_HOTKEYS],
}

impl KeyBinding {
    pub fn new(name: String, buttons: [u32; NUM_KEYS], hotkeys: [u32; NUM_HOTKEYS]) -> Self {
        KeyBinding { name, buttons, hotkeys }
    }

    pub fn from_ini(name: &str, props: &Properties, default_hotkeys: [u32; NUM_HOTKEYS]) -> Self {
        let mut buttons = [0u32; NUM_KEYS];
        for (i, key_name) in DS_KEY_NAMES.iter().enumerate() {
            buttons[i] = props.get(*key_name).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        }
        let mut hotkeys = [0u32; NUM_HOTKEYS];
        for (i, hotkey_name) in HOTKEY_NAMES.iter().enumerate() {
            hotkeys[i] = props.get(*hotkey_name).and_then(|v| v.parse::<u32>().ok()).unwrap_or(default_hotkeys[i]);
        }
        KeyBinding {
            name: name.to_string(),
            buttons,
            hotkeys,
        }
    }

    pub fn to_ini(&self, section_setter: &mut SectionSetter) {
        for (i, key_name) in DS_KEY_NAMES.iter().enumerate() {
            section_setter.set(*key_name, self.buttons[i].to_string());
        }
        for (i, hotkey_name) in HOTKEY_NAMES.iter().enumerate() {
            section_setter.set(*hotkey_name, self.hotkeys[i].to_string());
        }
    }

    pub fn name_c_str(&self) -> CString {
        CString::new(self.name.clone()).unwrap_or_default()
    }
}

impl Default for KeyBinding {
    fn default() -> Self {
        KeyBinding {
            name: String::new(),
            buttons: [0; NUM_KEYS],
            hotkeys: [0; NUM_HOTKEYS],
        }
    }
}
