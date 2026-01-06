use crate::cartridge_io::{CartridgeIo, CartridgePreview};
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::core::input::Keycode;
use crate::logging::info_println;
use crate::presenter::imgui::root::{
    ImDrawData, ImGui, ImGuiStyleVar__ImGuiStyleVar_ItemSpacing, ImGuiStyleVar__ImGuiStyleVar_WindowRounding, ImGui_ImplVitaGL_GamepadUsage, ImGui_ImplVitaGL_Init, ImGui_ImplVitaGL_MouseStickUsage,
    ImGui_ImplVitaGL_NewFrame, ImGui_ImplVitaGL_RenderDrawData, ImGui_ImplVitaGL_TouchUsage, ImVec2,
};
use crate::presenter::ui::{init_ui, show_main_menu, show_pause_menu, show_progress, UiBackend, UiPauseMenuReturn};
use crate::presenter::{
    PresentEvent, PRESENTER_AUDIO_IN_BUF_SIZE, PRESENTER_AUDIO_IN_SAMPLE_RATE, PRESENTER_AUDIO_OUT_BUF_SIZE, PRESENTER_AUDIO_OUT_SAMPLE_RATE, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH,
};
use crate::settings::{Settings, SettingsConfig};
use gl::types::GLuint;
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::path::PathBuf;
use std::ptr;
use vitasdk_sys::*;

mod imgui {
    #![allow(warnings, unused)]
    include!(concat!(env!("OUT_DIR"), "/imgui_bindings.rs"));
}

const ROM_PATH: &str = "ux0:data/dsvita";
pub const LOG_PATH: &str = "ux0:data/dsvita/log";
pub const LOG_FILE: &str = "ux0:data/dsvita/log/log.txt";

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
// #[link(name = "SceRazorHud_stub", kind = "static", modifiers = "+whole-archive")]
// #[link(name = "ScePerf_stub", kind = "static", modifiers = "+whole-archive")]
extern "C" {
    // pub fn sceRazorCpuPushMarkerWithHud(label: *const c_char, color: c_int, flags: c_int) -> c_int;
    // pub fn sceRazorCpuPopMarker() -> c_int;
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
    pressed_btn: u32,
    do_nothing_until_all_btns_released: bool,
}

impl Presenter {
    #[cold]
    pub fn new() -> Self {
        unsafe {
            info_println!("Set clocks");
            scePowerSetArmClockFrequency(444);
            scePowerSetGpuClockFrequency(222);
            scePowerSetBusClockFrequency(222);
            scePowerSetGpuXbarClockFrequency(166);

            sceShellUtilInitEvents(0);

            info_println!("Set shader compiler arguments");
            vita_gl::vglSetupRuntimeShaderCompiler(SharkOpt::Fast as _, 1, 0, 1);
            info_println!("Initialize vitaGL");
            // Disable multisampling for depth texture
            vita_gl::vglInitExtended(0, 960, 544, 65 * 1024 * 1024, SCE_GXM_MULTISAMPLE_NONE);
            gl::load_with(|name| {
                let name = CString::new(name).unwrap();
                vita_gl::vglGetProcAddress(name.as_ptr() as _) as _
            });

            sceTouchSetSamplingState(SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_STOP);

            let mut instance = Presenter {
                presenter_audio_out: PresenterAudioOut::new(),
                presenter_audio_in: PresenterAudioIn::new(),
                touch_points: None,
                keymap: 0xFFFFFFFF,
                pressed_btn: 0,
                do_nothing_until_all_btns_released: false,
            };

            init_ui(&mut instance);
            instance
        }
    }

