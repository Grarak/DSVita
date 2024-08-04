use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::input;
use crate::presenter::menu::Menu;
use crate::presenter::{PresentEvent, PRESENTER_AUDIO_BUF_SIZE, PRESENTER_AUDIO_SAMPLE_RATE, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH, PRESENTER_SUB_BOTTOM_SCREEN};
use crate::utils::BuildNoHasher;
use gl::types::GLuint;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::mouse::MouseButton;
use sdl2::video::{GLContext, GLProfile, Window};
use sdl2::{keyboard, EventPump};
use std::collections::HashMap;
use std::ops::Deref;
use std::ptr::NonNull;
use std::rc::Rc;
use std::slice;
use std::sync::{Condvar, Mutex};

#[derive(Clone)]
pub struct PresenterAudio {
    buf: Rc<Mutex<Option<NonNull<u32>>>>,
    condvar: Rc<Condvar>,
}

unsafe impl Send for PresenterAudio {}

impl AudioCallback for PresenterAudio {
    type Channel = i16;

    fn callback(&mut self, callback_buf: &mut [Self::Channel]) {
        let mut buf_lock = self.buf.lock().unwrap();
        match buf_lock.deref() {
            None => callback_buf.fill(0),
            Some(buf) => {
                let buf = unsafe { slice::from_raw_parts(buf.as_ptr() as *const i16, PRESENTER_AUDIO_BUF_SIZE * 2) };
                callback_buf.copy_from_slice(buf);
                *buf_lock = None;
                self.condvar.notify_one();
            }
        }
    }
}

impl PresenterAudio {
    fn new() -> Self {
        PresenterAudio {
            buf: Rc::new(Mutex::new(None)),
            condvar: Rc::new(Condvar::new()),
        }
    }

    pub fn play(&self, buffer: &[u32; PRESENTER_AUDIO_BUF_SIZE]) {
        let mut buf_lock = self.buf.lock().unwrap();
        *buf_lock = Some(unsafe { NonNull::new_unchecked(buffer.as_ptr() as _) });
        let _guard = self.condvar.wait_while(buf_lock, |buf| buf.is_some()).unwrap();
    }
}

pub struct Presenter {
    audio_playback: AudioDevice<PresenterAudio>,
    presenter_audio: PresenterAudio,
    window: Window,
    _gl_ctx: GLContext,
    key_code_mapping: HashMap<keyboard::Keycode, input::Keycode, BuildNoHasher>,
    event_pump: EventPump,
    mouse_pressed: bool,
    mouse_id: Option<u32>,
    keymap: u32,
}

