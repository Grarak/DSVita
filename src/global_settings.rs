use crate::screen_layouts::CustomLayout;
use ini::Ini;
use std::path::PathBuf;
use std::{fs, io};

pub struct GlobalSettings {
    dir: PathBuf,
    pub custom_layouts: Vec<CustomLayout>,
}

impl GlobalSettings {
    pub fn new(dir: PathBuf) -> io::Result<Self> {
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

        Ok(GlobalSettings { dir, custom_layouts })
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
}
