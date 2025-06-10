use crate::cartridge_io::{CartridgeIo, CartridgePreview};
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::input::Keycode;
use crate::presenter::platform::imgui::{
    vglGetProcAddress, ImFontAtlas_AddFontFromMemoryTTF, ImFontAtlas_GetGlyphRangesDefault, ImFontConfig, ImFontConfig_ImFontConfig, ImGuiCond__ImGuiSetCond_Always,
    ImGuiFocusedFlags__ImGuiFocusedFlags_ChildWindows, ImGuiHoveredFlags__ImGuiHoveredFlags_Default, ImGuiItemFlags__ImGuiItemFlags_Disabled, ImGuiNavInput__ImGuiNavInput_Cancel,
    ImGuiStyleVar__ImGuiStyleVar_Alpha, ImGuiStyleVar__ImGuiStyleVar_ItemSpacing, ImGuiStyleVar__ImGuiStyleVar_WindowRounding, ImGuiWindowFlags__ImGuiWindowFlags_NoBringToFrontOnFocus,
    ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse, ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing, ImGuiWindowFlags__ImGuiWindowFlags_NoMove, ImGuiWindowFlags__ImGuiWindowFlags_NoResize,
    ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar, ImGui_Begin, ImGui_BeginCombo, ImGui_BeginMainMenuBar, ImGui_Button, ImGui_CreateContext, ImGui_DestroyContext, ImGui_Dummy, ImGui_End,
    ImGui_EndCombo, ImGui_EndMainMenuBar, ImGui_GetContentRegionAvail, ImGui_GetCursorPosX, ImGui_GetDrawData, ImGui_GetIO, ImGui_GetStyle, ImGui_Image, ImGui_ImplVitaGL_GamepadUsage,
    ImGui_ImplVitaGL_Init, ImGui_ImplVitaGL_MouseStickUsage, ImGui_ImplVitaGL_NewFrame, ImGui_ImplVitaGL_RenderDrawData, ImGui_ImplVitaGL_TouchUsage, ImGui_IsItemHovered, ImGui_IsWindowFocused,
    ImGui_PopID, ImGui_PopItemFlag, ImGui_PopStyleVar, ImGui_PushID3, ImGui_PushItemFlag, ImGui_PushStyleVar, ImGui_PushStyleVar1, ImGui_Render, ImGui_SameLine, ImGui_Selectable, ImGui_SetCursorPosX,
    ImGui_SetItemDefaultFocus, ImGui_SetNextWindowPos, ImGui_SetNextWindowSize, ImGui_SetWindowFocus, ImGui_StyleColorsDark, ImGui_Text, ImVec2, ImVec4,
};
use crate::presenter::{PresentEvent, PRESENTER_AUDIO_BUF_SIZE, PRESENTER_AUDIO_SAMPLE_RATE, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH, PRESENTER_SUB_BOTTOM_SCREEN, PRESENTER_SUB_ROTATED_BOTTOM_SCREEN, PRESENTER_SUB_RESIZED_BOTTOM_SCREEN, PRESENTER_SUB_FOCUSED_BOTTOM_SCREEN};
use crate::settings::{Arm7Emu, ScreenMode, SettingValue, Settings, SettingsConfig};
use gl::types::{GLboolean, GLenum, GLuint};
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, mem, ptr};
use strum::IntoEnumIterator;
use vitasdk_sys::*;

mod imgui {
    #![allow(warnings, unused)]
    include!(concat!(env!("OUT_DIR"), "/imgui_bindings.rs"));
}

const ROM_PATH: &str = "ux0:data/dsvita";
const SAVES_PATH: &str = "ux0:data/dsvita/saves";
const SETTINGS_PATH: &str = "ux0:data/dsvita/settings";

#[repr(u8)]
pub enum SharkOpt {
    Slow = 0,
    Safe = 1,
    Default = 2,
    Fast = 3,
    Unsafe = 4,
}

