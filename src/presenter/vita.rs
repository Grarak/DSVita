use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::input::Keycode;
use crate::presenter::menu::Menu;
use crate::presenter::platform::imgui::{
    ImGuiCond__ImGuiCond_Once, ImGuiContext, ImGuiSelectableFlags__ImGuiSelectableFlags_SpanAllColumns, ImGui_Begin, ImGui_CreateContext, ImGui_DestroyContext, ImGui_End, ImGui_GetDrawData,
    ImGui_GetIO, ImGui_ImplVitaGL_GamepadUsage, ImGui_ImplVitaGL_Init, ImGui_ImplVitaGL_MouseStickUsage, ImGui_ImplVitaGL_NewFrame, ImGui_ImplVitaGL_RenderDrawData, ImGui_ImplVitaGL_TouchUsage,
    ImGui_ImplVitaGL_UseIndirectFrontTouch, ImGui_ListBoxFooter, ImGui_ListBoxHeader, ImGui_Render, ImGui_Selectable, ImGui_SetNextWindowPos, ImGui_SetNextWindowSize, ImGui_StyleColorsDark,
    ImGui_Text, ImVec2,
};
use crate::presenter::{PresentEvent, PRESENTER_AUDIO_BUF_SIZE, PRESENTER_AUDIO_SAMPLE_RATE, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH, PRESENTER_SUB_BOTTOM_SCREEN};
use gl::types::{GLboolean, GLenum, GLuint};
use std::ffi::CString;
use std::mem::MaybeUninit;
use std::ptr;
use vitasdk_sys::*;
use vitasdk_sys::{sceAudioOutOpenPort, sceTouchPeek};

mod imgui {
    #![allow(warnings, unused)]
    include!(concat!(env!("OUT_DIR"), "/imgui_bindings.rs"));
}

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
    pub fn vglGetProcAddress(name: *const c_char) -> *mut c_void;
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
                audio_port: sceAudioOutOpenPort(SCE_AUDIO_OUT_PORT_TYPE_MAIN, PRESENTER_AUDIO_BUF_SIZE as _, PRESENTER_AUDIO_SAMPLE_RATE as _, SCE_AUDIO_OUT_MODE_STEREO),
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
    imgui_context: *mut ImGuiContext,
}

impl Presenter {
    pub fn new() -> Self {
        unsafe {
            vglSetupRuntimeShaderCompiler(SharkOpt::Unsafe as _, 1, 0, 1);
            // Disable multisampling for depth texture
            vglInitExtended(0, 960, 544, 16 * 1024 * 1024, SCE_GXM_MULTISAMPLE_NONE);
            gl::load_with(|name| {
                let name = CString::new(name).unwrap();
                vglGetProcAddress(name.as_ptr())
            });

            sceTouchSetSamplingState(SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_START);

            let imgui_context = ImGui_CreateContext(ptr::null_mut());
            ImGui_ImplVitaGL_Init();
            ImGui_ImplVitaGL_TouchUsage(false);
            ImGui_ImplVitaGL_UseIndirectFrontTouch(false);
            ImGui_ImplVitaGL_MouseStickUsage(false);
            ImGui_ImplVitaGL_GamepadUsage(false);
            ImGui_StyleColorsDark(ptr::null_mut());

            Presenter {
                presenter_audio: PresenterAudio::new(),
                keymap: 0xFFFFFFFF,
                imgui_context,
            }
        }
    }

    pub fn poll_event(&mut self) -> PresentEvent {
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
                if PRESENTER_SUB_BOTTOM_SCREEN.is_within(x, y) {
                    let (x, y) = PRESENTER_SUB_BOTTOM_SCREEN.normalize(x, y);
                    let x = (DISPLAY_WIDTH as u32 * x / PRESENTER_SUB_BOTTOM_SCREEN.width) as u8;
                    let y = (DISPLAY_HEIGHT as u32 * y / PRESENTER_SUB_BOTTOM_SCREEN.height) as u8;
                    touch = Some((x, y));
                    self.keymap &= !(1 << 16);
                }
            } else {
                self.keymap |= 1 << 16;
            }
        }
        PresentEvent::Inputs { keymap: self.keymap, touch }
    }

    pub fn present_menu(&mut self, menu: &Menu) {
        unsafe {
            ImGui_ImplVitaGL_NewFrame();

            let title = CString::new(menu.title.as_str()).unwrap();

            let pos = ImVec2 { x: 0f32, y: 0f32 };
            let pivot = ImVec2 { x: 0f32, y: 0f32 };
            ImGui_SetNextWindowPos(&pos, ImGuiCond__ImGuiCond_Once as _, &pivot);
            let size = ImVec2 {
                x: PRESENTER_SCREEN_WIDTH as f32,
                y: PRESENTER_SCREEN_HEIGHT as f32,
            };
            ImGui_SetNextWindowSize(&size, ImGuiCond__ImGuiCond_Once as _);

            let mut open = true;
            if ImGui_Begin(title.as_ptr() as _, &mut open, 0) {
                let size = ImVec2 {
                    x: 0f32,
                    y: PRESENTER_SCREEN_HEIGHT as f32,
                };
                if ImGui_ListBoxHeader(title.as_ptr() as _, &size) {
                    for (i, entry) in menu.entries.iter().enumerate() {
                        let entry_name = CString::new(entry.title.as_str()).unwrap();
                        let size = ImVec2 { x: 0f32, y: 0f32 };
                        ImGui_Selectable(entry_name.as_ptr() as _, i == menu.selected, ImGuiSelectableFlags__ImGuiSelectableFlags_SpanAllColumns as _, &size);
                    }
                    ImGui_ListBoxFooter();
                }
                ImGui_End();
            }

            gl::Viewport(0, 0, (*ImGui_GetIO()).DisplaySize.x as _, (*ImGui_GetIO()).DisplaySize.y as _);
            gl::ClearColor(0f32, 0f32, 0f32, 1f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            ImGui_Render();
            ImGui_ImplVitaGL_RenderDrawData(ImGui_GetDrawData());

            self.gl_swap_window();
        }
    }

    pub fn destroy_menu(&self) {
        unsafe {
            ImGui_ImplVitaGL_NewFrame();

            let pos = ImVec2 { x: 0f32, y: 0f32 };
            let pivot = ImVec2 { x: 0f32, y: 0f32 };
            ImGui_SetNextWindowPos(&pos, ImGuiCond__ImGuiCond_Once as _, &pivot);
            let size = ImVec2 {
                x: PRESENTER_SCREEN_WIDTH as f32,
                y: PRESENTER_SCREEN_HEIGHT as f32,
            };
            ImGui_SetNextWindowSize(&size, ImGuiCond__ImGuiCond_Once as _);

            let mut open = true;
            if ImGui_Begin("DSVita\0".as_ptr() as _, &mut open, 0) {
                ImGui_Text("Loading\0".as_ptr() as _);
                ImGui_End();
            }

            gl::Viewport(0, 0, (*ImGui_GetIO()).DisplaySize.x as _, (*ImGui_GetIO()).DisplaySize.y as _);
            gl::ClearColor(0f32, 0f32, 0f32, 1f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            ImGui_Render();
            ImGui_ImplVitaGL_RenderDrawData(ImGui_GetDrawData());

            self.gl_swap_window();

            ImGui_DestroyContext(self.imgui_context);

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
