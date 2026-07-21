use crate::cartridge_io::{CartridgeIo, CartridgePreview};
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::core::input::Keycode;
use crate::global_settings::GlobalSettings;
use crate::key_bindings::{Hotkey, KeyBinding, DS_KEY_NAMES, HOTKEY_NAMES, NUM_HOTKEYS, NUM_KEYS};
use crate::logging::info_println;
use crate::presenter::imgui::root::{
    ImDrawData, ImGui, ImGuiCol__ImGuiCol_Text, ImGui_ImplVitaGL_GamepadUsage, ImGui_ImplVitaGL_Init, ImGui_ImplVitaGL_MouseStickUsage, ImGui_ImplVitaGL_NewFrame, ImGui_ImplVitaGL_RenderDrawData,
    ImGui_ImplVitaGL_TouchUsage, ImVec2,
};
use crate::presenter::ui::{draw_layout_preview, draw_overlay_picker, init_ui, show_main_menu, show_pause_menu, show_progress, CustomLayoutContext, RALoginContext, UiBackend, UiPauseMenuReturn};
use crate::presenter::{
    cjk_font, PresentEvent, PRESENTER_AUDIO_IN_BUF_SIZE, PRESENTER_AUDIO_IN_SAMPLE_RATE, PRESENTER_AUDIO_OUT_BUF_SIZE, PRESENTER_AUDIO_OUT_SAMPLE_RATE, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH,
};
use crate::ra_context::RaContext;
use crate::screen_layouts::{CustomLayout, ScreenLayouts};
use crate::settings::{Settings, SettingsConfig};
use gl::types::{GLenum, GLuint};
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::path::PathBuf;
use std::str::FromStr;
use std::{env, mem, ptr};
use vita_gl::{SharkOpt, VglMemType};
use vitasdk_sys::*;

mod imgui {
    #![allow(warnings, unused)]
    include!(concat!(env!("OUT_DIR"), "/imgui_bindings.rs"));
}

const ROM_PATH: &str = "ux0:data/dsvita";
pub const LOG_PATH: &str = "ux0:data/dsvita/log";
pub const LOG_FILE: &str = "ux0:data/dsvita/log/log.txt";

#[link(name = "taihen_stub", kind = "static", modifiers = "+whole-archive")]
#[link(name = "SceShaccCgExt", kind = "static", modifiers = "+whole-archive")]
#[link(name = "mathneon", kind = "static", modifiers = "+whole-archive")]
#[link(name = "vitashark", kind = "static", modifiers = "+whole-archive")]
// #[link(name = "SceRazorHud_stub", kind = "static", modifiers = "+whole-archive")]
// #[link(name = "ScePerf_stub", kind = "static", modifiers = "+whole-archive")]
extern "C" {
    // pub fn sceRazorCpuPushMarkerWithHud(label: *const c_char, color: c_int, flags: c_int) -> c_int;
    // pub fn sceRazorCpuPopMarker() -> c_int;
    pub fn udcd_uvc_dsvita_sendCustomFrame(data: *const c_void);
}

