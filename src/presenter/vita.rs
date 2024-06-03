use crate::emu::gpu::gpu::{DISPLAY_HEIGHT, DISPLAY_PIXEL_COUNT, DISPLAY_WIDTH};
use crate::emu::input::Keycode;
use crate::presenter::menu::Menu;
use crate::presenter::{PresentEvent, PRESENTER_AUDIO_BUF_SIZE, PRESENTER_AUDIO_SAMPLE_RATE, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH, PRESENTER_SUB_BOTTOM_SCREEN, PRESENTER_SUB_TOP_SCREEN};
use gl::types::GLboolean;
use std::ffi::CString;
use std::mem::MaybeUninit;
use std::ptr;
use vitasdk_sys::*;
use vitasdk_sys::{sceAudioOutOpenPort, sceTouchPeek};

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
#[link(name = "vitaGL", kind = "static", modifiers = "+whole-archive")]
extern "C" {
    pub fn vglInit(legacy_pool_size: c_int) -> GLboolean;
    pub fn vglGetProcAddress(name: *const c_char) -> *mut c_void;
    pub fn vglSwapBuffers(has_commondialog: GLboolean);
    pub fn vglSetupRuntimeShaderCompiler(opt_level: SharkOpt, use_fastmath: c_int, use_fastprecision: c_int, use_fastint: c_int);
    pub fn vglInitExtended(legacy_pool_size: c_int, width: c_int, height: c_int, ram_threshold: c_int, msaa: SceGxmMultisampleMode) -> GLboolean;
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
}

impl Presenter {
    pub fn new() -> Self {
        unsafe {
            // vglSetupRuntimeShaderCompiler(SharkOpt::Default, 0, 0, 0);
            vglInitExtended(0x800000, 960, 544, 0x1000000 * 2, SCE_GXM_MULTISAMPLE_NONE);
            // vglInit(0x800000);
            gl::load_with(|name| {
                let name = CString::new(name).unwrap();
                vglGetProcAddress(name.as_ptr())
            });

            Presenter {
                presenter_audio: PresenterAudio::new(),
                keymap: 0xFFFFFFFF,
            }
        }
    }

    pub fn poll_event(&mut self) -> PresentEvent {
        let mut touch = None;

        unsafe {
            let pressed = MaybeUninit::<SceCtrlData>::uninit();
            let mut pressed = pressed.assume_init();
            sceCtrlPeekBufferPositive(0, ptr::addr_of_mut!(pressed), 1);

            for (host_key, guest_key) in KEY_CODE_MAPPING {
                if pressed.buttons & host_key != 0 {
                    self.keymap &= !(1 << guest_key as u8);
                } else {
                    self.keymap |= 1 << guest_key as u8;
                }
            }

            let touch_report = MaybeUninit::<SceTouchData>::uninit();
            let mut touch_report = touch_report.assume_init();
            sceTouchPeek(SCE_TOUCH_PORT_FRONT, ptr::addr_of_mut!(touch_report), 1);

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

    pub fn present_menu(&mut self, menu: &Menu) {}

    pub fn get_presenter_audio(&self) -> PresenterAudio {
        self.presenter_audio.clone()
    }

    pub fn gl_swap_window(&self) {
        unsafe { vglSwapBuffers(gl::FALSE) };
    }

    pub fn wait_vsync(&self) {
        unsafe { sceDisplayWaitVblankStart() };
    }
}
