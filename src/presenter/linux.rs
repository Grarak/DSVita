use crate::cartridge_io::{CartridgeIo, CartridgePreview};
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::core::input;
use crate::global_settings::GlobalSettings;
use crate::key_bindings::KeyBinding;
use crate::logging::debug_panic;
use crate::presenter::imgui::root::{
    ImDrawData, ImGui, ImGuiCol__ImGuiCol_Text, ImGuiConfigFlags__ImGuiConfigFlags_NavEnableKeyboard, ImGuiInputTextFlags__ImGuiInputTextFlags_Password, ImGui_ImplSdlGL3_Init,
    ImGui_ImplSdlGL3_NewFrame, ImGui_ImplSdlGL3_ProcessEvent, ImGui_ImplSdlGL3_RenderDrawData, ImVec2,
};
use crate::presenter::ui::{draw_layout_preview, draw_overlay_picker, init_ui, show_main_menu, show_pause_menu, show_progress, CustomLayoutContext, RALoginContext, UiBackend, UiPauseMenuReturn};
use crate::presenter::{cjk_font, PresentEvent, PRESENTER_AUDIO_IN_BUF_SIZE, PRESENTER_AUDIO_OUT_BUF_SIZE, PRESENTER_AUDIO_OUT_SAMPLE_RATE, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use crate::ra_context::RaContext;
use crate::screen_layouts::{CustomLayout, ScreenLayouts};
use crate::settings::{Arm7Emu, Settings, DEFAULT_SETTINGS};
use crate::utils::BuildNoHasher;
use crate::DEBUG_LOG;
use clap::{arg, command, value_parser, ArgAction, ArgMatches, Command};
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
#[cfg(debug_assertions)]
use std::sync::atomic::Ordering;
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
    // DSVITA_DBG_PORT: a localhost TCP command port for headless control (buttons, touch,
    // framelimit, savestate, inst-log, quit) without a wayland virtual keyboard or signals.
    #[cfg(debug_assertions)]
    debug_state: Option<std::sync::Arc<DebugState>>,
    keymap: u32,
}

impl Presenter {
    #[cold]
    pub fn new() -> Option<Self> {
        let mut arg_matches = command!()
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
            .arg(arg!(-s <savestate> "Continue from a savestate file").num_args(1).required(false).value_parser(value_parser!(String)));
        if DEBUG_LOG {
            arg_matches = arg_matches
                .arg(
                    arg!(--"inst-log" <path> "Write a per-instruction binary log (debug builds only)")
                        .num_args(1)
                        .required(false)
                        .value_parser(value_parser!(String)),
                )
                .arg(
                    arg!(--"inst-log-lazy" <path> "Like --inst-log, but recording only starts on the debug port 'inst-log' command")
                        .num_args(1)
                        .required(false)
                        .value_parser(value_parser!(String)),
                )
                .arg(
                    // Strict cross-engine trace diffs need the raw os irq handler on both
                    // sides: the HLE substitution only exists in compiled code, so an
                    // interpreter-only run and a jit run diverge inside the handler body.
                    arg!(--"hle-irq" <bool> "0/1: override the 'HLE OS irq handler' setting (debug builds only)")
                        .num_args(1)
                        .required(false)
                        .value_parser(value_parser!(u8)),
                )
        }
        let arg_matches = arg_matches
            .arg(arg!([nds_rom] "NDS rom to run").num_args(1).required(true).value_parser(value_parser!(String)))
            .subcommand(
                Command::new("decode-inst-log")
                    .about("Decode a binary instruction log to text")
                    .arg(arg!(<path> "Log file to decode").value_parser(value_parser!(String))),
            )
            .subcommand_negates_reqs(true)
            .get_matches();

        // Offline deserializer for the binary instruction log; runs and exits without starting the emulator.
        if let Some(sub) = arg_matches.subcommand_matches("decode-inst-log") {
            crate::debug_inst_log::decode_file(sub.get_one::<String>("path").unwrap());
            return None;
        }

        if DEBUG_LOG {
            if let Some(path) = arg_matches.get_one::<String>("inst-log") {
                crate::debug_inst_log::init(path);
            }
            if let Some(path) = arg_matches.get_one::<String>("inst-log-lazy") {
                crate::debug_inst_log::init_lazy(path);
            }
        }

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

        debug_assert_eq!(gl_attr.context_profile(), GLProfile::GLES);
        debug_assert_eq!(gl_attr.context_version(), (3, 0));

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
            #[cfg(debug_assertions)]
            debug_state: std::env::var("DSVITA_DBG_PORT").ok().and_then(|p| p.parse::<u16>().ok()).map(spawn_debug_port),
            keymap: 0xFFFFFFFF,
        };