#[link(name = "taihen_stub", kind = "static", modifiers = "+whole-archive")]
#[link(name = "SceShaccCgExt", kind = "static", modifiers = "+whole-archive")]
#[link(name = "mathneon", kind = "static", modifiers = "+whole-archive")]
#[link(name = "vitashark", kind = "static", modifiers = "+whole-archive")]
extern "C" {
    pub fn vglSwapBuffers(has_commondialog: GLboolean);
    pub fn vglSetupRuntimeShaderCompiler(opt_level: c_uint, use_fastmath: c_int, use_fastprecision: c_int, use_fastint: c_int);
    pub fn vglInitExtended(legacy_pool_size: c_int, width: c_int, height: c_int, ram_threshold: c_int, msaa: SceGxmMultisampleMode) -> GLboolean;
    pub fn vglGetTexDataPointer(target: GLenum) -> *mut c_void;
    pub fn vglFree(addr: *mut c_void);
    pub fn vglTexImageDepthBuffer(target: GLenum);
}

const KEY_CODE_MAPPING: [(SceCtrlButtons, Keycode); 12] = [
    (SCE_CTRL_UP, Keycode::Up),
    (SCE_CTRL_DOWN, Keycode::Down),
    (SCE_CTRL_LEFT, Keycode::Left),
    (SCE_CTRL_RIGHT, Keycode::Right),
    (SCE_CTRL_START, Keycode::Start),
    (SCE_CTRL_SELECT, Keycode::Select),
    (SCE_CTRL_CIRCLE, Keycode::A),
    (SCE_CTRL_CROSS, Keycode::B),
    (SCE_CTRL_TRIANGLE, Keycode::X),
    (SCE_CTRL_SQUARE, Keycode::Y),
    (SCE_CTRL_LTRIGGER, Keycode::TriggerL),
    (SCE_CTRL_RTRIGGER, Keycode::TriggerR),
];

#[derive(Clone)]
pub struct PresenterAudio {
    audio_port: c_int,
}

impl PresenterAudio {
    fn new() -> Self {
        unsafe {
            PresenterAudio {
                audio_port: sceAudioOutOpenPort(SCE_AUDIO_OUT_PORT_TYPE_BGM, PRESENTER_AUDIO_BUF_SIZE as _, PRESENTER_AUDIO_SAMPLE_RATE as _, SCE_AUDIO_OUT_MODE_STEREO),
            }
        }
    }

    pub fn play(&self, buffer: &[u32; PRESENTER_AUDIO_BUF_SIZE]) {
        unsafe { sceAudioOutOutput(self.audio_port, buffer.as_slice().as_ptr() as _) };
    }
}

unsafe impl Send for PresenterAudio {}

pub struct Presenter {
    presenter_audio: PresenterAudio,
    keymap: u32,
}

impl Presenter {
    #[cold]
    pub fn new() -> Self {
        unsafe {
            scePowerSetArmClockFrequency(444);
            scePowerSetGpuClockFrequency(222);
            scePowerSetBusClockFrequency(222);
            scePowerSetGpuXbarClockFrequency(166);

            vglSetupRuntimeShaderCompiler(SharkOpt::Unsafe as _, 1, 0, 1);
            // Disable multisampling for depth texture
            vglInitExtended(0, 960, 544, 74 * 1024 * 1024, SCE_GXM_MULTISAMPLE_NONE);
            gl::load_with(|name| {
                let name = CString::new(name).unwrap();
                vglGetProcAddress(name.as_ptr())
            });

            sceTouchSetSamplingState(SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_STOP);

            ImGui_CreateContext(ptr::null_mut());

            ImGui_ImplVitaGL_Init();
            let font = include_bytes!("../../font/OpenSans-Regular.ttf");
            let mut config: ImFontConfig = mem::zeroed();
            ImFontConfig_ImFontConfig(&mut config);
            config.FontDataOwnedByAtlas = false;
            ImFontAtlas_AddFontFromMemoryTTF(
                (*ImGui_GetIO()).Fonts,
                font.as_ptr() as _,
                font.len() as _,
                22f32,
                &config,
                ImFontAtlas_GetGlyphRangesDefault((*ImGui_GetIO()).Fonts),
            );

            let vec = ImVec2 { x: 0f32, y: 2f32 };
            ImGui_PushStyleVar1(ImGuiStyleVar__ImGuiStyleVar_ItemSpacing as _, &vec);
            ImGui_PushStyleVar(ImGuiStyleVar__ImGuiStyleVar_WindowRounding as _, 0f32);
            (*ImGui_GetIO()).MouseDrawCursor = false;
            ImGui_ImplVitaGL_TouchUsage(false);
            ImGui_ImplVitaGL_GamepadUsage(true);
            ImGui_ImplVitaGL_MouseStickUsage(false);
            ImGui_StyleColorsDark(ptr::null_mut());

            Presenter {
                presenter_audio: PresenterAudio::new(),
                keymap: 0xFFFFFFFF,
            }
        }
    }

