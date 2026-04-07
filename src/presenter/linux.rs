use crate::cartridge_io::{CartridgeIo, CartridgePreview};
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::core::input;
use crate::global_settings::GlobalSettings;
use crate::logging::debug_panic;
use crate::presenter::imgui::root::{
    ImDrawData, ImGui, ImGuiCol__ImGuiCol_Text, ImGuiConfigFlags__ImGuiConfigFlags_NavEnableKeyboard, ImGuiInputTextFlags__ImGuiInputTextFlags_Password, ImGui_ImplSdlGL3_Init,
    ImGui_ImplSdlGL3_NewFrame, ImGui_ImplSdlGL3_ProcessEvent, ImGui_ImplSdlGL3_RenderDrawData, ImVec2,
};
use crate::presenter::ui::{init_ui, show_main_menu, show_pause_menu, show_progress, CustomLayoutContext, RALoginContext, UiBackend, UiPauseMenuReturn};
use crate::presenter::{PresentEvent, PRESENTER_AUDIO_IN_BUF_SIZE, PRESENTER_AUDIO_OUT_BUF_SIZE, PRESENTER_AUDIO_OUT_SAMPLE_RATE, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use crate::ra_context::RaContext;
use crate::screen_layouts::{CustomLayout, ScreenLayouts};
use crate::settings::{Arm7Emu, Settings, DEFAULT_SETTINGS};
use crate::utils::BuildNoHasher;
use clap::{arg, command, value_parser, ArgAction, ArgMatches};
use gl::types::GLuint;
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::event::{Event, EventType};
use sdl2::mouse::MouseButton;
use sdl2::video::{GLContext, GLProfile, Window};
use sdl2::{keyboard, EventPump};
use std::cmp::min;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::ops::BitOrAssign;
use std::path::PathBuf;
use std::rc::Rc;
use std::str::FromStr;
use std::{mem, ptr, slice, thread};

#[derive(Clone)]
pub struct PresenterAudioOut {
    audio_queue: Rc<Option<AudioQueue<i16>>>,
}

unsafe impl Send for PresenterAudioOut {}

impl PresenterAudioOut {
    fn new(audio_queue: Option<AudioQueue<i16>>) -> Self {
        PresenterAudioOut { audio_queue: Rc::new(audio_queue) }
    }

    pub fn play(&self, buffer: &[u32; PRESENTER_AUDIO_OUT_BUF_SIZE]) {
        let raw = unsafe { slice::from_raw_parts(buffer.as_ptr() as *const i16, PRESENTER_AUDIO_OUT_BUF_SIZE * 2) };
        if let Some(audio_queue) = self.audio_queue.as_ref() {
            audio_queue.queue_audio(raw).unwrap();
            while audio_queue.size() != 0 {
                thread::yield_now();
            }
        }
    }
}

pub struct PresenterAudioIn;

unsafe impl Send for PresenterAudioIn {}

impl PresenterAudioIn {
    pub fn receive(&self, _: &mut [i16; PRESENTER_AUDIO_IN_BUF_SIZE]) {}
}

pub struct Presenter {
    arg_matches: ArgMatches,
    presenter_audio_out: PresenterAudioOut,
    window: Window,
    _gl_ctx: GLContext,
    key_code_mapping: HashMap<keyboard::Keycode, input::Keycode, BuildNoHasher>,
    event_pump: EventPump,
    mouse_pressed: bool,
    mouse_id: Option<u32>,
    touch_points: Option<(i16, i16)>,
    keymap: u32,
}

impl Presenter {
    #[cold]
    pub fn new() -> Option<Self> {
        let arg_matches = command!()
            .arg(
                arg!(-f <framelimit> "0: No 1: 100%, 2: 200%, 3: 300%")
                    .num_args(1)
                    .required(false)
                    .default_value("0")
                    .value_parser(value_parser!(u8)),
            )
            .arg(arg!(audio: -a "Enable audio").required(false).action(ArgAction::SetTrue))
            .arg(
                arg!(-e <arm7_emu> "0: Accurate, 1: SoundHle, 2: Hle")
                    .num_args(1)
                    .required(false)
                    .default_value("0")
                    .value_parser(value_parser!(u8)),
            )
            .arg(arg!(ui: --ui "Use UI").required(false).action(ArgAction::SetTrue))
            .arg(arg!([nds_rom] "NDS rom to run").num_args(1).required(true).value_parser(value_parser!(String)))
            .get_matches();

        sdl2::hint::set("SDL_NO_SIGNAL_HANDLERS", "1");
        let sdl = sdl2::init().unwrap();
        let sdl_video = sdl.video().unwrap();
        let audio_queue = sdl
            .audio()
            .and_then(|sdl_audio| {
                sdl_audio
                    .open_queue(
                        None,
                        &AudioSpecDesired {
                            freq: Some(PRESENTER_AUDIO_OUT_SAMPLE_RATE as i32),
                            channels: Some(2),
                            samples: Some(PRESENTER_AUDIO_OUT_BUF_SIZE as u16),
                        },
                    )
                    .and_then(|audio_queue| {
                        audio_queue.resume();
                        Ok(audio_queue)
                    })
            })
            .ok();

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
            presenter_audio_out: PresenterAudioOut::new(audio_queue),
            window,
            _gl_ctx: gl_ctx,
            key_code_mapping,
            event_pump,
            mouse_pressed: false,
            mouse_id: None,
            touch_points: None,
            keymap: 0xFFFFFFFF,
        };

        init_ui(&mut instance);
        Some(instance)
    }

    pub fn present_ui(&mut self, screen_layouts: &mut ScreenLayouts, ra_context: &mut RaContext) -> Option<(CartridgeIo, GlobalSettings, Settings)> {
        let file_path = PathBuf::from(self.arg_matches.get_one::<String>("nds_rom").unwrap());
        if self.arg_matches.get_flag("ui") {
            if file_path.exists() && file_path.is_file() {
                eprintln!("When using ui mode then <nds_rom> must point to a directory");
                std::process::exit(1);
            }

            match show_main_menu(file_path, screen_layouts, ra_context, self) {
                None => None,
                Some((cartridge_io, global_settings, mut settings)) => {
                    screen_layouts.populate_custom_layouts(&global_settings.custom_layouts);
                    settings.populate_screen_layouts(screen_layouts);
                    Some((cartridge_io, global_settings, settings))
                }
            }
        } else {
            let mut settings = DEFAULT_SETTINGS.clone();
            settings.set_framelimit(*self.arg_matches.get_one::<u8>("framelimit").unwrap_or(&0));
            settings.set_audio(self.arg_matches.get_flag("audio"));
            settings.set_arm7_emu(Arm7Emu::from(*self.arg_matches.get_one::<u8>("arm7_emu").unwrap_or(&0)));

            let file_name = file_path.file_name().unwrap().to_str().unwrap();
            let save_path = file_path.parent().unwrap().join(format!("{file_name}.sav"));
            let preview = CartridgePreview::new(file_path.clone()).unwrap();

            ra_context.set_cache_dir(file_path.parent().unwrap().join("ra"));

            let global_settings = GlobalSettings::new(file_path.parent().unwrap().join("global_settings")).unwrap();
            screen_layouts.populate_custom_layouts(&global_settings.custom_layouts);
            settings.populate_screen_layouts(screen_layouts);
            Some((CartridgeIo::from_preview(preview, save_path).unwrap(), global_settings, settings))
        }
    }

    pub fn destroy_ui(&self) {}

    pub fn on_game_launched(&self) {}

    pub fn present_pause(&mut self, gpu_renderer: &GpuRenderer, settings: &mut Settings) -> UiPauseMenuReturn {
        show_pause_menu(self, gpu_renderer, settings)
    }

    pub fn present_progress(&mut self, current_name: impl AsRef<str>, progress: usize, total: usize) {
        show_progress(self, current_name, progress, total)
    }

    pub fn poll_event(&mut self, _: &Settings) -> PresentEvent {
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
                    self.touch_points = None;
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
                    self.touch_points = Some((x as i16, y as i16));
                }
                Event::MouseMotion { which, x, y, .. } => {
                    if let Some(mouse_id) = self.mouse_id {
                        if self.mouse_pressed && mouse_id == which {
                            self.touch_points = Some((x as i16, y as i16));
                        }
                    }
                }
                Event::Quit { .. } => return PresentEvent::Quit,
                _ => {}
            }
        }
        PresentEvent::Inputs {
            keymap: self.keymap,
            touch: self.touch_points,
        }
    }

    pub fn gl_swap_window(&self) {
        self.window.gl_swap_window();
    }

    pub fn get_presenter_audio_out(&self) -> PresenterAudioOut {
        self.presenter_audio_out.clone()
    }

    pub fn get_presenter_audio_in(&self) -> PresenterAudioIn {
        PresenterAudioIn
    }

    pub fn wait_vsync(&self) {}

    pub fn gl_create_depth_tex() -> GLuint {
        0
    }

    pub fn gl_tex_image_2d_rgba5(_: i32, _: i32) {
        debug_panic!()
    }

    pub fn gl_version_suffix() -> &'static str {
        ""
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