        init_ui(&mut instance);
        Some(instance)
    }

    pub fn present_ui(
        &mut self,
        screen_layouts: &mut ScreenLayouts,
        ra_context: &mut RaContext,
        cjk_download: &mut cjk_font::Download,
        default_keybinding: KeyBinding,
    ) -> Option<(CartridgeIo, GlobalSettings, Settings, PathBuf)> {
        let file_path = PathBuf::from(self.arg_matches.get_one::<String>("nds_rom").unwrap());
        if self.arg_matches.get_flag("ui") {
            if file_path.exists() && file_path.is_file() {
                eprintln!("When using ui mode then <nds_rom> must point to a directory");
                std::process::exit(1);
            }

            match show_main_menu(file_path, screen_layouts, ra_context, default_keybinding, cjk_download, self) {
                None => None,
                Some((cartridge_io, global_settings, mut settings, settings_file_path)) => {
                    screen_layouts.populate_custom_layouts(&global_settings.custom_layouts);
                    settings.populate_screen_layouts(screen_layouts);
                    settings.populate_controls(&global_settings.default_control, &global_settings.custom_controls);
                    Some((cartridge_io, global_settings, settings, settings_file_path))
                }
            }
        } else {
            let mut settings = DEFAULT_SETTINGS.clone();
            settings.set_framelimit(*self.arg_matches.get_one::<u8>("framelimit").unwrap_or(&0));
            settings.set_audio(self.arg_matches.get_flag("audio"));
            settings.set_arm7_emu(Arm7Emu::from(*self.arg_matches.get_one::<u8>("arm7_emu").unwrap_or(&0)));
            if DEBUG_LOG {
                if let Some(hle_irq) = self.arg_matches.try_get_one::<u8>("hle-irq").ok().flatten() {
                    settings.set_hle_os_irq_handler(*hle_irq != 0);
                }
            }

            let file_name = file_path.file_name().unwrap().to_str().unwrap();
            let save_path = file_path.parent().unwrap().join(format!("{file_name}.sav"));
            let preview = CartridgePreview::new(file_path.clone()).unwrap();

            ra_context.set_cache_dir(file_path.parent().unwrap().join("ra"));
            cjk_download.set_file_path(&cjk_font::font_path(&file_path.parent().unwrap()));

            let global_settings = GlobalSettings::new(file_path.parent().unwrap().join("global_settings"), default_keybinding).unwrap();
            screen_layouts.populate_custom_layouts(&global_settings.custom_layouts);
            settings.populate_screen_layouts(screen_layouts);
            settings.populate_controls(&global_settings.default_control, &global_settings.custom_controls);
            // Direct CLI launch has no per-game ini, so it isn't savable at runtime.
            Some((CartridgeIo::from_preview(preview, save_path).unwrap(), global_settings, settings, PathBuf::new()))
        }
    }

    pub fn destroy_ui(&self) {}

    pub fn on_game_launched(&self) {}

    pub fn present_pause(&mut self, gpu_renderer: &GpuRenderer, settings: &mut Settings, settings_file_path: &std::path::Path, rom_path: &std::path::Path) -> UiPauseMenuReturn {
        show_pause_menu(self, gpu_renderer, settings, settings_file_path, rom_path)
    }

    pub fn present_progress(&mut self, current_name: impl AsRef<str>, progress: usize, total: usize) {
        show_progress(self, current_name, progress, total)
    }

    pub fn present_savestate_progress(&mut self, gpu_renderer: &GpuRenderer, text: impl AsRef<str>, progress: usize) {
        crate::presenter::ui::show_savestate_progress(self, gpu_renderer, text, progress)
    }

    pub fn get_default_key_mapping() -> [u32; crate::key_bindings::NUM_KEYS] {
        [0; crate::key_bindings::NUM_KEYS]
    }

    pub fn get_default_hotkey_mapping() -> [u32; crate::key_bindings::NUM_HOTKEYS] {
        [0; crate::key_bindings::NUM_HOTKEYS]
    }

    pub fn set_key_mapping(&mut self, _: &KeyBinding) {}

    pub fn get_savestate_path(&self) -> Option<PathBuf> {
        self.arg_matches.get_one::<String>("savestate").map(PathBuf::from)
    }

    pub fn poll_event(&mut self, _: &Settings) -> PresentEvent {
        for event in self.event_pump.poll_iter() {
            match event {
                Event::KeyDown {
                    keycode: Some(keyboard::Keycode::Escape),
                    ..
                } => return PresentEvent::Pause,
                Event::KeyDown { keycode: Some(code), keymod, .. } => {
                    // F1-F9 set the framelimit to 1-9 (100%..500%), F10 uncaps it.
                    let function_keys = [
                        keyboard::Keycode::F1,
                        keyboard::Keycode::F2,
                        keyboard::Keycode::F3,
                        keyboard::Keycode::F4,
                        keyboard::Keycode::F5,
                        keyboard::Keycode::F6,
                        keyboard::Keycode::F7,
                        keyboard::Keycode::F8,
                        keyboard::Keycode::F9,
                        keyboard::Keycode::F10,
                    ];
                    if let Some(index) = function_keys.iter().position(|&key| key == code) {
                        return PresentEvent::SetFramelimit(if index == 9 { 0 } else { index as u8 + 1 });
                    }
                    if code == keyboard::Keycode::F11 {
                        crate::savestate::request_save();
                    }
                    if keymod.intersects(keyboard::Mod::LCTRLMOD | keyboard::Mod::RCTRLMOD) {
                        // Held ctrl is the hotkey layer, like a held PS button on the Vita:
                        // hotkeys trigger and the DS keys are suppressed
                        if let Some(code) = hotkey_code(code) {
                            self.keymap &= !(1 << code as u8);
                        }
                    } else if let Some(code) = self.key_code_mapping.get(&code) {
                        self.keymap &= !(1 << *code as u8);
                    }
                }
                Event::KeyUp { keycode: Some(code), .. } => {
                    if let Some(code) = hotkey_code(code) {
                        self.keymap |= 1 << code as u8;
                    }
                    if matches!(code, keyboard::Keycode::LCtrl | keyboard::Keycode::RCtrl) {
                        self.keymap |= (1 << input::Keycode::BlowMic as u8) | (1 << input::Keycode::Lid as u8);
                    }
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
        // Debug command port (DSVITA_DBG_PORT): service one-shot runtime commands (quit,
        // framelimit) as PresentEvents, and fold held buttons / touch into this frame's inputs.
        #[cfg(debug_assertions)]
        if let Some(ref st) = self.debug_state {
            if st.quit.swap(false, Ordering::Relaxed) {
                return PresentEvent::Quit;
            }
            let fl = st.pending_framelimit.swap(-1, Ordering::Relaxed);
            if fl >= 0 {
                return PresentEvent::SetFramelimit(fl as u8);
            }
        }

        let keymap;
        #[cfg(debug_assertions)]
        let mut debug_touch: Option<(i16, i16)> = None;
        #[cfg(debug_assertions)]
        {
            let mut km = self.keymap;
            if let Some(ref st) = self.debug_state {
                km &= !st.held_buttons.load(Ordering::Relaxed);
                let t = st.touch.load(Ordering::Relaxed);
                if t >= 0 {
                    debug_touch = Some(((t >> 16) as i16, (t & 0xFFFF) as i16));
                }
            }
            keymap = km;
        }
        #[cfg(not(debug_assertions))]
        {
            keymap = self.keymap;
        }

        PresentEvent::Inputs {
            keymap,
            touch: self.touch_points,
            #[cfg(debug_assertions)]
            debug_touch,
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

    pub fn core_unlocked(&self) -> bool {
        true
    }

    pub fn can_stream_screen(&self) -> bool {
        false
    }
}

// Shared state for the DSVITA_DBG_PORT command port. A background thread mutates it from socket
// commands; poll_event reads it each frame. Held-button bits are DS input::Keycode positions.
#[cfg(debug_assertions)]
struct DebugState {
    held_buttons: std::sync::atomic::AtomicU32,
    touch: std::sync::atomic::AtomicI32, // (x << 16) | y, or -1 for released
    pending_framelimit: std::sync::atomic::AtomicI32, // -1 = none, else 0..=9
    quit: std::sync::atomic::AtomicBool,
}

// Spawn the debug command port on 127.0.0.1:<port>. Newline-delimited text commands:
//   press/release <btn> | buttons [<btn>...] | touch <x> <y> | touch off |
//   framelimit <0..9> | savestate | inst-log | quit
// btn: a b x y up down left right start select l r. inst-log arms --inst-log-lazy capture.
// Replies "ok" or "err: ...".
#[cfg(debug_assertions)]
fn spawn_debug_port(port: u16) -> std::sync::Arc<DebugState> {
    use std::io::{BufRead, BufReader, Write};
    let state = std::sync::Arc::new(DebugState {
        held_buttons: std::sync::atomic::AtomicU32::new(0),
        touch: std::sync::atomic::AtomicI32::new(-1),
        pending_framelimit: std::sync::atomic::AtomicI32::new(-1),
        quit: std::sync::atomic::AtomicBool::new(false),
    });
    let srv = state.clone();
    thread::Builder::new()
        .name("dbg_port".to_owned())
        .spawn(move || {
            let listener = match std::net::TcpListener::bind(("127.0.0.1", port)) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("[dbg_port] bind 127.0.0.1:{port} failed: {e}");
                    return;
                }
            };
            eprintln!("[dbg_port] listening on 127.0.0.1:{port}");
            for stream in listener.incoming().flatten() {
                let mut writer = match stream.try_clone() {
                    Ok(w) => w,
                    Err(_) => continue,
                };
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    let reply = handle_debug_cmd(&srv, line.trim());
                    if writeln!(writer, "{reply}").is_err() {
                        break;
                    }
                }
            }
        })
        .unwrap();
    state
}

#[cfg(debug_assertions)]
fn handle_debug_cmd(state: &DebugState, line: &str) -> String {
    let mut it = line.split_whitespace();
    let cmd = it.next().unwrap_or("");
    match cmd {
        "" => "ok".to_owned(),
        "press" | "release" => {
            let Some(code) = it.next().and_then(debug_input_key) else {
                return "err: unknown button".to_owned();
            };
            let bit = 1u32 << code as u8;
            if cmd == "press" {
                state.held_buttons.fetch_or(bit, Ordering::Relaxed);
            } else {
                state.held_buttons.fetch_and(!bit, Ordering::Relaxed);
            }
            "ok".to_owned()
        }
        "buttons" => {
            let mut mask = 0u32;
            for tok in it {
                match debug_input_key(tok) {
                    Some(code) => mask |= 1 << code as u8,
                    None => return format!("err: unknown button '{tok}'"),
                }
            }
            state.held_buttons.store(mask, Ordering::Relaxed);
            "ok".to_owned()
        }
        "touch" => {
            match it.next() {
                Some("off") | None => state.touch.store(-1, Ordering::Relaxed),
                Some(x) => {
                    let (Ok(x), Some(Ok(y))) = (x.parse::<i32>(), it.next().map(str::parse::<i32>)) else {
                        return "err: touch <x> <y> | touch off".to_owned();
                    };
                    if !(0..256).contains(&x) || !(0..192).contains(&y) {
                        return "err: touch out of range (x 0..256, y 0..192)".to_owned();
                    }
                    state.touch.store((x << 16) | y, Ordering::Relaxed);
                }
            }
            "ok".to_owned()
        }
        "framelimit" => match it.next().and_then(|s| s.parse::<i32>().ok()) {
            Some(n @ 0..=9) => {
                state.pending_framelimit.store(n, Ordering::Relaxed);
                "ok".to_owned()
            }
            _ => "err: framelimit <0..9>".to_owned(),
        },
        "savestate" => {
            crate::savestate::request_save();
            "ok".to_owned()
        }
        "inst-log" => {
            crate::debug_inst_log::arm_lazy();
            "ok".to_owned()
        }
        "quit" => {
            state.quit.store(true, Ordering::Relaxed);
            "ok".to_owned()
        }
        other => format!("err: unknown cmd '{other}'"),
    }
}

// Debug-only DS button names → input::Keycode, for the debug command port.
#[cfg(debug_assertions)]
fn debug_input_key(name: &str) -> Option<input::Keycode> {
    use input::Keycode::*;
    Some(match name {
        "a" => A,
        "b" => B,
        "x" => X,
        "y" => Y,
        "up" => Up,
        "down" => Down,
        "left" => Left,
        "right" => Right,
        "start" => Start,
        "select" => Select,
        "l" => TriggerL,
        "r" => TriggerR,
        _ => return None,
    })
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

unsafe fn input_dimension(label: &CStr, value: &mut u16) {
    let mut v = *value as i32;
    if ImGui::InputInt(label.as_ptr(), &mut v, 1, 10, 0) {
        *value = v.clamp(0, u16::MAX as i32) as u16;
    }
}

pub fn show_layout_create_settings(global_settings: &mut GlobalSettings, custom_layout_context: &mut CustomLayoutContext, custom_layout: &mut CustomLayout) -> bool {
    unsafe {
        // Reserve the Save button (plus one error line only when an error is
        // shown) so the fields/preview span all the way down to the button.
        let has_error = custom_layout_context.empty_name || custom_layout_context.duplicated_name;
        let mut footer = ImGui::GetFrameHeightWithSpacing();
        if has_error {
            footer += ImGui::GetTextLineHeightWithSpacing();
        }
        let body_height = (ImGui::GetContentRegionAvail().y - footer).max(0.0);

        let fields_sz = ImVec2 { x: 430.0, y: body_height };
        if ImGui::BeginChild(c"##layout_fields".as_ptr(), &fields_sz, false, 0) {
            ImGui::PushItemWidth(150.0);

            let mut name = [0u8; 33];
            let len = min(name.len() - 1, custom_layout.name.len());
            name[..len].copy_from_slice(&custom_layout.name.as_bytes()[..len]);
            if ImGui::InputText(c"Layout name".as_ptr(), name.as_mut_ptr(), name.len(), 0, None, ptr::null_mut()) {
                custom_layout.name = CStr::from_ptr(name.as_ptr()).to_str().unwrap_or("").to_string();
            }

            for (i, screen) in [(0usize, c"Top screen"), (1usize, c"Bottom screen")] {
                ImGui::Spacing();
                ImGui::Separator();
                ImGui::TextDisabled(screen.as_ptr());
                ImGui::PushID3(i as _);
                input_dimension(c"Width", &mut custom_layout.sizes[i].0);
                input_dimension(c"Height", &mut custom_layout.sizes[i].1);
                input_dimension(c"Position X", &mut custom_layout.pos[i].0);
                input_dimension(c"Position Y", &mut custom_layout.pos[i].1);
                input_dimension(c"Rotation", &mut custom_layout.rotation[i]);
                ImGui::PopID();
            }

            ImGui::Spacing();
            ImGui::Separator();
            ImGui::InputFloat(c"Widescreen coefficient".as_ptr(), &mut custom_layout.wide_screen_coefficient, 0.05, 0.5, c"%.3f".as_ptr(), 0);

            ImGui::PopItemWidth();

            draw_overlay_picker(custom_layout);
        }
        ImGui::EndChild();

        ImGui::SameLine(0.0, (*ImGui::GetStyle()).ItemSpacing.x);

        let preview_sz = ImVec2 { x: 0.0, y: body_height };
        if ImGui::BeginChild(c"##layout_preview".as_ptr(), &preview_sz, false, 0) {
            draw_layout_preview(custom_layout);
        }
        ImGui::EndChild();

        if has_error {
            ImGui::PushStyleColor(ImGuiCol__ImGuiCol_Text as _, 0xFF0000FF);
            if custom_layout_context.empty_name {
                ImGui::Text(c"Layout name can't be empty".as_ptr());
            } else {
                ImGui::Text(c"A layout with that name already exists".as_ptr());
            }
            ImGui::PopStyleColor(1);
        }

        let vec = ImVec2 { x: -1.0, y: 0.0 };
        if ImGui::Button(c"Save layout".as_ptr(), &vec) {
            if custom_layout.name.is_empty() {
                custom_layout_context.empty_name = true;
                custom_layout_context.duplicated_name = false;
            } else if global_settings.add_custom_layout(custom_layout.clone()) {
                return true;
            } else {
                custom_layout_context.empty_name = false;
                custom_layout_context.duplicated_name = true;
            }
        }
        false
    }
}

/// The keyboard hotkeys, active while ctrl is held (the Vita's PS-button layer).
fn hotkey_code(code: keyboard::Keycode) -> Option<input::Keycode> {
    match code {
        keyboard::Keycode::M => Some(input::Keycode::BlowMic),
        keyboard::Keycode::N => Some(input::Keycode::Lid),
        _ => None,
    }
}

pub fn default_key_binding() -> crate::key_bindings::KeyBinding {
    crate::key_bindings::KeyBinding::default()
}

pub fn show_controls_create_settings(_: &mut GlobalSettings, _: &mut CustomLayoutContext, _: &mut crate::key_bindings::KeyBinding) -> bool {
    unsafe {
        ImGui::Text(c"Custom controls are only available on the Vita.".as_ptr());
    }
    false
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