fn sce_common_dialog_set_magic_number(param: &mut SceCommonDialogParam) {
    param.magic = SCE_COMMON_DIALOG_MAGIC_NUMBER + param as *mut _ as usize as u32;
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

const BUTTONS_TO_SAMPLE: SceCtrlButtons = {
    let mut mask = 0;
    let mut i = 0;
    while i < KEY_CODE_MAPPING.len() {
        mask |= KEY_CODE_MAPPING[i].0;
        i += 1;
    }
    mask
} | SCE_CTRL_PSBUTTON;

/// `Keycode` for each editor row, parallel to `key_bindings::DS_KEY_NAMES`. A
/// binding's `buttons` array is indexed by editor-row position (the display
/// order), NOT by `Keycode` value, so this table maps row -> Keycode.
const DISPLAY_KEYCODES: [Keycode; NUM_KEYS] = [
    Keycode::A,
    Keycode::B,
    Keycode::X,
    Keycode::Y,
    Keycode::Right,
    Keycode::Left,
    Keycode::Up,
    Keycode::Down,
    Keycode::TriggerR,
    Keycode::TriggerL,
    Keycode::Select,
    Keycode::Start,
];

/// Default button per hotkey (indexed by `Hotkey`), triggered while the PS
/// button is held. Blow mic and lid toggle default to unbound.
const DEFAULT_HOTKEY_MAPPING: [u32; NUM_HOTKEYS] = [SCE_CTRL_LTRIGGER, SCE_CTRL_RTRIGGER, SCE_CTRL_CROSS, SCE_CTRL_SQUARE, SCE_CTRL_CIRCLE, 0, 0];

/// Default Vita button per editor row (display order, matches `DS_KEY_NAMES`).
const DEFAULT_KEY_MAPPING: [u32; NUM_KEYS] = {
    let mut mapping = [0u32; NUM_KEYS];
    let mut row = 0;
    while row < NUM_KEYS {
        let keycode = DISPLAY_KEYCODES[row] as usize;
        let mut j = 0;
        while j < KEY_CODE_MAPPING.len() {
            if KEY_CODE_MAPPING[j].1 as usize == keycode {
                mapping[row] = KEY_CODE_MAPPING[j].0;
            }
            j += 1;
        }
        row += 1;
    }
    mapping
};

/// The Vita buttons a DS key can be bound to, shown in the controls editor.
const BINDABLE_BUTTONS: [(&CStr, u32); 12] = [
    (c"Circle", SCE_CTRL_CIRCLE),
    (c"Cross", SCE_CTRL_CROSS),
    (c"Triangle", SCE_CTRL_TRIANGLE),
    (c"Square", SCE_CTRL_SQUARE),
    (c"L", SCE_CTRL_LTRIGGER),
    (c"R", SCE_CTRL_RTRIGGER),
    (c"Up", SCE_CTRL_UP),
    (c"Down", SCE_CTRL_DOWN),
    (c"Left", SCE_CTRL_LEFT),
    (c"Right", SCE_CTRL_RIGHT),
    (c"Start", SCE_CTRL_START),
    (c"Select", SCE_CTRL_SELECT),
];

/// A fresh controls profile seeded with the default mapping (for the editor).
pub fn default_key_binding() -> KeyBinding {
    KeyBinding {
        name: String::new(),
        buttons: DEFAULT_KEY_MAPPING,
        hotkeys: DEFAULT_HOTKEY_MAPPING,
    }
}

#[derive(Clone)]
pub struct PresenterAudioOut {
    audio_port: c_int,
}

impl PresenterAudioOut {
    fn new() -> Self {
        unsafe {
            PresenterAudioOut {
                audio_port: sceAudioOutOpenPort(
                    SCE_AUDIO_OUT_PORT_TYPE_BGM,
                    PRESENTER_AUDIO_OUT_BUF_SIZE as _,
                    PRESENTER_AUDIO_OUT_SAMPLE_RATE as _,
                    SCE_AUDIO_OUT_MODE_STEREO,
                ),
            }
        }
    }

    pub fn play(&self, buffer: &[u32; PRESENTER_AUDIO_OUT_BUF_SIZE]) {
        unsafe { sceAudioOutOutput(self.audio_port, buffer.as_ptr() as _) };
    }
}

unsafe impl Send for PresenterAudioOut {}

#[derive(Clone)]
pub struct PresenterAudioIn {
    audio_port: c_int,
}

impl PresenterAudioIn {
    fn new() -> Self {
        unsafe {
            PresenterAudioIn {
                audio_port: sceAudioInOpenPort(
                    SCE_AUDIO_IN_PORT_TYPE_VOICE,
                    PRESENTER_AUDIO_IN_BUF_SIZE as _,
                    PRESENTER_AUDIO_IN_SAMPLE_RATE as _,
                    SCE_AUDIO_IN_PARAM_FORMAT_S16_MONO,
                ),
            }
        }
    }

    pub fn receive(&self, buffer: &mut [i16; PRESENTER_AUDIO_IN_BUF_SIZE]) {
        unsafe { sceAudioInInput(self.audio_port, buffer.as_mut_ptr() as _) };
    }
}

unsafe impl Send for PresenterAudioIn {}

pub struct Presenter {
    presenter_audio_out: PresenterAudioOut,
    presenter_audio_in: PresenterAudioIn,
    touch_points: Option<(i16, i16)>,
    keymap: u32,
    key_mapping: [u32; NUM_KEYS],
    hotkey_mapping: [u32; NUM_HOTKEYS],
    pressed_btn: u32,
    do_nothing_until_all_btns_released: bool,
    ps_hotkey_used: bool,
    core_unlocked: bool,
    can_stream_screen: bool,
}

impl Presenter {
    fn module_installed(name: &str) -> bool {
        let name = CString::from_str(name).unwrap();
        let search_unk = [0u32; 2];
        unsafe { _vshKernelSearchModuleByName(name.as_ptr(), search_unk.as_ptr() as _) >= 0 }
    }

    #[cold]
    pub fn new() -> Option<Self> {
        unsafe {
            env::set_var("SSL_CERT_FILE", "vs0:data/external/cert/CA_LIST.cer");

            info_println!("Set clocks");
            scePowerSetArmClockFrequency(444);
            scePowerSetGpuClockFrequency(222);
            scePowerSetBusClockFrequency(222);
            scePowerSetGpuXbarClockFrequency(166);

            sceShellUtilInitEvents(0);

            let mut init_params: SceAppUtilInitParam = mem::zeroed();
            let mut init_boot_params: SceAppUtilBootParam = mem::zeroed();
            sceAppUtilInit(&mut init_params, &mut init_boot_params);

            let param: SceCommonDialogConfigParam = mem::zeroed();
            sceCommonDialogSetConfigParam(&param);

            info_println!("Set shader compiler arguments");
            vita_gl::vglSetupRuntimeShaderCompiler(SharkOpt::Fast as _, 1, 0, 1);
            info_println!("Initialize vitaGL");
            // Disable multisampling for depth texture
            vita_gl::vglInitExtended(0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _, 75 * 1024 * 1024, SCE_GXM_MULTISAMPLE_NONE);

            info_println!("Checking for kubridge");

            if !Self::module_installed("kubridge") {
                let mut msg_param: SceMsgDialogUserMessageParam = mem::zeroed();
                msg_param.buttonType = SCE_MSG_DIALOG_BUTTON_TYPE_OK as _;
                msg_param.msg = c"Kubridge not installed, get version 0.3.1 from https://github.com/bythos14/kubridge/releases and put in under the *KERNEL section in config.txt!".as_ptr();

                let mut param: SceMsgDialogParam = mem::zeroed();
                sce_common_dialog_set_magic_number(&mut param.commonParam);
                param.sdkVersion = PSP2_SDK_VERSION;
                param.mode = SCE_MSG_DIALOG_MODE_USER_MSG as _;
                param.userMsgParam = ptr::addr_of_mut!(msg_param);

                sceMsgDialogInit(&param);

                while sceMsgDialogGetStatus() != SCE_COMMON_DIALOG_STATUS_FINISHED {
                    vita_gl::vglSwapBuffers(gl::TRUE);
                }
                sceMsgDialogTerm();
                return None;
            }

            gl::load_with(|name| {
                let name = CString::new(name).unwrap();
                vita_gl::vglGetProcAddress(name.as_ptr() as _) as _
            });

            sceTouchSetSamplingState(SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_STOP);

            let has_cap_unlocker = Self::module_installed("CapUnlocker");
            let mut instance = Presenter {
                presenter_audio_out: PresenterAudioOut::new(),
                presenter_audio_in: PresenterAudioIn::new(),
                touch_points: None,
                keymap: 0xFFFFFFFF,
                key_mapping: DEFAULT_KEY_MAPPING,
                hotkey_mapping: DEFAULT_HOTKEY_MAPPING,
                pressed_btn: 0,
                do_nothing_until_all_btns_released: false,
                ps_hotkey_used: false,
                core_unlocked: has_cap_unlocker,
                can_stream_screen: has_cap_unlocker && Self::module_installed("udcd_uvc_dsvita"),
            };

            init_ui(&mut instance);
            Some(instance)
        }
    }

    pub fn get_default_key_mapping() -> [u32; NUM_KEYS] {
        DEFAULT_KEY_MAPPING
    }

    pub fn get_default_hotkey_mapping() -> [u32; NUM_HOTKEYS] {
        DEFAULT_HOTKEY_MAPPING
    }

    pub fn set_key_mapping(&mut self, binding: &KeyBinding) {
        self.key_mapping = binding.buttons;
        self.hotkey_mapping = binding.hotkeys;
    }

    pub fn get_savestate_path(&self) -> Option<std::path::PathBuf> {
        None
    }

    pub fn poll_event(&mut self, settings: &Settings) -> PresentEvent {
        let mut stick_keymap = 0xFFFFFFFF;

        unsafe {
            let pressed = MaybeUninit::<SceCtrlData>::uninit();
            let mut pressed = pressed.assume_init();
            sceCtrlPeekBufferPositive(0, &mut pressed, 1);
            pressed.buttons &= BUTTONS_TO_SAMPLE;

            let mut previous_pressed_btn = self.pressed_btn;
            self.pressed_btn = pressed.buttons;

            let touch_report = MaybeUninit::<SceTouchData>::uninit();
            let mut touch_report = touch_report.assume_init();
            sceTouchPeek(SCE_TOUCH_PORT_FRONT, &mut touch_report, 1);

            if touch_report.reportNum > 0 {
                let report = touch_report.report.first().unwrap();
                let x = report.x as u32 * PRESENTER_SCREEN_WIDTH / 1920;
                let y = report.y as u32 * PRESENTER_SCREEN_HEIGHT / 1080;
                self.touch_points = Some((x as i16, y as i16));
            } else {
                self.touch_points = None;
            }

            // Virtual hotkey bits re-arm every poll; the PS layer below pulses them
            self.keymap |= (1 << Keycode::BlowMic as u8) | (1 << Keycode::Lid as u8);

            if pressed.buttons & SCE_CTRL_PSBUTTON != 0 {
                // A held PS button is a hotkey modifier layer: DS keys are suppressed
                self.keymap = 0xFFFFFFFF;

                let held = |btn: u32| btn != 0 && pressed.buttons & btn == btn;
                let released = |btn: u32| btn != 0 && previous_pressed_btn & btn == btn && pressed.buttons & btn != btn;

                const LAYOUT_HOTKEYS: [(Hotkey, PresentEvent); 5] = [
                    (
                        Hotkey::PreviousLayout,
                        PresentEvent::CycleScreenLayout {
                            offset: -1,
                            swap: false,
                            top_screen_scale_offset: 0,
                            bottom_screen_scale_offset: 0,
                        },
                    ),
                    (
                        Hotkey::NextLayout,
                        PresentEvent::CycleScreenLayout {
                            offset: 1,
                            swap: false,
                            top_screen_scale_offset: 0,
                            bottom_screen_scale_offset: 0,
                        },
                    ),
                    (
                        Hotkey::SwapScreens,
                        PresentEvent::CycleScreenLayout {
                            offset: 0,
                            swap: true,
                            top_screen_scale_offset: 0,
                            bottom_screen_scale_offset: 0,
                        },
                    ),
                    (
                        Hotkey::ScaleTopScreen,
                        PresentEvent::CycleScreenLayout {
                            offset: 0,
                            swap: false,
                            top_screen_scale_offset: 1,
                            bottom_screen_scale_offset: 0,
                        },
                    ),
                    (
                        Hotkey::ScaleBottomScreen,
                        PresentEvent::CycleScreenLayout {
                            offset: 0,
                            swap: false,
                            top_screen_scale_offset: 0,
                            bottom_screen_scale_offset: 1,
                        },
                    ),
                ];

                for (hotkey, event) in LAYOUT_HOTKEYS {
                    if released(self.hotkey_mapping[hotkey as usize]) {
                        self.do_nothing_until_all_btns_released = true;
                        return event;
                    }
                }

                if held(self.hotkey_mapping[Hotkey::BlowMic as usize]) {
                    self.keymap &= !(1 << Keycode::BlowMic as u8);
                    self.ps_hotkey_used = true;
                }
                if released(self.hotkey_mapping[Hotkey::ToggleLid as usize]) {
                    // One pulsed poll; the core toggles the hinge on this edge
                    self.keymap &= !(1 << Keycode::Lid as u8);
                    self.ps_hotkey_used = true;
                }

                return PresentEvent::Inputs {
                    keymap: self.keymap,
                    touch: self.touch_points,
                    #[cfg(debug_assertions)]
                    debug_touch: None,
                };
            }

            if self.do_nothing_until_all_btns_released {
                if pressed.buttons == 0 {
                    previous_pressed_btn = 0;
                    self.do_nothing_until_all_btns_released = false;
                } else {
                    return PresentEvent::Inputs {
                        keymap: 0xFFFFFFFF,
                        touch: None,
                        #[cfg(debug_assertions)]
                        debug_touch: None,
                    };
                }
            }

            if previous_pressed_btn & SCE_CTRL_PSBUTTON != 0 && pressed.buttons & SCE_CTRL_PSBUTTON == 0 {
                let used = self.ps_hotkey_used;
                self.ps_hotkey_used = false;
                if used {
                    // A hold-type hotkey fired during this PS press; swallow the
                    // pause and ignore the still-held combo buttons until released
                    self.do_nothing_until_all_btns_released = true;
                } else {
                    return PresentEvent::Pause;
                }
            }

            // All bits of a binding must be held, so hand-edited ini profiles can bind
            // button combinations; editor-made single-button bindings behave as before
            for (row, &host_key) in self.key_mapping.iter().enumerate() {
                let guest_key = DISPLAY_KEYCODES[row] as usize;
                if host_key != 0 && pressed.buttons & host_key == host_key {
                    self.keymap &= !(1 << guest_key);
                } else {
                    self.keymap |= 1 << guest_key;
                }
            }

            if settings.joystick_as_dpad() {
                let stick_x = (pressed.lx as f32 - 127.0) / 127.0;
                let stick_y = (pressed.ly as f32 - 127.0) / 127.0;
                let length_threshold = 0.8;
                if stick_x * stick_x + stick_y * stick_y > length_threshold * length_threshold {
                    const STICK_MAPPING: [((f32, f32), Keycode); 4] = [((-1.0, 0.0), Keycode::Left), ((0.0, -1.0), Keycode::Up), ((1.0, 0.0), Keycode::Right), ((0.0, 1.0), Keycode::Down)];
                    for ((x, y), guest_key) in STICK_MAPPING {
                        let dot = stick_x * x + stick_y * y;
                        if dot > 0.5 {
                            stick_keymap &= !(1 << guest_key as u8);
                        }
                    }
                }
            }
        }
        PresentEvent::Inputs {
            keymap: self.keymap & stick_keymap,
            touch: self.touch_points,
            #[cfg(debug_assertions)]
            debug_touch: None,
        }
    }

    pub fn present_ui(
        &mut self,
        screen_layouts: &mut ScreenLayouts,
        ra_context: &mut RaContext,
        cjk_download: &mut cjk_font::Download,
        default_key_binding: KeyBinding,
    ) -> Option<(CartridgeIo, GlobalSettings, Settings, PathBuf)> {
        unsafe {
            sceShellUtilUnlock(SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN | SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN_2);

            let cartridge_path = PathBuf::from(ROM_PATH);

            let mut params = [0u8; 1024];
            if sceAppMgrGetAppParam(params.as_mut_ptr() as _) == 0 {
                if let Ok(params) = CStr::from_bytes_until_nul(&params) {
                    if let Ok(params) = params.to_str() {
                        if params.contains("psgm:play") {
                            if let Some(pos) = params.find("&param=") {
                                let path = PathBuf::from(&params[pos + 7..]);
                                info_println!("Launching from app param {}", path.to_str().unwrap());
                                let name = path.file_name().unwrap().to_str().unwrap();
                                let save_file = cartridge_path.join("saves").join(format!("{name}.sav"));
                                let settings_file = cartridge_path.join("settings").join(format!("{name}.ini"));
                                let preview = CartridgePreview::new(path).unwrap();

                                let global_settings = GlobalSettings::new(cartridge_path.join("global_settings"), default_key_binding).unwrap();
                                let mut settings = SettingsConfig::new(settings_file.clone()).settings;
                                screen_layouts.populate_custom_layouts(&global_settings.custom_layouts);
                                settings.populate_screen_layouts(screen_layouts);
                                settings.populate_controls(&global_settings.default_control, &global_settings.custom_controls);

                                ra_context.set_cache_dir(cartridge_path.join("ra"));
                                cjk_download.set_file_path(&cjk_font::font_path(&cartridge_path));

                                return Some((CartridgeIo::from_preview(preview, save_file).unwrap(), global_settings, settings, settings_file));
                            }
                        }
                    }
                }
            }

            match show_main_menu(PathBuf::from(ROM_PATH), screen_layouts, ra_context, default_key_binding, cjk_download, self) {
                None => None,
                Some((cartridge_io, global_settings, mut settings, settings_file_path)) => {
                    screen_layouts.populate_custom_layouts(&global_settings.custom_layouts);
                    settings.populate_screen_layouts(screen_layouts);
                    settings.populate_controls(&global_settings.default_control, &global_settings.custom_controls);
                    Some((cartridge_io, global_settings, settings, settings_file_path))
                }
            }
        }
    }

    pub fn on_game_launched(&mut self) {
        unsafe { sceShellUtilLock(SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN | SCE_SHELL_UTIL_LOCK_TYPE_QUICK_MENU | SCE_SHELL_UTIL_LOCK_TYPE_USB_CONNECTION | SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN_2) };
    }

    pub fn present_pause(&mut self, gpu_renderer: &GpuRenderer, settings: &mut Settings, settings_file_path: &std::path::Path, rom_path: &std::path::Path) -> UiPauseMenuReturn {
        unsafe { sceShellUtilUnlock(SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN | SCE_SHELL_UTIL_LOCK_TYPE_QUICK_MENU | SCE_SHELL_UTIL_LOCK_TYPE_USB_CONNECTION | SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN_2) };
        let ret = show_pause_menu(self, gpu_renderer, settings, settings_file_path, rom_path);
        match ret {
            UiPauseMenuReturn::Resume | UiPauseMenuReturn::BlowMic => unsafe {
                self.do_nothing_until_all_btns_released = true;
                sceShellUtilLock(SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN | SCE_SHELL_UTIL_LOCK_TYPE_QUICK_MENU | SCE_SHELL_UTIL_LOCK_TYPE_USB_CONNECTION | SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN_2);
            },
            _ => {}
        }
        ret
    }

    pub fn present_progress(&mut self, current_name: impl AsRef<str>, progress: usize, total: usize) {
        show_progress(self, current_name, progress, total)
    }

    pub fn present_savestate_progress(&mut self, gpu_renderer: &GpuRenderer, text: impl AsRef<str>, progress: usize) {
        crate::presenter::ui::show_savestate_progress(self, gpu_renderer, text, progress)
    }

    pub fn get_presenter_audio_out(&self) -> PresenterAudioOut {
        self.presenter_audio_out.clone()
    }

    pub fn get_presenter_audio_in(&self) -> PresenterAudioIn {
        self.presenter_audio_in.clone()
    }

    pub fn gl_swap_window(&self) {
        unsafe { vita_gl::vglSwapBuffers(gl::FALSE) };
    }

    pub fn wait_vsync(&self) {
        unsafe { sceDisplayWaitVblankStart() };
    }

    pub unsafe fn gl_create_depth_tex() -> GLuint {
        let mut tex = 0;
        gl::GenTextures(1, &mut tex);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, 1, 1, 0, gl::RGBA, gl::UNSIGNED_BYTE, ptr::null());
        vita_gl::vglFree(vita_gl::vglGetTexDataPointer(gl::TEXTURE_2D));
        vita_gl::vglTexImageDepthBuffer(gl::TEXTURE_2D);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        tex
    }

    pub unsafe fn gl_get_tex_ptr() -> *mut u8 {
        vita_gl::vglGetTexDataPointer(gl::TEXTURE_2D) as _
    }

    pub unsafe fn gl_remap_tex() -> *mut u8 {
        vita_gl::vglRemapTexPtr() as _
    }

    pub unsafe fn gl_tex_image_2d_rgba5(width: i32, height: i32) {
        vita_gl::glTexImage2Drgba5(width, height);
    }

    pub unsafe fn gl_buffer_data(target: GLenum, data: *const u8) {
        vita_gl::vglBufferData(target, data);
    }

    pub unsafe fn gl_mem_align_ram(alignment: usize, size: usize) -> *mut u8 {
        vita_gl::vgl_memalign(alignment, size, VglMemType::Ram)
    }

    pub unsafe fn gl_mem_align_vram(alignment: usize, size: usize) -> *mut u8 {
        vita_gl::vgl_memalign(alignment, size, VglMemType::Vram)
    }

    pub fn gl_version_suffix() -> &'static str {
        vita_gl::VITA_GL_VERSION
    }

    pub fn core_unlocked(&self) -> bool {
        self.core_unlocked
    }

    pub fn can_stream_screen(&self) -> bool {
        self.can_stream_screen
    }
}

