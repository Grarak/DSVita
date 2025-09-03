use crate::cartridge_io::{CartridgeIo, CartridgePreview};
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::core::input::Keycode;
use crate::logging::info_println;
use crate::presenter::imgui::root::{
    ImDrawData, ImGui, ImGuiStyleVar__ImGuiStyleVar_ItemSpacing, ImGuiStyleVar__ImGuiStyleVar_WindowRounding, ImGui_ImplVitaGL_GamepadUsage, ImGui_ImplVitaGL_Init, ImGui_ImplVitaGL_MouseStickUsage,
    ImGui_ImplVitaGL_NewFrame, ImGui_ImplVitaGL_RenderDrawData, ImGui_ImplVitaGL_TouchUsage, ImVec2,
};
use crate::presenter::ui::{init_ui, show_main_menu, show_pause_menu, UiBackend, UiPauseMenuReturn};
use crate::presenter::{
    PresentEvent, PRESENTER_AUDIO_BUF_SIZE, PRESENTER_AUDIO_SAMPLE_RATE, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH, PRESENTER_SUB_BOTTOM_SCREEN, PRESENTER_SUB_RESIZED_BOTTOM_SCREEN,
    PRESENTER_SUB_ROTATED_BOTTOM_SCREEN,
};
use crate::settings::{ScreenMode, Settings, SettingsConfig};
use gl::types::{GLboolean, GLenum, GLuint};
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
extern "C" {
    pub fn vglSwapBuffers(has_commondialog: GLboolean);
    pub fn vglSetupRuntimeShaderCompiler(opt_level: c_uint, use_fastmath: c_int, use_fastprecision: c_int, use_fastint: c_int);
    pub fn vglInitExtended(legacy_pool_size: c_int, width: c_int, height: c_int, ram_threshold: c_int, msaa: SceGxmMultisampleMode) -> GLboolean;
    pub fn vglGetTexDataPointer(target: GLenum) -> *mut c_void;
    pub fn vglFree(addr: *mut c_void);
    pub fn vglTexImageDepthBuffer(target: GLenum);
    pub fn vglGetProcAddress(name: *const c_char) -> *const c_void;
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
    was_psn_btn_pressed: bool,
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
            vglSetupRuntimeShaderCompiler(SharkOpt::Unsafe as _, 1, 0, 1);
            info_println!("Initialize vitaGL");
            // Disable multisampling for depth texture
            vglInitExtended(0, 960, 544, 60 * 1024 * 1024, SCE_GXM_MULTISAMPLE_NONE);
            gl::load_with(|name| {
                let name = CString::new(name).unwrap();
                vglGetProcAddress(name.as_ptr())
            });

            sceTouchSetSamplingState(SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_STOP);

            let mut instance = Presenter {
                presenter_audio: PresenterAudio::new(),
                keymap: 0xFFFFFFFF,
                was_psn_btn_pressed: false,
            };

            init_ui(&mut instance);
            instance
        }
    }

    pub fn poll_event(&mut self, screenmode: ScreenMode) -> PresentEvent {
        let mut touch = None;

        unsafe {
            let pressed = MaybeUninit::<SceCtrlData>::uninit();
            let mut pressed = pressed.assume_init();
            sceCtrlPeekBufferPositive(0, &mut pressed, 1);

            if pressed.buttons & SCE_CTRL_PSBUTTON != 0 {
                self.was_psn_btn_pressed = true;
            } else if self.was_psn_btn_pressed {
                self.was_psn_btn_pressed = false;
                return PresentEvent::Pause;
            }

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
                        if PRESENTER_SUB_BOTTOM_SCREEN.is_within(x, y) {
                            let (x, y) = PRESENTER_SUB_BOTTOM_SCREEN.normalize(x, y);
                            let screen_x = (DISPLAY_WIDTH as u32 * x / PRESENTER_SUB_BOTTOM_SCREEN.width) as u8;
                            let screen_y = (DISPLAY_HEIGHT as u32 * y / PRESENTER_SUB_BOTTOM_SCREEN.height) as u8;
                            touch = Some((screen_x, screen_y));
                        }
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
                        if PRESENTER_SUB_RESIZED_BOTTOM_SCREEN.is_within(x, y) {
                            let (x, y) = PRESENTER_SUB_RESIZED_BOTTOM_SCREEN.normalize(x, y);
                            let screen_x = (DISPLAY_WIDTH as u32 * x / PRESENTER_SUB_RESIZED_BOTTOM_SCREEN.width) as u8;
                            let screen_y = (DISPLAY_HEIGHT as u32 * y / PRESENTER_SUB_RESIZED_BOTTOM_SCREEN.height) as u8;
                            touch = Some((screen_x, screen_y));
                        }
                    }
                }
                self.keymap &= !(1 << 16);
            } else {
                self.keymap |= 1 << 16;
            }
        }
        PresentEvent::Inputs { keymap: self.keymap, touch }
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

    pub fn on_game_launched(&self) {
        unsafe { sceShellUtilLock(SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN | SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN_2) };
    }

    pub fn present_pause(&mut self, gpu_renderer: &GpuRenderer, settings: &mut Settings) -> UiPauseMenuReturn {
        unsafe { sceShellUtilUnlock(SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN | SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN_2) };
        let ret = show_pause_menu(self, gpu_renderer, settings);
        match ret {
            UiPauseMenuReturn::Resume => unsafe {
                sceShellUtilLock(SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN | SCE_SHELL_UTIL_LOCK_TYPE_PS_BTN_2);
            },
            _ => {}
        }
        ret
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