    pub fn poll_event(&mut self, screenmode: ScreenMode) -> PresentEvent {
        let mut touch = None;

        unsafe {
            let pressed = MaybeUninit::<SceCtrlData>::uninit();
            let mut pressed = pressed.assume_init();
            sceCtrlPeekBufferPositive(0, &mut pressed, 1);

            for (host_key, guest_key) in KEY_CODE_MAPPING {
                if pressed.buttons & host_key != 0 {
                    self.keymap &= !(1 << guest_key as u8);
                } else {
                    self.keymap |= 1 << guest_key as u8;
                }
            }

            let touch_report = MaybeUninit::<SceTouchData>::uninit();
            let mut touch_report = touch_report.assume_init();
            sceTouchPeek(SCE_TOUCH_PORT_FRONT, &mut touch_report, 1);

            if touch_report.reportNum > 0 {
                let report = touch_report.report.first().unwrap();
                let x = report.x as u32 * PRESENTER_SCREEN_WIDTH / 1920;
                let y = report.y as u32 * PRESENTER_SCREEN_HEIGHT / 1080;

                match screenmode {
                    ScreenMode::Regular => {
                        let (x, y) = PRESENTER_SUB_BOTTOM_SCREEN.normalize(x, y);
                        let screen_x = (DISPLAY_WIDTH as u32 * x / PRESENTER_SUB_BOTTOM_SCREEN.width) as u8;
                        let screen_y = (DISPLAY_HEIGHT as u32 * y / PRESENTER_SUB_BOTTOM_SCREEN.height) as u8;
                        touch = Some((screen_x, screen_y));
                    }
                    ScreenMode::Rotated => {
                        if PRESENTER_SUB_ROTATED_BOTTOM_SCREEN.is_within(x, y) {
                            let (x, y) = PRESENTER_SUB_ROTATED_BOTTOM_SCREEN.normalize(x, y);
                            let screen_x = (DISPLAY_WIDTH as u32 - (DISPLAY_WIDTH as u32 * y / PRESENTER_SUB_ROTATED_BOTTOM_SCREEN.height)) as u8;
                            let screen_y = (DISPLAY_HEIGHT as u32 * x / PRESENTER_SUB_ROTATED_BOTTOM_SCREEN.width) as u8;
                            touch = Some((screen_x, screen_y));
                        }
                    }
                    ScreenMode::Resized => {
                        let (x, y) = PRESENTER_SUB_RESIZED_BOTTOM_SCREEN.normalize(x, y);
                        let screen_x = (DISPLAY_WIDTH as u32 * x / PRESENTER_SUB_RESIZED_BOTTOM_SCREEN.width) as u8;
                        let screen_y = (DISPLAY_HEIGHT as u32 * y / PRESENTER_SUB_RESIZED_BOTTOM_SCREEN.height) as u8;
                        touch = Some((screen_x, screen_y));
                    }
                    ScreenMode::Focused => {
                        let (x, y) = PRESENTER_SUB_FOCUSED_BOTTOM_SCREEN.normalize(x, y);
                        let screen_x = (DISPLAY_WIDTH as u32 * x / PRESENTER_SUB_FOCUSED_BOTTOM_SCREEN.width) as u8;
                        let screen_y = (DISPLAY_HEIGHT as u32 * y / PRESENTER_SUB_FOCUSED_BOTTOM_SCREEN.height) as u8;
                        touch = Some((screen_x, screen_y));
                    }
                }
                self.keymap &= !(1 << 16);
            } else {
                self.keymap |= 1 << 16;
            }
        }
        PresentEvent::Inputs { keymap: self.keymap, touch }
    }