impl UiBackend for Presenter {
    fn init(&mut self) {
        unsafe {
            info_println!("Initialize ImGui for vitaGL");
            ImGui_ImplVitaGL_Init();

            info_println!("Set style for ImGui");
            (*ImGui::GetIO()).MouseDrawCursor = false;
            ImGui_ImplVitaGL_TouchUsage(true);
            ImGui_ImplVitaGL_GamepadUsage(true);
            ImGui_ImplVitaGL_MouseStickUsage(false);
            ImGui::StyleColorsDark(ptr::null_mut());
        }
    }

    fn new_frame(&mut self) -> bool {
        unsafe { ImGui_ImplVitaGL_NewFrame() };
        true
    }

    fn render_draw_data(&mut self, draw_data: *mut ImDrawData) {
        unsafe { ImGui_ImplVitaGL_RenderDrawData(draw_data) };
    }

    fn swap_window(&mut self) {
        self.gl_swap_window();
    }
}

fn to_cstr_utf16(str: &str) -> Vec<u16> {
    let mut vec: Vec<u16> = str.encode_utf16().collect();
    vec.push(0);
    vec
}

unsafe fn dialog_input(title: &str, value: &str, input_type: u32, text_box_mode: u32, max_len: u32) -> String {
    let mut params: SceImeDialogParam = mem::zeroed();
    sce_common_dialog_set_magic_number(&mut params.commonParam);
    params.sdkVersion = PSP2_SDK_VERSION;
    params.type_ = input_type;
    params.textBoxMode = text_box_mode;

    let title = to_cstr_utf16(title);
    params.title = title.as_ptr() as _;

    let mut input_buf = [0u16; SCE_IME_DIALOG_MAX_TEXT_LENGTH as usize + 1];
    debug_assert!(max_len < SCE_IME_DIALOG_MAX_TEXT_LENGTH);

    let value = to_cstr_utf16(value);
    params.initialText = value.as_ptr() as _;
    params.inputTextBuffer = input_buf.as_mut_ptr() as _;
    params.maxTextLength = max_len;

    sceImeDialogInit(&params);

    while sceImeDialogGetStatus() != SCE_COMMON_DIALOG_STATUS_FINISHED {
        vita_gl::vglSwapBuffers(gl::TRUE);
    }
    sceImeDialogTerm();

    let len = input_buf.iter().position(|c| *c == 0).unwrap_or(input_buf.len());
    String::from_utf16(&input_buf[..len]).unwrap().trim().to_string()
}

