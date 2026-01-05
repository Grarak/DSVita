use crate::cartridge_io::{CartridgeIo, CartridgePreview};
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::presenter::imgui::root::{
    ImDrawData, ImFontAtlas_AddFontFromMemoryTTF, ImFontAtlas_GetGlyphRangesDefault, ImFontConfig, ImFontConfig_ImFontConfig, ImGui, ImGuiCond__ImGuiSetCond_Always,
    ImGuiHoveredFlags__ImGuiHoveredFlags_Default, ImGuiItemFlags__ImGuiItemFlags_Disabled, ImGuiNavInput__ImGuiNavInput_Cancel, ImGuiStyleVar__ImGuiStyleVar_Alpha,
    ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize, ImGuiWindowFlags__ImGuiWindowFlags_NoBringToFrontOnFocus, ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse,
    ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing, ImGuiWindowFlags__ImGuiWindowFlags_NoMove, ImGuiWindowFlags__ImGuiWindowFlags_NoResize, ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar,
    ImVec2, ImVec4,
};
use crate::presenter::{PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use crate::settings::{SettingValue, Settings, SettingsConfig};
use std::ffi::CString;
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, mem, ptr};

pub trait UiBackend {
    fn init(&mut self);
    fn new_frame(&mut self) -> bool;
    fn render_draw_data(&mut self, draw_data: *mut ImDrawData);
    fn swap_window(&mut self);
}

pub fn init_ui(ui_backend: &mut impl UiBackend) {
    unsafe {
        ImGui::CreateContext(ptr::null_mut());
        ImGui::StyleColorsDark(ptr::null_mut());
        ui_backend.init();

        let font = include_bytes!("../../font/OpenSans-Regular.ttf");
        let mut config: ImFontConfig = mem::zeroed();
        ImFontConfig_ImFontConfig(&mut config);
        config.FontDataOwnedByAtlas = false;
        ImFontAtlas_AddFontFromMemoryTTF(
            (*ImGui::GetIO()).Fonts,
            font.as_ptr() as _,
            font.len() as _,
            22f32,
            &config,
            ImFontAtlas_GetGlyphRangesDefault((*ImGui::GetIO()).Fonts),
        );
    }
}

unsafe fn show_settings(settings_config: &mut SettingsConfig, only_runtime: bool) {
    for (i, setting) in settings_config.settings.get_all_mut().iter_mut().enumerate() {
        if only_runtime && !setting.runtime {
            continue;
        }

        let title = CString::new(setting.title).unwrap();

        ImGui::Text(title.as_ptr() as _);
        ImGui::SameLine(0f32, -1f32);

        ImGui::PushID3(i as _);

        match &mut setting.value {
            SettingValue::Bool(_) => {
                ImGui::SetCursorPosX(ImGui::GetCursorPosX() + ImGui::GetContentRegionAvail().x - 50f32);

                let value = CString::new(setting.value.to_string()).unwrap();
                let vec = ImVec2 { x: 50f32, y: 0f32 };

                if ImGui::Button(value.as_ptr() as _, &vec) {
                    setting.value.next();
                    settings_config.dirty = true;
                }
            }
            SettingValue::List(selection, values) => {
                let value = CString::from_str(&values[*selection]).unwrap();

                ImGui::SetCursorPosX(ImGui::GetCursorPosX() + ImGui::GetContentRegionAvail().x - 125f32);

                let id = CString::new(format!("##{i}_list")).unwrap();
                if ImGui::BeginCombo(id.as_ptr() as _, value.as_ptr() as _, 0) {
                    for (i, value) in values.iter().enumerate() {
                        let is_selected = i == *selection;
                        let value_cstr = CString::from_str(value).unwrap();
                        let size = ImVec2 { x: 0f32, y: 0f32 };
                        if ImGui::Selectable(value_cstr.as_ptr() as _, is_selected, 0, &size) {
                            *selection = i;
                            settings_config.dirty = true;
                        }
                        if is_selected {
                            ImGui::SetItemDefaultFocus();
                        }
                    }
                    ImGui::EndCombo();
                }
            }
        }

        ImGui::PopID();

        if !setting.description.is_empty() {
            let description = CString::new(setting.description).unwrap();
            ImGui::Text(description.as_ptr() as _);
        }

        let vec = ImVec2 { x: 0f32, y: 10f32 };
        ImGui::Dummy(&vec);
    }
}