    pub fn present_ui(&self) -> (CartridgeIo, Settings) {
        unsafe {
            let _ = fs::create_dir(ROM_PATH);
            let _ = fs::create_dir(SAVES_PATH);
            let _ = fs::create_dir(SETTINGS_PATH);

            let mut params = [0u8; 1024];
            if sceAppMgrGetAppParam(params.as_mut_ptr() as _) == 0 {
                if let Ok(params) = CStr::from_bytes_until_nul(&params) {
                    if let Ok(params) = params.to_str() {
                        if params.contains("psgm:play") {
                            if let Some(pos) = params.find("&param=") {
                                let path = PathBuf::from(&params[pos + 7..]);
                                let name = path.file_name().unwrap().to_str().unwrap();
                                let save_file = PathBuf::from(SAVES_PATH).join(format!("{name}.sav"));
                                let settings_file = PathBuf::from(SETTINGS_PATH).join(format!("{name}.ini"));
                                let preview = CartridgePreview::new(path).unwrap();
                                return (CartridgeIo::from_preview(preview, save_file).unwrap(), SettingsConfig::new(settings_file).settings);
                            }
                        }
                    }
                }
            }

            let mut cartridges: Vec<CartridgePreview> = match fs::read_dir(ROM_PATH) {
                Ok(rom_dir) => rom_dir
                    .into_iter()
                    .filter_map(|dir| dir.ok().and_then(|dir| dir.file_type().ok().and_then(|file_type| if file_type.is_file() { Some(dir) } else { None })))
                    .filter_map(|entry| {
                        let path = entry.path();
                        let name = path.file_name().unwrap().to_str().unwrap();
                        if name.to_lowercase().ends_with(".nds") {
                            // I mistyped the save file extension in 0.3.0
                            // Add migration step
                            let old_save_file = PathBuf::from(SAVES_PATH).join(format!("{name}.nds"));
                            let save_file = PathBuf::from(SAVES_PATH).join(format!("{name}.sav"));
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
                let path = PathBuf::from(SETTINGS_PATH).join(format!("{}.ini", cartridge.file_name));
                settings_configs.push(SettingsConfig::new(path));
            }

            let mut selected = None;

            let mut icon_tex = 0;
            gl::GenTextures(1, &mut icon_tex);
            gl::BindTexture(gl::TEXTURE_2D, icon_tex);
            gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, 32, 32, 0, gl::RGBA, gl::UNSIGNED_BYTE, ptr::null());

            loop {
                let mut hovered = None;

                ImGui_ImplVitaGL_NewFrame();

                if ImGui_BeginMainMenuBar() {
                    let text = if cartridges.is_empty() {
                        format!("No roms found in {ROM_PATH}\0")
                    } else {
                        format!("Found {} roms in {ROM_PATH}\0", cartridges.len())
                    };
                    ImGui_Text(text.as_ptr() as _);
                    ImGui_EndMainMenuBar();
                }

                let vec = ImVec2 { x: 0f32, y: 27f32 };
                let vec2 = ImVec2 { x: 0f32, y: 0f32 };
                ImGui_SetNextWindowPos(&vec, ImGuiCond__ImGuiSetCond_Always as _, &vec2);
                let vec = ImVec2 { x: 553f32, y: 517f32 };
                ImGui_SetNextWindowSize(&vec, ImGuiCond__ImGuiSetCond_Always as _);
                ImGui_Begin(
                    c"##main".as_ptr() as _,
                    ptr::null_mut(),
                    (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoBringToFrontOnFocus
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing) as _,
                );

                if selected.is_none() && !ImGui_IsWindowFocused(ImGuiFocusedFlags__ImGuiFocusedFlags_ChildWindows as _) {
                    ImGui_SetWindowFocus();
                }

                let vec = ImVec2 { x: -1f32, y: 0f32 };
                for (i, cartridge) in cartridges.iter().enumerate() {
                    let name = CString::new(cartridge.file_name.clone()).unwrap();
                    if ImGui_Button(name.as_ptr() as _, &vec) {
                        selected = Some(i);
                    }
                    if ImGui_IsItemHovered(ImGuiHoveredFlags__ImGuiHoveredFlags_Default as _) {
                        hovered = Some(i);
                    }
                }

                ImGui_End();

                let vec = ImVec2 { x: 553f32, y: 27f32 };
                let vec2 = ImVec2 { x: 0f32, y: 0f32 };
                ImGui_SetNextWindowPos(&vec, ImGuiCond__ImGuiSetCond_Always as _, &vec2);
                let vec = ImVec2 { x: 407f32, y: 517f32 };
                ImGui_SetNextWindowSize(&vec, ImGuiCond__ImGuiSetCond_Always as _);
                ImGui_Begin(
                    c"##info".as_ptr() as _,
                    ptr::null_mut(),
                    (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoBringToFrontOnFocus
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
                        | ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing) as _,
                );

                if selected.is_some() {
                    if !ImGui_IsWindowFocused(ImGuiFocusedFlags__ImGuiFocusedFlags_ChildWindows as _) {
                        ImGui_SetWindowFocus();
                    }

                    if (*ImGui_GetIO()).NavInputs[ImGuiNavInput__ImGuiNavInput_Cancel as usize] != 0f32 {
                        selected = None;
                    }
                }

                if let Some(i) = hovered.or(selected) {
                    let cartridge = &cartridges[i];
                    gl::BindTexture(gl::TEXTURE_2D, icon_tex);
                    match cartridge.read_icon() {
                        Ok(icon) => gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, 32, 32, gl::RGBA as _, gl::UNSIGNED_BYTE, icon.as_ptr() as _),
                        Err(_) => {
                            const EMPTY_ICON: [u32; 32 * 32] = [0u32; 32 * 32];
                            gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, 32, 32, gl::RGBA as _, gl::UNSIGNED_BYTE, EMPTY_ICON.as_ptr() as _)
                        }
                    }

                    let size = ImVec2 { x: 128f32, y: 128f32 };
                    let uv0 = ImVec2 { x: 0f32, y: 0f32 };
                    let uv1 = ImVec2 { x: 1f32, y: 1f32 };
                    let tint_color = ImVec4 { x: 1f32, y: 1f32, z: 1f32, w: 1f32 };
                    let border_color = ImVec4 { x: 0f32, y: 0f32, z: 0f32, w: 0f32 };
                    ImGui_Image(icon_tex as _, &size, &uv0, &uv1, &tint_color, &border_color);

                    match cartridge.read_title() {
                        Ok(title) => {
                            let title = CString::new(title).unwrap();
                            ImGui_Text(title.as_ptr() as _);
                        }
                        Err(_) => ImGui_Text(c"Couldn't read game title".as_ptr() as _),
                    }

                    let vec = ImVec2 { x: 0f32, y: 10f32 };
                    ImGui_Dummy(&vec);

                    let vec = ImVec2 { x: -1f32, y: 0f32 };
                    if ImGui_Button(c"Launch game".as_ptr() as _, &vec) {
                        break;
                    }

                    let vec = ImVec2 { x: 0f32, y: 10f32 };
                    ImGui_Dummy(&vec);

                    ImGui_Text(c"Settings".as_ptr() as _);

                    let vec = ImVec2 { x: 0f32, y: 10f32 };
                    ImGui_Dummy(&vec);

                    let settings_config = &mut settings_configs[i];
                    for (i, setting) in settings_config.settings.get_all_mut().iter_mut().enumerate() {
                        let title = CString::new(setting.title).unwrap();

                        ImGui_Text(title.as_ptr() as _);
                        ImGui_SameLine(0f32, -1f32);

                        ImGui_PushID3(i as _);

                        match setting.value {
                            SettingValue::Bool(_) => {
                                ImGui_SetCursorPosX(ImGui_GetCursorPosX() + ImGui_GetContentRegionAvail().x - 50f32);

                                let value = CString::new(setting.value.to_string()).unwrap();
                                let vec = ImVec2 { x: 50f32, y: 0f32 };

                                if ImGui_Button(value.as_ptr() as _, &vec) {
                                    setting.value.next();
                                    settings_config.dirty = true;
                                }
                            }
                            SettingValue::Arm7Emu(_) => {
                                let value = CString::new(setting.value.to_string()).unwrap();

                                ImGui_SetCursorPosX(ImGui_GetCursorPosX() + ImGui_GetContentRegionAvail().x - 125f32);

                                if ImGui_BeginCombo(c"##arm7_emu".as_ptr() as _, value.as_ptr() as _, 0) {
                                    for value in Arm7Emu::iter() {
                                        let is_selected = setting.value.as_arm7_emu() == Some(value);
                                        let value_str: &str = value.into();
                                        let value_cstr = CString::from_str(value_str).unwrap();
                                        let size = ImVec2 { x: 0f32, y: 0f32 };
                                        if ImGui_Selectable(value_cstr.as_ptr() as _, is_selected, 0, &size) {
                                            setting.value = SettingValue::Arm7Emu(value);
                                            settings_config.dirty = true;
                                        }
                                        if is_selected {
                                            ImGui_SetItemDefaultFocus();
                                        }
                                    }
                                    ImGui_EndCombo();
                                }
                            }
                            SettingValue::ScreenMode(_) => {
                                let value = CString::new(setting.value.to_string()).unwrap();

                                ImGui_SetCursorPosX(ImGui_GetCursorPosX() + ImGui_GetContentRegionAvail().x - 125f32);

                                if ImGui_BeginCombo(c"##screenmode".as_ptr() as _, value.as_ptr() as _, 0) {
                                    for value in ScreenMode::iter() {
                                        let is_selected = setting.value.as_screenmode() == Some(value);
                                        let value_str: &str = value.into();
                                        let value_cstr = CString::from_str(value_str).unwrap();
                                        let size = ImVec2 { x: 0f32, y: 0f32 };
                                        if ImGui_Selectable(value_cstr.as_ptr() as _, is_selected, 0, &size) {
                                            setting.value = SettingValue::ScreenMode(value);
                                            settings_config.dirty = true;
                                        }
                                        if is_selected {
                                            ImGui_SetItemDefaultFocus();
                                        }
                                    }
                                    ImGui_EndCombo();
                                }
                            }
                        }

                        ImGui_PopID();

                        let description = CString::new(setting.description).unwrap();
                        ImGui_Text(description.as_ptr() as _);

                        let vec = ImVec2 { x: 0f32, y: 10f32 };
                        ImGui_Dummy(&vec);
                    }

                    let settings_dirty = settings_config.dirty;
                    let vec = ImVec2 { x: -1f32, y: 0f32 };
                    if !settings_dirty {
                        ImGui_PushItemFlag(ImGuiItemFlags__ImGuiItemFlags_Disabled as _, true);
                        ImGui_PushStyleVar(ImGuiStyleVar__ImGuiStyleVar_Alpha as _, (*ImGui_GetStyle()).Alpha * 0.5f32);
                    }
                    if ImGui_Button(c"Save settings".as_ptr() as _, &vec) {
                        settings_config.flush();
                    }
                    if !settings_dirty {
                        ImGui_PopItemFlag();
                        ImGui_PopStyleVar(1);
                    }
                }

                ImGui_End();

                gl::Viewport(0, 0, (*ImGui_GetIO()).DisplaySize.x as _, (*ImGui_GetIO()).DisplaySize.y as _);
                ImGui_Render();
                ImGui_ImplVitaGL_RenderDrawData(ImGui_GetDrawData());
                self.gl_swap_window();
            }

            gl::DeleteTextures(1, &icon_tex);

            let preview = cartridges.remove(selected.unwrap());
            let save_file = PathBuf::from(SAVES_PATH).join(format!("{}.sav", preview.file_name));
            (CartridgeIo::from_preview(preview, save_file).unwrap(), settings_configs.remove(selected.unwrap()).settings)
        }
    }

    pub fn destroy_ui(&self) {
        unsafe {
            ImGui_DestroyContext(ptr::null_mut());

            gl::UseProgram(0);
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::STENCIL_TEST);
            gl::Disable(gl::SCISSOR_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::Disable(gl::BLEND);
        }
    }

    pub fn get_presenter_audio(&self) -> PresenterAudio {
        self.presenter_audio.clone()
    }

    pub fn gl_swap_window(&self) {
        unsafe { vglSwapBuffers(gl::FALSE) };
    }

    pub fn wait_vsync(&self) {
        unsafe { sceDisplayWaitVblankStart() };
    }

    pub unsafe fn gl_create_depth_tex() -> GLuint {
        let mut tex = 0;
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, 1, 1, 0, gl::RGBA, gl::UNSIGNED_BYTE, ptr::null());
        vglFree(vglGetTexDataPointer(gl::TEXTURE_2D));
        vglTexImageDepthBuffer(gl::TEXTURE_2D);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        tex
    }
}