/// One settings row: a fixed-width button showing `value` (tapping it opens the
/// on-screen keyboard) with `label` to its right — mirrors the Linux InputInt
/// layout. Returns whether the button was tapped.
unsafe fn layout_field_button(label: &str, value: &CStr) -> bool {
    let c_label = CString::from_str(label).unwrap();
    ImGui::PushID(c_label.as_ptr());
    let sz = ImVec2 { x: 150.0, y: 0.0 };
    let clicked = ImGui::Button(value.as_ptr(), &sz);
    ImGui::SameLine(0f32, -1f32);
    ImGui::Text(c_label.as_ptr());
    ImGui::PopID();
    clicked
}

pub fn show_layout_create_settings(global_settings: &mut GlobalSettings, custom_layout_context: &mut CustomLayoutContext, custom_layout: &mut CustomLayout) -> bool {
    unsafe {
        // Reserve the Save button (plus one error line only when shown) so the
        // fields/preview span down to the button.
        let has_error = custom_layout_context.parse_error || custom_layout_context.empty_name || custom_layout_context.duplicated_name;
        let mut footer = ImGui::GetFrameHeightWithSpacing();
        if has_error {
            footer += ImGui::GetTextLineHeightWithSpacing();
        }
        let body_height = (ImGui::GetContentRegionAvail().y - footer).max(0.0);

        let fields_sz = ImVec2 { x: 430.0, y: body_height };
        ImGui::BeginChild(c"##layout_fields".as_ptr(), &fields_sz, false, 0);

        if layout_field_button("Layout name", &custom_layout.name_c_str()) {
            custom_layout.name = dialog_input("Layout name", &custom_layout.name, SCE_IME_TYPE_BASIC_LATIN, SCE_IME_DIALOG_TEXTBOX_MODE_DEFAULT, 32);
        }

        for (i, screen) in [(0usize, c"Top screen"), (1usize, c"Bottom screen")] {
            ImGui::Spacing();
            ImGui::Separator();
            ImGui::TextDisabled(screen.as_ptr());
            ImGui::PushID3(i as _);

            if layout_field_button("Width", &custom_layout.width_c_str(i)) {
                custom_layout_context.parse_error = !custom_layout.set_width(i, &dialog_input("Width", &custom_layout.width_str(i), SCE_IME_TYPE_NUMBER, SCE_IME_DIALOG_TEXTBOX_MODE_DEFAULT, 5));
            }
            if layout_field_button("Height", &custom_layout.height_c_str(i)) {
                custom_layout_context.parse_error = !custom_layout.set_height(i, &dialog_input("Height", &custom_layout.height_str(i), SCE_IME_TYPE_NUMBER, SCE_IME_DIALOG_TEXTBOX_MODE_DEFAULT, 5));
            }
            if layout_field_button("Position X", &custom_layout.pos_x_c_str(i)) {
                custom_layout_context.parse_error = !custom_layout.set_pos_x(i, &dialog_input("Position X", &custom_layout.pos_x_str(i), SCE_IME_TYPE_NUMBER, SCE_IME_DIALOG_TEXTBOX_MODE_DEFAULT, 5));
            }
            if layout_field_button("Position Y", &custom_layout.pos_y_c_str(i)) {
                custom_layout_context.parse_error = !custom_layout.set_pos_y(i, &dialog_input("Position Y", &custom_layout.pos_y_str(i), SCE_IME_TYPE_NUMBER, SCE_IME_DIALOG_TEXTBOX_MODE_DEFAULT, 5));
            }
            if layout_field_button("Rotation", &custom_layout.rot_c_str(i)) {
                custom_layout_context.parse_error = !custom_layout.set_rot(i, &dialog_input("Rotation", &custom_layout.rot_str(i), SCE_IME_TYPE_NUMBER, SCE_IME_DIALOG_TEXTBOX_MODE_DEFAULT, 3));
            }

            ImGui::PopID();
        }

        ImGui::Spacing();
        ImGui::Separator();
        if layout_field_button("Widescreen coefficient", &custom_layout.wide_screen_coefficient_c_str()) {
            custom_layout_context.parse_error = !custom_layout.set_wide_screen_coefficient(&dialog_input(
                "Widescreen coefficient",
                &custom_layout.wide_screen_coefficient_str(),
                SCE_IME_TYPE_EXTENDED_NUMBER,
                SCE_IME_DIALOG_TEXTBOX_MODE_DEFAULT,
                6,
            ));
        }

        draw_overlay_picker(custom_layout);

        ImGui::EndChild();
        ImGui::SameLine(0.0, (*ImGui::GetStyle()).ItemSpacing.x);
        let preview_sz = ImVec2 { x: 0.0, y: body_height };
        ImGui::BeginChild(c"##layout_preview".as_ptr(), &preview_sz, false, 0);
        draw_layout_preview(custom_layout);
        ImGui::EndChild();

        if has_error {
            ImGui::PushStyleColor(ImGuiCol__ImGuiCol_Text as _, 0xFF0000FF);
            if custom_layout_context.parse_error {
                ImGui::Text(c"Please enter a valid value".as_ptr());
            } else if custom_layout_context.empty_name {
                ImGui::Text(c"Layout name can't be empty".as_ptr());
            } else {
                ImGui::Text(c"Layout with same name already exists".as_ptr());
            }
            ImGui::PopStyleColor(1);
        }

        let vec = ImVec2 { x: -1.0, y: 0.0 };
        if ImGui::Button(c"Save layout".as_ptr(), &vec) {
            if custom_layout.name.is_empty() {
                custom_layout_context.empty_name = true;
            } else if global_settings.add_custom_layout(custom_layout.clone()) {
                return true;
            } else {
                custom_layout_context.duplicated_name = true;
            }
        }
        false
    }
}

