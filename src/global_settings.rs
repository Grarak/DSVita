use crate::key_bindings::KeyBinding;
use crate::screen_layouts::CustomLayout;
use ini::Ini;
use std::path::PathBuf;
use std::{fs, io};

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum MenuLayout {
    List,
    Tiles,
    Carousel,
}

impl MenuLayout {
    fn from_ini(value: &str) -> Self {
        match value {
            "tiles" => MenuLayout::Tiles,
            "carousel" => MenuLayout::Carousel,
            _ => MenuLayout::List,
        }
    }

    fn to_ini(self) -> &'static str {
        match self {
            MenuLayout::List => "list",
            MenuLayout::Tiles => "tiles",
            MenuLayout::Carousel => "carousel",
        }
    }

    pub fn label(self) -> &'static std::ffi::CStr {
        match self {
            MenuLayout::List => c"Menu layout: List",
            MenuLayout::Tiles => c"Menu layout: Tiles",
            MenuLayout::Carousel => c"Menu layout: Carousel",
        }
    }

    pub fn next(self) -> Self {
        match self {
            MenuLayout::List => MenuLayout::Tiles,
            MenuLayout::Tiles => MenuLayout::Carousel,
            MenuLayout::Carousel => MenuLayout::List,
        }
    }
}

pub struct GlobalSettings {
    dir: PathBuf,
    pub custom_layouts: Vec<CustomLayout>,
    pub default_control: KeyBinding,
    pub custom_controls: Vec<KeyBinding>,
    pub ra_username: String,
    pub ra_token: String,
    pub menu_layout: MenuLayout,
}

impl GlobalSettings {
    pub fn new(dir: PathBuf, default_keybinding: KeyBinding) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;

        let mut custom_layouts = Vec::new();

        let custom_layouts_ini_path = dir.join("custom_layouts.ini");
        if let Ok(ini) = Ini::load_from_file(custom_layouts_ini_path) {
            for layout_name in ini.sections() {
                if let Some(layout_name) = layout_name {
                    if let Some(props) = ini.section(Some(layout_name)) {
                        custom_layouts.push(CustomLayout::from_ini(layout_name, props));
                    }
                }
            }
        }

        let mut custom_controls = Vec::new();

        let custom_controls_ini_path = dir.join("custom_controls.ini");
        if let Ok(ini) = Ini::load_from_file(custom_controls_ini_path) {
            for binding_name in ini.sections() {
                if let Some(binding_name) = binding_name {
                    if let Some(props) = ini.section(Some(binding_name)) {
                        custom_controls.push(KeyBinding::from_ini(binding_name, props, default_keybinding.hotkeys));
                    }
                }
            }
        }

        let mut ra_username = "".to_string();
        let mut ra_token = "".to_string();
        let mut menu_layout = MenuLayout::List;
        let settings_path = dir.join("settings.ini");
        if let Ok(ini) = Ini::load_from_file(settings_path) {
            if let Some(props) = ini.section(Some("ra")) {
                if let Some(username) = props.get("username") {
                    ra_username = username.to_string();
                }
                if let Some(token) = props.get("token") {
                    ra_token = token.to_string();
                }
            }
            if let Some(props) = ini.section(Some("menu")) {
                if let Some(layout) = props.get("layout") {
                    menu_layout = MenuLayout::from_ini(layout);
                }
            }
        }

        Ok(GlobalSettings {
            dir,
            custom_layouts,
            default_control: default_keybinding,
            custom_controls,
            ra_username,
            ra_token,
            menu_layout,
        })
    }

    pub fn add_custom_layout(&mut self, custom_layout: CustomLayout) -> bool {
        match self.custom_layouts.iter().find(|layout| layout.name == custom_layout.name) {
            None => {
                self.custom_layouts.push(custom_layout);
                self.flush_custom_layouts();
                true
            }
            Some(_) => false,
        }
    }

    pub fn delete_custom_layout(&mut self, index: usize) {
        self.custom_layouts.remove(index);
        self.flush_custom_layouts();
    }

    fn flush_custom_layouts(&self) {
        let custom_layouts_ini_path = self.dir.join("custom_layouts.ini");
        let mut ini = Ini::new();
        for layout in &self.custom_layouts {
            let mut section_setter = ini.with_section(Some(&layout.name));
            layout.to_ini(&mut section_setter);
        }
        ini.write_to_file(custom_layouts_ini_path).unwrap();
    }

    pub fn get_control(&self, index: usize) -> &KeyBinding {
        if index == 0 {
            &self.default_control
        } else {
            &self.custom_controls[index - 1]
        }
    }

    pub fn add_custom_controls(&mut self, binding: KeyBinding) -> bool {
        match self.custom_controls.iter().find(|b| b.name == binding.name) {
            None => {
                self.custom_controls.push(binding);
                self.flush_custom_controls();
                true
            }
            Some(_) => false,
        }
    }

    pub fn delete_custom_controls(&mut self, index: usize) {
        self.custom_controls.remove(index);
        self.flush_custom_controls();
    }

    fn flush_custom_controls(&self) {
        let custom_controls_ini_path = self.dir.join("custom_controls.ini");
        let mut ini = Ini::new();
        for binding in &self.custom_controls {
            let mut section_setter = ini.with_section(Some(&binding.name));
            binding.to_ini(&mut section_setter);
        }
        ini.write_to_file(custom_controls_ini_path).unwrap();
    }

    pub fn set_ra_data(&mut self, username: String, token: String) {
        self.ra_username = username;
        self.ra_token = token;
        self.flush_settings();
    }

    pub fn set_menu_layout(&mut self, value: MenuLayout) {
        self.menu_layout = value;
        self.flush_settings();
    }

    fn flush_settings(&self) {
        let settings_path = self.dir.join("settings.ini");
        let mut ini = Ini::new();
        ini.with_section(Some("ra")).set("username", &self.ra_username).set("token", &self.ra_token);
        ini.with_section(Some("menu")).set("layout", self.menu_layout.to_ini());
        ini.write_to_file(settings_path).unwrap();
    }
}