pub fn show_main_menu(cartridge_path: PathBuf, ui_backend: &mut impl UiBackend) -> Option<(CartridgeIo, Settings)> {
    unsafe {
        let saves_path = cartridge_path.join("saves");
        let settings_path = cartridge_path.join("settings");

        let _ = fs::create_dir_all(&cartridge_path);
        let _ = fs::create_dir_all(&saves_path);
        let _ = fs::create_dir_all(&settings_path);

        let mut cartridges: Vec<CartridgePreview> = match fs::read_dir(&cartridge_path) {
            Ok(rom_dir) => rom_dir
                .into_iter()
                .filter_map(|dir| dir.ok().and_then(|dir| dir.file_type().ok().and_then(|file_type| if file_type.is_file() { Some(dir) } else { None })))
                .filter_map(|entry| {
                    let path = entry.path();
                    let name = path.file_name().unwrap().to_str().unwrap();
                    if name.to_lowercase().ends_with(".nds") {
                        // I mistyped the save file extension in 0.3.0
                        // Add migration step
                        let old_save_file = saves_path.join(format!("{name}.nds"));
                        let save_file = saves_path.join(format!("{name}.sav"));
                        if old_save_file.exists() {
                            if save_file.exists() {
                                let _ = fs::remove_file(old_save_file);
                            } else {
                                let _ = fs::rename(old_save_file, &save_file);
                            }
                        }
                        CartridgePreview::new(path).ok()
                    } else {
                        None
                    }
                })
                .collect(),
            Err(_) => Vec::new(),
        };

        cartridges.sort_by(|a, b| a.file_name.cmp(&b.file_name));

        let mut settings_configs = Vec::new();
        for cartridge in &cartridges {
            let path = settings_path.join(format!("{}.ini", cartridge.file_name));
            settings_configs.push(SettingsConfig::new(path));
        }

        let mut hovered: Option<usize> = None;
        static mut SELECTED: Option<usize> = None;
        let mut launched = false;

        let mut icon_tex = 0;
        gl::GenTextures(1, &mut icon_tex);
        gl::BindTexture(gl::TEXTURE_2D, icon_tex);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, 32, 32, 0, gl::RGBA, gl::UNSIGNED_BYTE, ptr::null());

        while !launched {
            if !ui_backend.new_frame() {
                return None;
            }

            if let Some(i) = SELECTED.or(hovered) {
                let cartridge: &CartridgePreview = &cartridges[i];
                gl::BindTexture(gl::TEXTURE_2D, icon_tex);
                match cartridge.read_icon() {
                    Ok(icon) => gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, 32, 32, gl::RGBA as _, gl::UNSIGNED_BYTE, icon.as_ptr() as _),
                    Err(_) => {
                        const EMPTY_ICON: [u32; 32 * 32] = [0u32; 32 * 32];
                        gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, 32, 32, gl::RGBA as _, gl::UNSIGNED_BYTE, EMPTY_ICON.as_ptr() as _)
                    }
                }
            }

            if ImGui::BeginMainMenuBar() {
                let text = if cartridges.is_empty() {
                    format!("No roms found in {}", cartridge_path.to_str().unwrap())
                } else {
                    format!("Found {} roms in {}", cartridges.len(), cartridge_path.to_str().unwrap())
                };
                let text = CString::from_str(&text).unwrap();
                ImGui::Text(text.as_ptr() as _);

                ImGui::EndMainMenuBar();
            }

            let vec = ImVec2 { x: 0.0, y: 27.0 };
            let vec2 = ImVec2 { x: 0.0, y: 0.0 };
            ImGui::SetNextWindowPos(&vec, ImGuiCond__ImGuiSetCond_Always as _, &vec2);
            let vec = ImVec2 { x: 700.0, y: 517.0 };
            ImGui::SetNextWindowSize(&vec, ImGuiCond__ImGuiSetCond_Always as _);
            if ImGui::Begin(
                c"##main".as_ptr() as _,
                ptr::null_mut(),
                (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse) as _,
            ) {
                let vec = ImVec2 { x: -1f32, y: 0f32 };
                for (i, cartridge) in cartridges.iter().enumerate() {
                    let name = CString::new(cartridge.file_name.clone()).unwrap();
                    if ImGui::Button(name.as_ptr() as _, &vec) {
                        SELECTED = Some(i);
                    }
                    if ImGui::IsItemHovered(ImGuiHoveredFlags__ImGuiHoveredFlags_Default as _) {
                        hovered = Some(i);
                    }
                }
            }

            ImGui::End();

            let vec = ImVec2 { x: 700.0, y: 27.0 };
            let vec2 = ImVec2 { x: 0.0, y: 0.0 };
            ImGui::SetNextWindowPos(&vec, ImGuiCond__ImGuiSetCond_Always as _, &vec2);
            let vec = ImVec2 { x: 260.0, y: 517.0 };
            ImGui::SetNextWindowSize(&vec, ImGuiCond__ImGuiSetCond_Always as _);
            if ImGui::Begin(
                c"##info".as_ptr() as _,
                ptr::null_mut(),
                (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoBringToFrontOnFocus
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing) as _,
            ) {
                if let Some(i) = hovered {
                    let cartridge = &cartridges[i];

                    let size = ImVec2 { x: 128f32, y: 128f32 };
                    let uv0 = ImVec2 { x: 0f32, y: 0f32 };
                    let uv1 = ImVec2 { x: 1f32, y: 1f32 };
                    let tint_color = ImVec4 { x: 1f32, y: 1f32, z: 1f32, w: 1f32 };
                    let border_color = ImVec4 { x: 0f32, y: 0f32, z: 0f32, w: 0f32 };
                    ImGui::Image(icon_tex as _, &size, &uv0, &uv1, &tint_color, &border_color);

                    match cartridge.read_title() {
                        Ok(title) => {
                            let title = CString::new(title).unwrap();
                            ImGui::Text(title.as_ptr() as _);
                        }
                        Err(_) => ImGui::Text(c"Couldn't read game title".as_ptr() as _),
                    }
                }
            }

            ImGui::End();

            if let Some(i) = SELECTED {
                let vec = ImVec2 { x: 0.0, y: 0.0 };
                let vec2 = ImVec2 { x: 0.0, y: 0.0 };
                ImGui::SetNextWindowPos(&vec, ImGuiCond__ImGuiSetCond_Always as _, &vec2);
                let vec = ImVec2 { x: 960.0, y: 544.0 };
                ImGui::SetNextWindowSize(&vec, ImGuiCond__ImGuiSetCond_Always as _);
                if ImGui::Begin(
                    c"##details".as_ptr() as _,
                    ptr::null_mut(),
                    (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse) as _,
                ) {
                    let cartridge = &cartridges[i];

                    let size = ImVec2 { x: 128f32, y: 128f32 };
                    let uv0 = ImVec2 { x: 0f32, y: 0f32 };
                    let uv1 = ImVec2 { x: 1f32, y: 1f32 };
                    let bg_color = ImVec4 { x: 0f32, y: 0f32, z: 0f32, w: 0f32 };
                    let tint_color = ImVec4 { x: 1f32, y: 1f32, z: 1f32, w: 1f32 };
                    ImGui::ImageButton(icon_tex as _, &size, &uv0, &uv1, 0, &bg_color, &tint_color);

                    match cartridge.read_title() {
                        Ok(title) => {
                            let title = CString::new(title).unwrap();
                            ImGui::Text(title.as_ptr() as _);
                        }
                        Err(_) => ImGui::Text(c"Couldn't read game title".as_ptr() as _),
                    }

                    let vec = ImVec2 { x: 0f32, y: 10f32 };
                    ImGui::Dummy(&vec);

                    ImGui::Text(c"First launch will take some time. Please do not exit or shutdown your vita!".as_ptr());

                    let vec = ImVec2 { x: -1f32, y: 0f32 };
                    if ImGui::Button(c"Launch game".as_ptr() as _, &vec) {
                        launched = true;
                    }

                    let vec = ImVec2 { x: 0f32, y: 10f32 };
                    ImGui::Dummy(&vec);

                    ImGui::Text(c"Settings".as_ptr() as _);

                    let vec = ImVec2 { x: 0f32, y: 10f32 };
                    ImGui::Dummy(&vec);

                    let settings_config = &mut settings_configs[i];
                    show_settings(settings_config, false);

                    let settings_dirty = settings_config.dirty;
                    let vec = ImVec2 { x: -1f32, y: 0f32 };
                    if !settings_dirty {
                        ImGui::PushItemFlag(ImGuiItemFlags__ImGuiItemFlags_Disabled as _, true);
                        ImGui::PushStyleVar(ImGuiStyleVar__ImGuiStyleVar_Alpha as _, (*ImGui::GetStyle()).Alpha * 0.5f32);
                    }
                    if ImGui::Button(c"Save settings".as_ptr() as _, &vec) {
                        settings_config.flush();
                    }
                    if !settings_dirty {
                        ImGui::PopItemFlag();
                        ImGui::PopStyleVar(1);
                    }

                    if (*ImGui::GetIO()).NavInputs[ImGuiNavInput__ImGuiNavInput_Cancel as usize] != 0f32 {
                        hovered = SELECTED;
                        SELECTED = None;
                    }
                }

                ImGui::End();
            }

            let io = ImGui::GetIO();
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, (*io).DisplaySize.x as _, (*io).DisplaySize.y as _);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            ImGui::Render();

            ui_backend.render_draw_data(ImGui::GetDrawData());
            ui_backend.swap_window();
        }

        gl::DeleteTextures(1, &icon_tex);

        let preview = cartridges.remove(SELECTED.unwrap());
        let save_file = saves_path.join(format!("{}.sav", preview.file_name));
        Some((CartridgeIo::from_preview(preview, save_file).unwrap(), settings_configs.remove(SELECTED.unwrap()).settings))
    }
}

