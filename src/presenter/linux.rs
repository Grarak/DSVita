use crate::cartridge_io::{CartridgeIo, CartridgePreview};
use crate::core::graphics::gpu::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::core::input;
use crate::presenter::imgui::root::{
    ImDrawData, ImGui, ImGuiConfigFlags__ImGuiConfigFlags_NavEnableKeyboard, ImGui_ImplSdlGL3_Init, ImGui_ImplSdlGL3_NewFrame, ImGui_ImplSdlGL3_ProcessEvent, ImGui_ImplSdlGL3_RenderDrawData,
};
use crate::presenter::ui::{init_ui, show_main_menu, show_pause_menu, UiBackend, UiPauseMenuReturn};
use crate::presenter::{PresentEvent, PRESENTER_AUDIO_BUF_SIZE, PRESENTER_AUDIO_SAMPLE_RATE, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH, PRESENTER_SUB_BOTTOM_SCREEN};
use crate::settings::{Arm7Emu, ScreenMode, SettingValue, Settings, DEFAULT_SETTINGS};
use crate::utils::BuildNoHasher;
use clap::{arg, command, value_parser, ArgAction, ArgMatches};
use gl::types::GLuint;
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::event::{Event, EventType};
use sdl2::mouse::MouseButton;
use sdl2::video::{GLContext, GLProfile, Window};
use sdl2::{keyboard, EventPump};
use std::collections::HashMap;
use std::ops::BitOrAssign;
use std::path::PathBuf;
use std::rc::Rc;
use std::{mem, ptr, slice, thread};

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
        let raw = unsafe { slice::from_raw_parts(buffer.as_ptr() as *const i16, PRESENTER_AUDIO_BUF_SIZE * 2) };
        self.audio_queue.queue_audio(raw).unwrap();
        while self.audio_queue.size() != 0 {
            thread::yield_now();
        }
    }
}

pub struct Presenter {
    arg_matches: ArgMatches,
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
    #[cold]
    pub fn new() -> Self {
        let arg_matches = command!()
            .arg(arg!(framelimit: -f "Enable framelimit").required(false).action(ArgAction::SetTrue))
            .arg(arg!(audio: -a "Enable audio").required(false).action(ArgAction::SetTrue))
            .arg(
                arg!(-e <arm7_emu> "0: Accurate, 1: Partial, 2: Partial with Sound, 3: Hle")
                    .num_args(1)
                    .required(false)
                    .default_value("0")
                    .value_parser(value_parser!(u8)),
            )
            .arg(arg!(enable_arm7_block_validation: -b "Enable arm7 block validation").required(false).action(ArgAction::SetTrue))
            .arg(arg!(ui: --ui "Use UI").required(false).action(ArgAction::SetTrue))
            .arg(arg!([nds_rom] "NDS rom to run").num_args(1).required(true).value_parser(value_parser!(String)))
            .get_matches();

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

        let mut instance = Presenter {
            arg_matches,
            presenter_audio: PresenterAudio::new(audio_queue),
            window,
            _gl_ctx: gl_ctx,
            key_code_mapping,
            event_pump,
            mouse_pressed: false,
            mouse_id: None,
            keymap: 0xFFFFFFFF,
        };

        init_ui(&mut instance);
        instance
    }

    pub fn present_ui(&mut self) -> Option<(CartridgeIo, Settings)> {
        let file_path = PathBuf::from(self.arg_matches.get_one::<String>("nds_rom").unwrap());
        if self.arg_matches.get_flag("ui") {
            if file_path.exists() && file_path.is_file() {
                eprintln!("When using ui mode then <nds_rom> must point to a directory");
                std::process::exit(1);
            }

            show_main_menu(file_path, self)
        } else {
            let mut settings = DEFAULT_SETTINGS.clone();
            settings.setting_framelimit_mut().value = SettingValue::Bool(self.arg_matches.get_flag("framelimit"));
            settings.setting_audio_mut().value = SettingValue::Bool(self.arg_matches.get_flag("audio"));
            settings.setting_arm7_hle_mut().value = SettingValue::Arm7Emu(Arm7Emu::from(*self.arg_matches.get_one::<u8>("arm7_emu").unwrap_or(&0)));
            settings.setting_arm7_block_validation_mut().value = SettingValue::Bool(self.arg_matches.get_flag("enable_arm7_block_validation"));

            let file_name = file_path.file_name().unwrap().to_str().unwrap();
            let save_path = file_path.parent().unwrap().join(format!("{file_name}.sav"));
            let preview = CartridgePreview::new(file_path).unwrap();
            Some((CartridgeIo::from_preview(preview, save_path).unwrap(), settings))
        }
    }

    pub fn destroy_ui(&self) {}

    pub fn on_game_launched(&self) {}

    pub fn present_pause(&mut self, gpu_renderer: &GpuRenderer, settings: &mut Settings) -> UiPauseMenuReturn {
        show_pause_menu(self, gpu_renderer, settings)
    }

    pub fn poll_event(&mut self, _: ScreenMode) -> PresentEvent {
        let mut touch = None;

        let mut sample_touch_points = |x, y| {
            let (x, y) = PRESENTER_SUB_BOTTOM_SCREEN.normalize(x as _, y as _);
            let x = (DISPLAY_WIDTH as u32 * x / PRESENTER_SUB_BOTTOM_SCREEN.width) as u8;
            let y = (DISPLAY_HEIGHT as u32 * y / PRESENTER_SUB_BOTTOM_SCREEN.height) as u8;
            touch = Some((x, y));
        };

        for event in self.event_pump.poll_iter() {
            match event {
                Event::KeyDown {
                    keycode: Some(keyboard::Keycode::Escape),
                    ..
                } => return PresentEvent::Pause,
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

impl UiBackend for Presenter {
    fn init(&mut self) {
        unsafe {
            (*ImGui::GetIO()).ConfigFlags.bitor_assign(ImGuiConfigFlags__ImGuiConfigFlags_NavEnableKeyboard as i32);
            ImGui_ImplSdlGL3_Init(self.window.raw() as _, ptr::null())
        };
    }

    fn new_frame(&mut self) -> bool {
        unsafe {
            let mut event: sdl2::sys::SDL_Event = mem::zeroed();
            while sdl2::sys::SDL_PollEvent(&mut event) != 0 {
                if let Ok(event) = EventType::try_from(event.type_) {
                    if event == EventType::Quit {
                        return false;
                    }
                }
                ImGui_ImplSdlGL3_ProcessEvent(ptr::addr_of_mut!(event) as _);
            }

            ImGui_ImplSdlGL3_NewFrame(self.window.raw() as _);
            true
        }
    }

    fn render_draw_data(&mut self, draw_data: *mut ImDrawData) {
        unsafe { ImGui_ImplSdlGL3_RenderDrawData(draw_data) };
    }

    fn swap_window(&mut self) {
        self.gl_swap_window();
    }
}