impl Presenter {
    pub fn new() -> Self {
        sdl2::hint::set("SDL_NO_SIGNAL_HANDLERS", "1");
        let sdl = sdl2::init().unwrap();
        let sdl_video = sdl.video().unwrap();
        let sdl_audio = sdl.audio().unwrap();
        let presenter_audio = PresenterAudio::new();
        let audio_playback = sdl_audio
            .open_playback(
                None,
                &AudioSpecDesired {
                    freq: Some(PRESENTER_AUDIO_SAMPLE_RATE as i32),
                    channels: Some(2),
                    samples: Some(PRESENTER_AUDIO_BUF_SIZE as u16),
                },
                |_| presenter_audio.clone(),
            )
            .unwrap();
        audio_playback.resume();

        let gl_attr = sdl_video.gl_attr();
        gl_attr.set_context_profile(GLProfile::GLES);
        gl_attr.set_context_version(3, 0);

        let window = sdl_video.window("DSVita", PRESENTER_SCREEN_WIDTH, PRESENTER_SCREEN_HEIGHT).opengl().build().unwrap();

        let gl_ctx = window.gl_create_context().unwrap();
        gl::load_with(|name| sdl_video.gl_get_proc_address(name) as *const _);

        assert_eq!(gl_attr.context_profile(), GLProfile::GLES);
        assert_eq!(gl_attr.context_version(), (3, 0));

        let event_pump = sdl.event_pump().unwrap();

        let mut key_code_mapping = HashMap::default();
        key_code_mapping.insert(keyboard::Keycode::W, input::Keycode::Up);
        key_code_mapping.insert(keyboard::Keycode::S, input::Keycode::Down);
        key_code_mapping.insert(keyboard::Keycode::A, input::Keycode::Left);
        key_code_mapping.insert(keyboard::Keycode::D, input::Keycode::Right);
        key_code_mapping.insert(keyboard::Keycode::B, input::Keycode::Start);
        key_code_mapping.insert(keyboard::Keycode::V, input::Keycode::Select);
        key_code_mapping.insert(keyboard::Keycode::K, input::Keycode::A);
        key_code_mapping.insert(keyboard::Keycode::J, input::Keycode::B);
        key_code_mapping.insert(keyboard::Keycode::I, input::Keycode::X);
        key_code_mapping.insert(keyboard::Keycode::U, input::Keycode::Y);
        key_code_mapping.insert(keyboard::Keycode::Num8, input::Keycode::TriggerL);
        key_code_mapping.insert(keyboard::Keycode::Num9, input::Keycode::TriggerR);

        Presenter {
            audio_playback,
            presenter_audio,
            window,
            _gl_ctx: gl_ctx,
            key_code_mapping,
            event_pump,
            mouse_pressed: false,
            mouse_id: None,
            keymap: 0xFFFFFFFF,
        }
    }

    pub fn present_menu(&mut self, _: &Menu) {}

    pub fn destroy_menu(&self) {}

    pub fn poll_event(&mut self) -> PresentEvent {
        let mut touch = None;

        let mut sample_touch_points = |x, y| {
            let (x, y) = PRESENTER_SUB_BOTTOM_SCREEN.normalize(x as _, y as _);
            let x = (DISPLAY_WIDTH as u32 * x / PRESENTER_SUB_BOTTOM_SCREEN.width) as u8;
            let y = (DISPLAY_HEIGHT as u32 * y / PRESENTER_SUB_BOTTOM_SCREEN.height) as u8;
            touch = Some((x, y));
        };

        for event in self.event_pump.poll_iter() {
            match event {
                Event::KeyDown { keycode: Some(code), .. } => {
                    if let Some(code) = self.key_code_mapping.get(&code) {
                        self.keymap &= !(1 << *code as u8);
                    }
                }
                Event::KeyUp { keycode: Some(code), .. } => {
                    if let Some(code) = self.key_code_mapping.get(&code) {
                        self.keymap |= 1 << *code as u8;
                    }
                }
                Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } => {
                    self.mouse_pressed = false;
                    self.keymap |= 1 << 16;
                }
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    which,
                    x,
                    y,
                    ..
                } => {
                    self.mouse_pressed = true;
                    self.mouse_id = Some(which);
                    if PRESENTER_SUB_BOTTOM_SCREEN.is_within(x as _, y as _) {
                        sample_touch_points(x, y);
                        self.keymap &= !(1 << 16);
                    }
                }
                Event::MouseMotion { which, x, y, .. } => {
                    if let Some(mouse_id) = self.mouse_id {
                        if self.mouse_pressed && mouse_id == which && PRESENTER_SUB_BOTTOM_SCREEN.is_within(x as _, y as _) {
                            sample_touch_points(x, y);
                        }
                    }
                }
                Event::Quit { .. } => return PresentEvent::Quit,
                _ => {}
            }
        }
        PresentEvent::Inputs { keymap: self.keymap, touch }
    }

    pub fn gl_swap_window(&self) {
        self.window.gl_swap_window();
    }

    pub fn get_presenter_audio(&self) -> PresenterAudio {
        self.presenter_audio.clone()
    }

    pub fn wait_vsync(&self) {}

    pub fn gl_create_depth_tex() -> GLuint {
        0
    }
}