pub enum UiPauseMenuReturn {
    Resume,
    Quit,
    QuitApp,
}

pub fn show_pause_menu(ui_backend: &mut impl UiBackend, gpu_renderer: &GpuRenderer, settings: &mut Settings) -> UiPauseMenuReturn {
    let mut pressed_settings = false;
    let mut pressed_quit = false;
    let mut pressed_exit = false;
    let mut return_value = None;
    let mut settings_config = SettingsConfig::from(settings.clone());
    loop {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
            gl::ClearColor(0f32, 0f32, 0f32, 1f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gpu_renderer.blit_main_framebuffer();

            if !ui_backend.new_frame() {
                return UiPauseMenuReturn::QuitApp;
            }

            if ImGui::BeginPopupModal(
                c"PausePopup".as_ptr(),
                ptr::null_mut(),
                (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
                    | ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize) as _,
            ) {
                let vec = ImVec2 { x: 100.0, y: 50.0 };
                if ImGui::Button(c"Settings".as_ptr(), &vec) {
                    pressed_settings = true;
                    ImGui::CloseCurrentPopup();
                }
                ImGui::SameLine(0.0, 5.0);
                if ImGui::Button(c"Quit game".as_ptr(), &vec) {
                    pressed_quit = true;
                    ImGui::CloseCurrentPopup();
                }
                let vec = ImVec2 { x: 100.0, y: 50.0 };
                if ImGui::Button(c"Resume".as_ptr(), &vec) {
                    return_value = Some(UiPauseMenuReturn::Resume);
                    ImGui::CloseCurrentPopup();
                }
                ImGui::SameLine(0.0, 5.0);
                if ImGui::Button(c"Exit emu".as_ptr(), &vec) {
                    pressed_exit = true;
                    ImGui::CloseCurrentPopup();
                }

                ImGui::EndPopup();
            }

            if ImGui::BeginPopupModal(
                c"QuitPopup".as_ptr(),
                ptr::null_mut(),
                (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                    | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
                    | ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize) as _,
            ) {
                let text = c"Exit the game? Unsaved progress will be lost.";
                let text_width = ImGui::CalcTextSize(text.as_ptr(), ptr::null(), false, 0.0).x;
                ImGui::Text(text.as_ptr());
                let vec = ImVec2 { x: text_width / 2.0, y: 50.0 };
                if ImGui::Button(c"No".as_ptr(), &vec) {
                    pressed_quit = false;
                    pressed_exit = false;
                    ImGui::CloseCurrentPopup();
                }
                ImGui::SameLine(0.0, 5.0);
                if ImGui::Button(c"Yes".as_ptr(), &vec) {
                    return_value = if pressed_exit { Some(UiPauseMenuReturn::QuitApp) } else { Some(UiPauseMenuReturn::Quit) };
                    ImGui::CloseCurrentPopup();
                }

                ImGui::EndPopup();
            }

            if return_value.is_none() {
                if pressed_settings {
                    let vec = ImVec2 { x: 0.0, y: 0.0 };
                    let vec2 = ImVec2 { x: 0.0, y: 0.0 };
                    ImGui::SetNextWindowPos(&vec, ImGuiCond__ImGuiSetCond_Always as _, &vec2);
                    let vec = ImVec2 {
                        x: PRESENTER_SCREEN_WIDTH as f32,
                        y: PRESENTER_SCREEN_HEIGHT as f32,
                    };
                    ImGui::SetNextWindowSize(&vec, ImGuiCond__ImGuiSetCond_Always as _);
                    if ImGui::Begin(
                        c"##details".as_ptr() as _,
                        ptr::null_mut(),
                        (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                            | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                            | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                            | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse) as _,
                    ) {
                        show_settings(&mut settings_config, true);

                        if (*ImGui::GetIO()).NavInputs[ImGuiNavInput__ImGuiNavInput_Cancel as usize] != 0f32 {
                            pressed_settings = false;
                        }
                    }
                    ImGui::End();
                } else if pressed_quit || pressed_exit {
                    ImGui::OpenPopup(c"QuitPopup".as_ptr());
                } else {
                    ImGui::OpenPopup(c"PausePopup".as_ptr());
                }
            }

            ImGui::Render();

            ui_backend.render_draw_data(ImGui::GetDrawData());
            ui_backend.swap_window();

            if let Some(return_value) = return_value {
                if settings_config.dirty {
                    *settings = settings_config.settings;
                }
                return return_value;
            }
        }
    }
}

pub fn show_progress(ui_backend: &mut impl UiBackend, current_name: impl AsRef<str>, progress: usize, total: usize) {
    unsafe {
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
        gl::ClearColor(0f32, 0f32, 0f32, 1f32);
        gl::Clear(gl::COLOR_BUFFER_BIT);

        ui_backend.new_frame();

        if ImGui::BeginPopupModal(
            c"ProgressPopup".as_ptr(),
            ptr::null_mut(),
            (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
                | ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize) as _,
        ) {
            let text = CString::from_str(current_name.as_ref()).unwrap();
            ImGui::Text(text.as_ptr());
            let vec = ImVec2 { x: 400.0, y: 45.0 };
            ImGui::ProgressBar(progress as f32 / total as f32, &vec, ptr::null());

            ImGui::EndPopup();
        }

        ImGui::OpenPopup(c"ProgressPopup".as_ptr());

        ImGui::Render();

        ui_backend.render_draw_data(ImGui::GetDrawData());
        ui_backend.swap_window();
    }
}