/// One controls-editor row: `label` with a combo of the bindable Vita buttons
/// (plus None) writing the picked button bit into `value`.
unsafe fn binding_button_row(id: i32, label: &str, value: &mut u32) {
    ImGui::PushID3(id);
    let key_label = CString::from_str(label).unwrap();
    ImGui::Text(key_label.as_ptr());
    ImGui::SameLine(0f32, -1f32);
    ImGui::SetCursorPosX(ImGui::GetCursorPosX() + ImGui::GetContentRegionAvail().x - 200f32);
    ImGui::PushItemWidth(200f32);

    let current = BINDABLE_BUTTONS.iter().position(|(_, bit)| *bit == *value);
    let preview = current.map(|c| BINDABLE_BUTTONS[c].0).unwrap_or(c"None");
    if ImGui::BeginCombo(c"##btn".as_ptr(), preview.as_ptr(), 0) {
        let sz = ImVec2 { x: 0f32, y: 0f32 };
        if ImGui::Selectable(c"None".as_ptr(), current.is_none(), 0, &sz) {
            *value = 0;
        }
        for (j, (name, bit)) in BINDABLE_BUTTONS.iter().enumerate() {
            let is_selected = current == Some(j);
            if ImGui::Selectable(name.as_ptr(), is_selected, 0, &sz) {
                *value = *bit;
            }
            if is_selected {
                ImGui::SetItemDefaultFocus();
            }
        }
        ImGui::EndCombo();
    }
    ImGui::PopItemWidth();
    ImGui::PopID();
}

