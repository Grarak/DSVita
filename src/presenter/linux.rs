use crate::emu::gpu::gpu::{DISPLAY_HEIGHT, DISPLAY_PIXEL_COUNT, DISPLAY_WIDTH};
use crate::emu::input;
use crate::presenter::menu::Menu;
use crate::presenter::{PresentEvent, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use crate::utils::BuildNoHasher;
use sdl2::event::Event;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Texture, TextureCreator, WindowCanvas};
use sdl2::video::WindowContext;
use sdl2::{keyboard, EventPump};
use std::collections::HashMap;
use std::{mem, ptr};

pub struct Presenter {
    key_code_mapping: HashMap<keyboard::Keycode, input::Keycode, BuildNoHasher>,
    canvas: WindowCanvas,
    _texture_creator: TextureCreator<WindowContext>,
    texture_top: Texture<'static>,
    texture_bottom: Texture<'static>,
    event_pump: EventPump,
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
        key_code_mapping.insert(keyboard::Keycode::U, input::Keycode::X);
        key_code_mapping.insert(keyboard::Keycode::I, input::Keycode::Y);
        key_code_mapping.insert(keyboard::Keycode::Num8, input::Keycode::TriggerL);
        key_code_mapping.insert(keyboard::Keycode::Num9, input::Keycode::TriggerR);

        sdl2::hint::set("SDL_NO_SIGNAL_HANDLERS", "1");
        let sdl = sdl2::init().unwrap();
        let sdl_video = sdl.video().unwrap();

        let window = sdl_video
            .window("DSPSV", PRESENTER_SCREEN_WIDTH, PRESENTER_SCREEN_HEIGHT)
            .build()
            .unwrap();
        let mut canvas = window
            .into_canvas()
            .software()
            .target_texture()
            .build()
            .unwrap();
        let texture_creator = canvas.texture_creator();
        let texture_creator_ref =
            unsafe { ptr::addr_of!(texture_creator).as_ref().unwrap_unchecked() };
        let texture_top = texture_creator_ref
            .create_texture_streaming(
                PixelFormatEnum::ABGR8888,
                DISPLAY_WIDTH as u32,
                DISPLAY_HEIGHT as u32,
            )
            .unwrap();
        let texture_bottom = texture_creator_ref
            .create_texture_streaming(
                PixelFormatEnum::ABGR8888,
                DISPLAY_WIDTH as u32,
                DISPLAY_HEIGHT as u32,
            )
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
            event_pump,
        }
    }

    pub fn event_poll(&mut self) -> PresentEvent {
        let mut key_map = 0xFFFF;
        for event in self.event_pump.poll_iter() {
            match event {
                Event::KeyDown {
                    keycode: Some(code),
                    ..
                } => {
                    if let Some(code) = self.key_code_mapping.get(&code) {
                        key_map &= !(1 << *code as u8);
                    }
                }
                Event::KeyUp {
                    keycode: Some(code),
                    ..
                } => {
                    if let Some(code) = self.key_code_mapping.get(&code) {
                        key_map |= 1 << *code as u8;
                    }
                }
                Event::Quit { .. } => return PresentEvent::Quit,
                _ => {}
            }
        }
        PresentEvent::Keymap(key_map)
    }

    pub fn present_menu(&mut self, menu: &Menu) {}

    pub fn present_textures(
        &mut self,
        top: &[u32; DISPLAY_PIXEL_COUNT],
        bottom: &[u32; DISPLAY_PIXEL_COUNT],
        fps: u16,
    ) {
        let top_aligned: &[u8; DISPLAY_PIXEL_COUNT * 4] = unsafe { mem::transmute(top) };
        let bottom_aligned: &[u8; DISPLAY_PIXEL_COUNT * 4] = unsafe { mem::transmute(bottom) };
        self.texture_top
            .update(None, top_aligned, DISPLAY_WIDTH * 4)
            .unwrap();
        self.texture_bottom
            .update(None, bottom_aligned, DISPLAY_WIDTH * 4)
            .unwrap();

        self.canvas.clear();
        const ADJUSTED_DISPLAY_HEIGHT: u32 =
            PRESENTER_SCREEN_WIDTH / 2 * DISPLAY_HEIGHT as u32 / DISPLAY_WIDTH as u32;
        self.canvas
            .copy(
                &self.texture_top,
                None,
                Some(Rect::new(
                    0,
                    ((PRESENTER_SCREEN_HEIGHT - ADJUSTED_DISPLAY_HEIGHT) / 2) as _,
                    PRESENTER_SCREEN_WIDTH / 2,
                    ADJUSTED_DISPLAY_HEIGHT,
                )),
            )
            .unwrap();
        self.canvas
            .copy(
                &self.texture_bottom,
                None,
                Some(Rect::new(
                    PRESENTER_SCREEN_WIDTH as i32 / 2,
                    ((PRESENTER_SCREEN_HEIGHT - ADJUSTED_DISPLAY_HEIGHT) / 2) as _,
                    PRESENTER_SCREEN_WIDTH / 2,
                    ADJUSTED_DISPLAY_HEIGHT,
                )),
            )
            .unwrap();
        self.canvas.present();

        self.canvas
            .window_mut()
            .set_title(&format!("DSPSV - Internal fps {}", fps))
            .unwrap();
    }

    pub fn wait_vsync(&self) {}
}