    pub fn poll_event(&mut self, settings: &Settings) -> PresentEvent {
        let mut stick_keymap = 0xFFFFFFFF;

        unsafe {
            let pressed = MaybeUninit::<SceCtrlData>::uninit();
            let mut pressed = pressed.assume_init();
            sceCtrlPeekBufferPositive(0, &mut pressed, 1);

            let mut previous_pressed_btn = self.pressed_btn;
            self.pressed_btn = pressed.buttons;

            if pressed.buttons & SCE_CTRL_PSBUTTON != 0 {
                const SHORTCUT_EVENTS: [(PresentEvent, SceCtrlButtons); 5] = [
                    (
                        PresentEvent::CycleScreenLayout {
                            offset: -1,
                            swap: false,
                            top_screen_scale_offset: 0,
                            bottom_screen_scale_offset: 0,
                        },
                        SCE_CTRL_LTRIGGER,
                    ),
                    (
                        PresentEvent::CycleScreenLayout {
                            offset: 1,
                            swap: false,
                            top_screen_scale_offset: 0,
                            bottom_screen_scale_offset: 0,
                        },
                        SCE_CTRL_RTRIGGER,
                    ),
                    (
                        PresentEvent::CycleScreenLayout {
                            offset: 0,
                            swap: true,
                            top_screen_scale_offset: 0,
                            bottom_screen_scale_offset: 0,
                        },
                        SCE_CTRL_CROSS,
                    ),
                    (
                        PresentEvent::CycleScreenLayout {
                            offset: 0,
                            swap: false,
                            top_screen_scale_offset: 0,
                            bottom_screen_scale_offset: 1,
                        },
                        SCE_CTRL_CIRCLE,
                    ),
                    (
                        PresentEvent::CycleScreenLayout {
                            offset: 0,
                            swap: false,
                            top_screen_scale_offset: 1,
                            bottom_screen_scale_offset: 0,
                        },
                        SCE_CTRL_SQUARE,
                    ),
                ];

                for (event, button) in SHORTCUT_EVENTS {
                    if previous_pressed_btn & button != 0 && pressed.buttons & button == 0 {
                        self.do_nothing_until_all_btns_released = true;
                        return event;
                    }
                }
            }

            if self.do_nothing_until_all_btns_released {
                if pressed.buttons == 0 {
                    previous_pressed_btn = 0;
                    self.do_nothing_until_all_btns_released = false;
                } else {
                    return PresentEvent::Inputs { keymap: 0xFFFFFFFF, touch: None };
                }
            }

            if previous_pressed_btn & SCE_CTRL_PSBUTTON != 0 && pressed.buttons & SCE_CTRL_PSBUTTON == 0 {
                return PresentEvent::Pause;
            }

            for (host_key, guest_key) in KEY_CODE_MAPPING {
                if pressed.buttons & host_key != 0 {
                    self.keymap &= !(1 << guest_key as u8);
                } else {
                    self.keymap |= 1 << guest_key as u8;
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
        }
        PresentEvent::Inputs {
            keymap: self.keymap & stick_keymap,
            touch: self.touch_points,
        }
    }

    pub fn present_ui(&mut self) -> Option<(CartridgeIo, Settings)> {
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
                                let save_file = PathBuf::from(cartridge_path.join("saves")).join(format!("{name}.sav"));
                                let settings_file = PathBuf::from(cartridge_path.join("settings")).join(format!("{name}.ini"));
                                let preview = CartridgePreview::new(path).unwrap();
                                return Some((CartridgeIo::from_preview(preview, save_file).unwrap(), SettingsConfig::new(settings_file).settings));
                            }
                        }
                    }
                }
            }

            show_main_menu(PathBuf::from(ROM_PATH), self)
        }
    }

    pub fn on_game_launched(&mut self) {
        unsafe { sceShellUtilLock(SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN | SCE_SHELL_UTIL_LOCK_TYPE_QUICK_MENU | SCE_SHELL_UTIL_LOCK_TYPE_USB_CONNECTION | SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN_2) };
    }

    pub fn present_pause(&mut self, gpu_renderer: &GpuRenderer, settings: &mut Settings) -> UiPauseMenuReturn {
        unsafe { sceShellUtilUnlock(SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN | SCE_SHELL_UTIL_LOCK_TYPE_QUICK_MENU | SCE_SHELL_UTIL_LOCK_TYPE_USB_CONNECTION | SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN_2) };
        let ret = show_pause_menu(self, gpu_renderer, settings);
        match ret {
            UiPauseMenuReturn::Resume => unsafe {
                sceShellUtilLock(SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN | SCE_SHELL_UTIL_LOCK_TYPE_QUICK_MENU | SCE_SHELL_UTIL_LOCK_TYPE_USB_CONNECTION | SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN_2);
            },
            _ => {}
        }
        ret
    }

    pub fn present_progress(&mut self, current_name: impl AsRef<str>, progress: usize, total: usize) {
        show_progress(self, current_name, progress, total)
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

    pub unsafe fn gl_bind_frag_ubo(index: u32) {
        vita_gl::vglBindFragUbo(index);
    }

    pub fn gl_version_suffix() -> &'static str {
        vita_gl::VITA_GL_VERSION
    }
}

impl UiBackend for Presenter {
    fn init(&mut self) {
        unsafe {
            info_println!("Initialize ImGui for vitaGL");
            ImGui_ImplVitaGL_Init();

            info_println!("Set style for ImGui");
            let vec = ImVec2 { x: 0f32, y: 2f32 };
            ImGui::PushStyleVar1(ImGuiStyleVar__ImGuiStyleVar_ItemSpacing as _, &vec);
            ImGui::PushStyleVar(ImGuiStyleVar__ImGuiStyleVar_WindowRounding as _, 0f32);
            (*ImGui::GetIO()).MouseDrawCursor = false;
            ImGui_ImplVitaGL_TouchUsage(false);
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