pub fn show_controls_create_settings(global_settings: &mut GlobalSettings, custom_layout_context: &mut CustomLayoutContext, binding: &mut KeyBinding) -> bool {
    unsafe {
        let has_error = custom_layout_context.empty_name || custom_layout_context.duplicated_name;
        let mut footer = ImGui::GetFrameHeightWithSpacing();
        if has_error {
            footer += ImGui::GetTextLineHeightWithSpacing();
        }
        let body_height = (ImGui::GetContentRegionAvail().y - footer).max(0.0);

        let fields_sz = ImVec2 { x: 0.0, y: body_height };
        ImGui::BeginChild(c"##controls_fields".as_ptr(), &fields_sz, false, 0);

        if layout_field_button("Profile name", &binding.name_c_str()) {
            binding.name = dialog_input("Profile name", &binding.name, SCE_IME_TYPE_BASIC_LATIN, SCE_IME_DIALOG_TEXTBOX_MODE_DEFAULT, 32);
        }
        ImGui::Spacing();
        ImGui::Separator();

        for i in 0..NUM_KEYS {
            binding_button_row(i as _, DS_KEY_NAMES[i], &mut binding.buttons[i]);
        }

        ImGui::Spacing();
        ImGui::Separator();
        ImGui::TextDisabled(c"Hotkeys (hold the PS button)".as_ptr());
        for i in 0..NUM_HOTKEYS {
            binding_button_row((NUM_KEYS + i) as _, HOTKEY_NAMES[i], &mut binding.hotkeys[i]);
        }

        ImGui::EndChild();

        if has_error {
            ImGui::PushStyleColor(ImGuiCol__ImGuiCol_Text as _, 0xFF0000FF);
            if custom_layout_context.empty_name {
                ImGui::Text(c"Profile name can't be empty".as_ptr());
            } else {
                ImGui::Text(c"A profile with that name already exists".as_ptr());
            }
            ImGui::PopStyleColor(1);
        }

        let vec = ImVec2 { x: -1.0, y: 0.0 };
        if ImGui::Button(c"Save profile".as_ptr(), &vec) {
            if binding.name.is_empty() {
                custom_layout_context.empty_name = true;
            } else if global_settings.add_custom_controls(binding.clone()) {
                return true;
            } else {
                custom_layout_context.duplicated_name = true;
            }
        }
        false
    }
}