pub fn show_layout_create_settings(_: &mut GlobalSettings, _: &mut CustomLayoutContext, _: &mut CustomLayout) -> bool {
    true
}

pub fn show_retroachievements_settings(global_settings: &mut GlobalSettings, login_context: &mut RALoginContext, context: &mut RaContext) {
    unsafe {
        if !global_settings.ra_username.is_empty() && !global_settings.ra_token.is_empty() {
            let msg = format!("Currently logged in as {}", global_settings.ra_username);
            ImGui::Text(CString::from_str(&msg).unwrap().as_ptr());
        }

        let mut username = [0; 128];
        let len = min(username.len() - 1, login_context.username.len());
        username[..len].copy_from_slice(&login_context.username.as_bytes()[..len]);
        if ImGui::InputText(c"Username".as_ptr(), username.as_mut_ptr(), username.len(), 0, None, ptr::null_mut()) {
            login_context.username = CStr::from_ptr(username.as_ptr()).to_str().unwrap().to_string();
        }

        let mut password = [0; 128];
        let len = min(password.len() - 1, login_context.password.len());
        password[..len].copy_from_slice(&login_context.password.as_bytes()[..len]);
        if ImGui::InputText(
            c"Password".as_ptr(),
            password.as_mut_ptr(),
            password.len(),
            ImGuiInputTextFlags__ImGuiInputTextFlags_Password as _,
            None,
            ptr::null_mut(),
        ) {
            login_context.password = CStr::from_ptr(password.as_ptr()).to_str().unwrap().to_string();
        }

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
