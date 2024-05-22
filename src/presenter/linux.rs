use crate::emu::gpu::gpu::{DISPLAY_HEIGHT, DISPLAY_PIXEL_COUNT, DISPLAY_WIDTH};
use crate::emu::input;
use crate::presenter::menu::Menu;
use crate::presenter::{PresentEvent, PRESENTER_AUDIO_BUF_SIZE, PRESENTER_AUDIO_SAMPLE_RATE, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH, PRESENTER_SUB_BOTTOM_SCREEN, PRESENTER_SUB_TOP_SCREEN};
use crate::utils::BuildNoHasher;
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::event::Event;
use sdl2::mouse::MouseButton;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Texture, TextureCreator, WindowCanvas};
use sdl2::video::WindowContext;
use sdl2::{keyboard, EventPump};
use std::collections::HashMap;
use std::rc::Rc;
use std::{mem, ptr, slice};

#[derive(Clone)]
pub struct PresenterAudio {
    audio_queue: Rc<AudioQueue<i16>>,
}

unsafe impl Send for PresenterAudio {}

impl PresenterAudio {
    fn new(audio_queue: AudioQueue<i16>) -> Self {
        PresenterAudio { audio_queue: Rc::new(audio_queue) }
    }

    pub fn play(&self, buffer: &[u32; PRESENTER_AUDIO_BUF_SIZE]) {
        self.audio_queue.clear();
        let raw = unsafe { slice::from_raw_parts(buffer.as_slice().as_ptr() as *const i16, PRESENTER_AUDIO_BUF_SIZE * 2) };
        self.audio_queue.queue_audio(raw).unwrap();
    }
}

pub struct Presenter {
    key_code_mapping: HashMap<keyboard::Keycode, input::Keycode, BuildNoHasher>,
    canvas: WindowCanvas,
    _texture_creator: TextureCreator<WindowContext>,
    texture_top: Texture<'static>,
    texture_bottom: Texture<'static>,
    presenter_audio: PresenterAudio,
    event_pump: EventPump,
    mouse_pressed: bool,
    mouse_id: Option<u32>,
    keymap: u32,
}

impl Presenter {
    pub fn new() -> Self {
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

        sdl2::hint::set("SDL_NO_SIGNAL_HANDLERS", "1");
        let sdl = sdl2::init().unwrap();
        let sdl_video = sdl.video().unwrap();
        let sdl_audio = sdl.audio().unwrap();
        let audio_queue = sdl_audio
            .open_queue(
                None,
                &AudioSpecDesired {
                    freq: Some(PRESENTER_AUDIO_SAMPLE_RATE as i32),
                    channels: Some(2),
                    samples: Some(PRESENTER_AUDIO_BUF_SIZE as u16),
                },
            )
            .unwrap();
        audio_queue.resume();

        let window = sdl_video.window("DSPSV", PRESENTER_SCREEN_WIDTH, PRESENTER_SCREEN_HEIGHT).build().unwrap();
        let mut canvas = window.into_canvas().software().target_texture().build().unwrap();
        let texture_creator = canvas.texture_creator();
        let texture_creator_ref = unsafe { ptr::addr_of!(texture_creator).as_ref().unwrap_unchecked() };
        let texture_top = texture_creator_ref
            .create_texture_streaming(PixelFormatEnum::ABGR8888, DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32)
            .unwrap();
        let texture_bottom = texture_creator_ref
            .create_texture_streaming(PixelFormatEnum::ABGR8888, DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32)
            .unwrap();
        canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
        canvas.clear();
        canvas.present();

        let event_pump = sdl.event_pump().unwrap();

        Presenter {
            key_code_mapping,
            canvas,
            _texture_creator: texture_creator,
            texture_top,
            texture_bottom,
            presenter_audio: PresenterAudio::new(audio_queue),
            event_pump,
            mouse_pressed: false,
            mouse_id: None,
            keymap: 0xFFFFFFFF,
        }
    }

    pub fn event_poll(&mut self) -> PresentEvent {
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

    pub fn present_menu(&mut self, _: &Menu) {}

    pub fn present_textures(&mut self, top: &[u32; DISPLAY_PIXEL_COUNT], bottom: &[u32; DISPLAY_PIXEL_COUNT], fps: u16) {
        let top_aligned: &[u8; DISPLAY_PIXEL_COUNT * 4] = unsafe { mem::transmute(top) };
        let bottom_aligned: &[u8; DISPLAY_PIXEL_COUNT * 4] = unsafe { mem::transmute(bottom) };
        self.texture_top.update(None, top_aligned, DISPLAY_WIDTH * 4).unwrap();
        self.texture_bottom.update(None, bottom_aligned, DISPLAY_WIDTH * 4).unwrap();

        self.canvas.clear();
        self.canvas
            .copy(
                &self.texture_top,
                None,
                Some(Rect::new(
                    PRESENTER_SUB_TOP_SCREEN.x as _,
                    PRESENTER_SUB_TOP_SCREEN.y as _,
                    PRESENTER_SUB_TOP_SCREEN.width,
                    PRESENTER_SUB_TOP_SCREEN.height,
                )),
            )
            .unwrap();
        self.canvas
            .copy(
                &self.texture_bottom,
                None,
                Some(Rect::new(
                    PRESENTER_SUB_BOTTOM_SCREEN.x as _,
                    PRESENTER_SUB_BOTTOM_SCREEN.y as _,
                    PRESENTER_SUB_BOTTOM_SCREEN.width,
                    PRESENTER_SUB_BOTTOM_SCREEN.height,
                )),
            )
            .unwrap();
        self.canvas.present();

        self.canvas.window_mut().set_title(&format!("DSPSV - Internal fps {}", fps)).unwrap();
    }

    pub fn get_presenter_audio(&self) -> PresenterAudio {
        self.presenter_audio.clone()
    }

    pub fn wait_vsync(&self) {}
}