pub fn show_retroachievements_settings(global_settings: &mut GlobalSettings, login_context: &mut RALoginContext, context: &mut RaContext) {
    unsafe {
        if !global_settings.ra_username.is_empty() && !global_settings.ra_token.is_empty() {
            let msg = format!("Currently logged in as {}", global_settings.ra_username);
            ImGui::Text(CString::from_str(&msg).unwrap().as_ptr());
        }

        let title = "Username";
        let c_title = CString::from_str(&title).unwrap();
        ImGui::PushID(c_title.as_ptr());
        ImGui::Text(c_title.as_ptr());
        ImGui::SameLine(0f32, -1f32);
        ImGui::SetCursorPosX(ImGui::GetCursorPosX() + ImGui::GetContentRegionAvail().x - 500f32);
        let vec = ImVec2 { x: 500.0, y: 0.0 };
        let username = CString::from_str(&login_context.username).unwrap();
        if ImGui::Button(username.as_ptr() as _, &vec) {
            login_context.username = dialog_input(&title, &login_context.username, SCE_IME_TYPE_BASIC_LATIN, SCE_IME_DIALOG_TEXTBOX_MODE_DEFAULT, 128);
        }
        ImGui::PopID();

        let title = "Password";
        let c_title = CString::from_str(&title).unwrap();
        ImGui::PushID(c_title.as_ptr());
        ImGui::Text(c_title.as_ptr());
        ImGui::SameLine(0f32, -1f32);
        ImGui::SetCursorPosX(ImGui::GetCursorPosX() + ImGui::GetContentRegionAvail().x - 500f32);
        let vec = ImVec2 { x: 500.0, y: 0.0 };
        let password = CString::from_str(&"*".repeat(login_context.password.len())).unwrap();
        if ImGui::Button(password.as_ptr() as _, &vec) {
            login_context.password = dialog_input(&title, &login_context.password, SCE_IME_TYPE_BASIC_LATIN, SCE_IME_DIALOG_TEXTBOX_MODE_PASSWORD, 128);
        }
        ImGui::PopID();

        ImGui::PushStyleColor(ImGuiCol__ImGuiCol_Text as _, 0xFF0000FF);
        if !login_context.error.is_empty() {
            ImGui::Text(CString::from_str(&login_context.error).unwrap().as_ptr());
        }
        ImGui::PopStyleColor(1);

        if login_context.logging_in {
            if let Some(data) = context.get_login_callback_data() {
                login_context.logging_in = false;
                if data.result == rcheevos::RC_OK {
                    *login_context = Default::default();
                    if let Some((username, token)) = context.get_user_info() {
                        global_settings.set_ra_data(username, token);
                    }
                } else {
                    login_context.error = data.error_message.unwrap_or_default();
                }
            }
        }

        let vec = ImVec2 { x: 0.0, y: 0.0 };
        if ImGui::Button(c"Login".as_ptr(), &vec) {
            login_context.error.clear();
            login_context.logging_in = true;
            context.login_with_password(&login_context.username, &login_context.password);
        }
    }
}
