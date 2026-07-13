// Native Android presenter: a plain Activity hands us a Surface, storage path, input
// and lifecycle events over JNI; this file owns EGL and AAudio. ALL UI (rom browser,
// pause menu, settings, dialogs) is the Activity's job — the native side renders only
// emulator frames and blocks in present_ui/present_pause until Java tells it what to do.
//
// Java side contract (DSVitaActivity):
//   System.loadLibrary("dsvita")
//   nativeInit(getExternalFilesDir(null).getAbsolutePath())   // once, before any surface
//   nativeSurfaceCreated(holder.getSurface()) / nativeSurfaceDestroyed()
//   nativeLaunch(romPath)             // start a game; native was blocking in present_ui
//   nativeTouch(x, y, down) / nativeKey(dsKey, down)          // dsKey = input::Keycode
//   nativePause()                     // -> native blocks in present_pause
//   nativePauseChoice(choice)         // 0 Resume, 1 BlowMic, 2 Quit(to browser), 3 QuitApp
//   nativeResume()

use crate::cartridge_io::{CartridgeIo, CartridgePreview};
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::core::input;
use crate::global_settings::GlobalSettings;
use crate::key_bindings::KeyBinding;
use crate::logging::debug_panic;
use crate::mmap::PAGE_SIZE;
use crate::presenter::{PresentEvent, UiPauseMenuReturn, PRESENTER_AUDIO_IN_BUF_SIZE, PRESENTER_AUDIO_OUT_BUF_SIZE, PRESENTER_AUDIO_OUT_SAMPLE_RATE, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use crate::ra_context::RaContext;
use crate::screen_layouts::ScreenLayouts;
use crate::settings::{SettingGroup, SettingValue, Settings, SettingsConfig};
use jni_sys::{jboolean, jint, jobject, jstring, JNIEnv};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::os::fd::FromRawFd;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread;

// ---------------------------------------------------------------------------- logcat

const ANDROID_LOG_INFO: c_int = 4;
const ANDROID_LOG_FATAL: c_int = 7;

extern "C" {
    fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

fn log_android(prio: c_int, text: &str) {
    let tag = c"dsvita";
    let text = CString::new(text).unwrap_or_else(|_| c"<invalid log line>".into());
    unsafe { __android_log_write(prio, tag.as_ptr(), text.as_ptr()) };
}

// bionic wires fds 1/2 to /dev/null; everything the emulator prints would vanish.
// Replace them with a pipe drained by a thread into logcat.
fn redirect_stdio_to_logcat() {
    unsafe {
        let mut fds = [0 as c_int; 2];
        if libc::pipe2(fds.as_mut_ptr(), 0) != 0 {
            return;
        }
        libc::dup2(fds[1], 1);
        libc::dup2(fds[1], 2);
        libc::close(fds[1]);
        let read_end = fds[0];
        thread::Builder::new()
            .name("logcat".to_string())
            .spawn(move || {
                let reader = BufReader::new(File::from_raw_fd(read_end));
                for line in reader.lines() {
                    match line {
                        Ok(line) => log_android(ANDROID_LOG_INFO, &line),
                        Err(_) => break,
                    }
                }
            })
            .unwrap();
    }
}

// ------------------------------------------------------------------- shared with java

// Written once in nativeInit before the emulator thread spawns, read-only afterwards.
static mut STORAGE_DIR: Option<PathBuf> = None;

/// App-specific external storage (Android/data/<pkg>/files) — adb-pushable without any
/// permission, user-visible in a file manager. Roms and all per-rom data dirs live here.
pub fn storage_dir() -> &'static Path {
    unsafe { STORAGE_DIR.as_ref().expect("nativeInit must run before the emulator starts").as_path() }
}

// Current ANativeWindow (0 = none). The emulator thread parks on the condvar until the
// first surface arrives; surface loss just swaps the value (handled on the next present).
static SURFACE: Mutex<usize> = Mutex::new(0);
static SURFACE_COND: Condvar = Condvar::new();

// Rom the Activity asked to launch; present_ui blocks on this.
static LAUNCH_REQUEST: Mutex<Option<PathBuf>> = Mutex::new(None);
static LAUNCH_COND: Condvar = Condvar::new();

// Pause-dialog outcome; present_pause blocks on this.
static PAUSE_CHOICE: Mutex<Option<UiPauseMenuReturn>> = Mutex::new(None);
static PAUSE_COND: Condvar = Condvar::new();

// DS button bits held down (active-high; poll_event inverts into the active-low keymap).
static DS_KEYS_HELD: AtomicU32 = AtomicU32::new(0);
// Touch in surface pixels: 0xFFFF_FFFF = up, else (x << 16) | y.
static TOUCH: AtomicU32 = AtomicU32::new(u32::MAX);
static PAUSE_REQUESTED: AtomicBool = AtomicBool::new(false);

// --------------------------------------------------------------------------- JNI glue

extern "C" {
    fn ANativeWindow_fromSurface(env: *mut JNIEnv, surface: jobject) -> *mut c_void;
    fn ANativeWindow_release(window: *mut c_void);
}

unsafe fn jstring_to_string(env: *mut JNIEnv, s: jstring) -> String {
    let chars = (**env).GetStringUTFChars.unwrap()(env, s, ptr::null_mut());
    let result = CStr::from_ptr(chars).to_string_lossy().into_owned();
    (**env).ReleaseStringUTFChars.unwrap()(env, s, chars);
    result
}

#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativeInit(env: *mut JNIEnv, _class: jobject, storage_path: jstring) {
    redirect_stdio_to_logcat();

    // The fastmem layout hardcodes 4 KiB host pages (mmap/mod.rs); a 16 KiB-page device
    // would corrupt guest memory through misaligned mirror maps. Refuse loudly.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size != PAGE_SIZE as i64 {
        let msg = format!("DSVita requires a 4 KiB-page device; this kernel uses {page_size}-byte pages");
        log_android(ANDROID_LOG_FATAL, &msg);
        eprintln!("{msg}");
        unsafe { libc::abort() };
    }

    let storage = PathBuf::from(unsafe { jstring_to_string(env, storage_path) });
    std::fs::create_dir_all(&storage).ok();
    unsafe { STORAGE_DIR = Some(storage) };

    // The debug tooling is driven by DSVITA_* env vars, but an Android app can't be
    // handed env from adb. Debug builds read <storage>/dsvita.env (KEY=VALUE lines).
    #[cfg(debug_assertions)]
    load_env_file();

    // Same shape as the desktop bin main: the emulator on its own 4 MiB-stack thread.
    // Detached — this JNI call must return to Java; the thread lives for the process.
    thread::Builder::new().name("actual_main".to_string()).stack_size(4 * 1024 * 1024).spawn(crate::actual_main).unwrap();
}

#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativeSurfaceCreated(env: *mut JNIEnv, _class: jobject, surface: jobject) {
    let window = unsafe { ANativeWindow_fromSurface(env, surface) };
    let mut guard = SURFACE.lock().unwrap();
    let old = *guard;
    *guard = window as usize;
    SURFACE_COND.notify_all();
    if old != 0 {
        unsafe { ANativeWindow_release(old as _) };
    }
}

#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativeSurfaceDestroyed(_env: *mut JNIEnv, _class: jobject) {
    let mut guard = SURFACE.lock().unwrap();
    let old = *guard;
    *guard = 0;
    if old != 0 {
        unsafe { ANativeWindow_release(old as _) };
    }
}

#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativeLaunch(env: *mut JNIEnv, _class: jobject, rom_path: jstring) {
    let path = PathBuf::from(unsafe { jstring_to_string(env, rom_path) });
    *LAUNCH_REQUEST.lock().unwrap() = Some(path);
    LAUNCH_COND.notify_all();
}

#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativeTouch(_env: *mut JNIEnv, _class: jobject, x: jint, y: jint, down: jboolean) {
    if down != 0 {
        TOUCH.store(((x as u32 & 0xFFFF) << 16) | (y as u32 & 0xFFFF), Ordering::Relaxed);
    } else {
        TOUCH.store(u32::MAX, Ordering::Relaxed);
    }
}

/// key = a `input::Keycode` discriminant the Java side maps physical buttons onto.
#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativeKey(_env: *mut JNIEnv, _class: jobject, key: jint, down: jboolean) {
    if key < 0 || key > input::Keycode::Lid as jint {
        return;
    }
    let bit = 1u32 << key;
    if down != 0 {
        DS_KEYS_HELD.fetch_or(bit, Ordering::Relaxed);
    } else {
        DS_KEYS_HELD.fetch_and(!bit, Ordering::Relaxed);
    }
}

/// Freeze the emulator; the main loop lands in present_pause and waits for
/// nativePauseChoice.
#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativePause(_env: *mut JNIEnv, _class: jobject) {
    PAUSE_REQUESTED.store(true, Ordering::Relaxed);
}

/// choice: 0 Resume, 1 BlowMic, 2 Quit (back to the browser), 3 QuitApp.
#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativePauseChoice(_env: *mut JNIEnv, _class: jobject, choice: jint) {
    let choice = match choice {
        1 => UiPauseMenuReturn::BlowMic,
        2 => UiPauseMenuReturn::Quit,
        3 => UiPauseMenuReturn::QuitApp,
        _ => UiPauseMenuReturn::Resume,
    };
    *PAUSE_CHOICE.lock().unwrap() = Some(choice);
    PAUSE_COND.notify_all();
}

#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativeResume(_env: *mut JNIEnv, _class: jobject) {}

#[cfg(debug_assertions)]
fn load_env_file() {
    let Ok(content) = std::fs::read_to_string(storage_dir().join("dsvita.env")) else {
        return;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            std::env::set_var(key.trim(), value.trim());
            println!("dsvita.env: {}={}", key.trim(), value.trim());
        }
    }
}

// ------------------------------------------------------------------ settings bridge

// The Activity renders settings generically from this JSON — the Rust definitions stay
// the single source of truth (titles, options, groups, runtime flags).
fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn serialize_settings(settings: &mut Settings) -> String {
    let mut out = String::from("[");
    for (i, setting) in settings.get_all_mut().iter_mut().enumerate() {
        if i != 0 {
            out.push(',');
        }
        let group = match setting.group {
            SettingGroup::Emulation => "Emulation",
            SettingGroup::Graphics => "Graphics",
            SettingGroup::Screen => "Screen",
            SettingGroup::System => "System",
        };
        out += &format!(
            "{{\"idx\":{i},\"title\":\"{}\",\"desc\":\"{}\",\"group\":\"{group}\",\"runtime\":{},",
            json_escape(setting.title),
            json_escape(setting.description),
            setting.runtime
        );
        match &setting.value {
            SettingValue::Bool(value) => out += &format!("\"kind\":\"bool\",\"value\":{value}}}"),
            SettingValue::List(inner) => {
                let options: Vec<String> = inner.values.iter().map(|v| format!("\"{}\"", json_escape(v))).collect();
                out += &format!("\"kind\":\"list\",\"selection\":{},\"options\":[{}]}}", inner.selection, options.join(","));
            }
            SettingValue::Int(value) => out += &format!("\"kind\":\"int\",\"value\":{value}}}"),
        }
    }
    out.push(']');
    out
}

// Per-game config with the dynamic option lists (layouts, control profiles) populated
// exactly like a launch does.
fn build_game_config(rom_path: &Path) -> SettingsConfig {
    let storage = storage_dir();
    let file_name = rom_path.file_name().unwrap().to_str().unwrap();
    let mut config = SettingsConfig::new(storage.join("settings").join(format!("{file_name}.ini")));
    let mut screen_layouts = ScreenLayouts::new();
    let global_settings = GlobalSettings::new(storage.join("global_settings"), default_key_binding()).unwrap();
    screen_layouts.populate_custom_layouts(&global_settings.custom_layouts);
    config.settings.populate_screen_layouts(&screen_layouts);
    config.settings.populate_controls(&global_settings.default_control, &global_settings.custom_controls);
    config
}

fn apply_setting(setting: &mut crate::settings::Setting, value: i32) {
    match &mut setting.value {
        SettingValue::Bool(b) => *b = value != 0,
        SettingValue::List(inner) => inner.selection = (value.max(0) as usize).min(inner.values.len().saturating_sub(1)),
        SettingValue::Int(v) => *v = value.max(0) as usize,
    }
}

unsafe fn new_jstring(env: *mut JNIEnv, s: &str) -> jstring {
    let c = CString::new(s).unwrap();
    (**env).NewStringUTF.unwrap()(env, c.as_ptr())
}

#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativeGetGameSettings(env: *mut JNIEnv, _class: jobject, rom_path: jstring) -> jstring {
    let path = PathBuf::from(unsafe { jstring_to_string(env, rom_path) });
    let mut config = build_game_config(&path);
    unsafe { new_jstring(env, &serialize_settings(&mut config.settings)) }
}

#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativeSetGameSetting(env: *mut JNIEnv, _class: jobject, rom_path: jstring, idx: jint, value: jint) {
    let path = PathBuf::from(unsafe { jstring_to_string(env, rom_path) });
    let mut config = build_game_config(&path);
    if let Some(setting) = config.settings.get_all_mut().get_mut(idx as usize) {
        apply_setting(setting, value);
        config.dirty = true;
        config.flush();
    }
}

// Runtime (paused-game) settings: the pause snapshot is published when present_pause
// blocks; edits queue up and are applied on the emu thread when the dialog resolves.
static PAUSE_SETTINGS_JSON: Mutex<String> = Mutex::new(String::new());
static PENDING_RUNTIME_SETTINGS: Mutex<Vec<(usize, i32)>> = Mutex::new(Vec::new());

#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativeGetRuntimeSettings(env: *mut JNIEnv, _class: jobject) -> jstring {
    let json = PAUSE_SETTINGS_JSON.lock().unwrap().clone();
    unsafe { new_jstring(env, &json) }
}

#[no_mangle]
pub extern "system" fn Java_com_grarak_dsvita_DSVitaActivity_nativeSetRuntimeSetting(_env: *mut JNIEnv, _class: jobject, idx: jint, value: jint) {
    PENDING_RUNTIME_SETTINGS.lock().unwrap().push((idx as usize, value));
}

// -------------------------------------------------------------------------------- EGL

type EGLDisplay = *mut c_void;
type EGLConfig = *mut c_void;
type EGLSurface = *mut c_void;
type EGLContext = *mut c_void;
type EGLint = i32;

const EGL_NONE: EGLint = 0x3038;
const EGL_RED_SIZE: EGLint = 0x3024;
const EGL_GREEN_SIZE: EGLint = 0x3023;
const EGL_BLUE_SIZE: EGLint = 0x3022;
const EGL_ALPHA_SIZE: EGLint = 0x3021;
const EGL_RENDERABLE_TYPE: EGLint = 0x3040;
const EGL_OPENGL_ES3_BIT: EGLint = 0x40;
const EGL_SURFACE_TYPE: EGLint = 0x3033;
const EGL_WINDOW_BIT: EGLint = 0x4;
const EGL_CONTEXT_CLIENT_VERSION: EGLint = 0x3098;
const EGL_WIDTH: EGLint = 0x3057;
const EGL_HEIGHT: EGLint = 0x3056;

#[link(name = "EGL")]
extern "C" {
    fn eglGetDisplay(display_id: *mut c_void) -> EGLDisplay;
    fn eglInitialize(dpy: EGLDisplay, major: *mut EGLint, minor: *mut EGLint) -> u32;
    fn eglChooseConfig(dpy: EGLDisplay, attribs: *const EGLint, configs: *mut EGLConfig, config_size: EGLint, num_config: *mut EGLint) -> u32;
    fn eglCreateWindowSurface(dpy: EGLDisplay, config: EGLConfig, win: *mut c_void, attribs: *const EGLint) -> EGLSurface;
    fn eglCreateContext(dpy: EGLDisplay, config: EGLConfig, share: EGLContext, attribs: *const EGLint) -> EGLContext;
    fn eglMakeCurrent(dpy: EGLDisplay, draw: EGLSurface, read: EGLSurface, ctx: EGLContext) -> u32;
    fn eglSwapBuffers(dpy: EGLDisplay, surface: EGLSurface) -> u32;
    fn eglSwapInterval(dpy: EGLDisplay, interval: EGLint) -> u32;
    fn eglQuerySurface(dpy: EGLDisplay, surface: EGLSurface, attribute: EGLint, value: *mut EGLint) -> u32;
    fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
    fn eglGetError() -> EGLint;
}

struct Egl {
    display: EGLDisplay,
    surface: EGLSurface,
    #[allow(dead_code)]
    context: EGLContext,
    surface_size: (i32, i32),
}

impl Egl {
    fn new(window: *mut c_void) -> Self {
        unsafe {
            let display = eglGetDisplay(ptr::null_mut());
            assert!(!display.is_null(), "eglGetDisplay failed: {:x}", eglGetError());
            assert_ne!(eglInitialize(display, ptr::null_mut(), ptr::null_mut()), 0, "eglInitialize failed: {:x}", eglGetError());

            #[rustfmt::skip]
            let config_attribs = [
                EGL_RENDERABLE_TYPE, EGL_OPENGL_ES3_BIT,
                EGL_SURFACE_TYPE, EGL_WINDOW_BIT,
                EGL_RED_SIZE, 8,
                EGL_GREEN_SIZE, 8,
                EGL_BLUE_SIZE, 8,
                EGL_ALPHA_SIZE, 8,
                EGL_NONE,
            ];
            let mut config: EGLConfig = ptr::null_mut();
            let mut num_configs: EGLint = 0;
            assert_ne!(eglChooseConfig(display, config_attribs.as_ptr(), &mut config, 1, &mut num_configs), 0);
            assert!(num_configs > 0, "no ES3 window EGLConfig");

            let surface = eglCreateWindowSurface(display, config, window, ptr::null());
            assert!(!surface.is_null(), "eglCreateWindowSurface failed: {:x}", eglGetError());

            let context_attribs = [EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE];
            let context = eglCreateContext(display, config, ptr::null_mut(), context_attribs.as_ptr());
            assert!(!context.is_null(), "eglCreateContext failed: {:x}", eglGetError());
            assert_ne!(eglMakeCurrent(display, surface, surface, context), 0, "eglMakeCurrent failed: {:x}", eglGetError());
            eglSwapInterval(display, 1);

            let (mut width, mut height) = (0, 0);
            eglQuerySurface(display, surface, EGL_WIDTH, &mut width);
            eglQuerySurface(display, surface, EGL_HEIGHT, &mut height);
            println!("EGL surface {width}x{height}");

            Egl {
                display,
                surface,
                context,
                surface_size: (width, height),
            }
        }
    }
}

// ------------------------------------------------------------------------------ audio

type AAudioStreamBuilder = c_void;
type AAudioStream = c_void;

const AAUDIO_FORMAT_PCM_I16: i32 = 1;
const AAUDIO_OK: i32 = 0;

#[link(name = "aaudio")]
extern "C" {
    fn AAudio_createStreamBuilder(builder: *mut *mut AAudioStreamBuilder) -> i32;
    fn AAudioStreamBuilder_setSampleRate(builder: *mut AAudioStreamBuilder, sample_rate: i32);
    fn AAudioStreamBuilder_setChannelCount(builder: *mut AAudioStreamBuilder, count: i32);
    fn AAudioStreamBuilder_setFormat(builder: *mut AAudioStreamBuilder, format: i32);
    fn AAudioStreamBuilder_setBufferCapacityInFrames(builder: *mut AAudioStreamBuilder, frames: i32);
    fn AAudioStreamBuilder_openStream(builder: *mut AAudioStreamBuilder, stream: *mut *mut AAudioStream) -> i32;
    fn AAudioStreamBuilder_delete(builder: *mut AAudioStreamBuilder) -> i32;
    fn AAudioStream_requestStart(stream: *mut AAudioStream) -> i32;
    fn AAudioStream_write(stream: *mut AAudioStream, buffer: *const c_void, num_frames: i32, timeout_nanos: i64) -> i32;
}

#[derive(Clone)]
pub struct PresenterAudioOut {
    stream: *mut AAudioStream,
}

unsafe impl Send for PresenterAudioOut {}

impl PresenterAudioOut {
    fn new() -> Self {
        unsafe {
            let mut builder: *mut AAudioStreamBuilder = ptr::null_mut();
            if AAudio_createStreamBuilder(&mut builder) != AAUDIO_OK {
                return PresenterAudioOut { stream: ptr::null_mut() };
            }
            AAudioStreamBuilder_setSampleRate(builder, PRESENTER_AUDIO_OUT_SAMPLE_RATE as i32);
            AAudioStreamBuilder_setChannelCount(builder, 2);
            AAudioStreamBuilder_setFormat(builder, AAUDIO_FORMAT_PCM_I16);
            // Two host buffers of headroom: the blocking write below is the back-pressure
            // that paces the cpu thread (see spu.rs) — keep it tight.
            AAudioStreamBuilder_setBufferCapacityInFrames(builder, PRESENTER_AUDIO_OUT_BUF_SIZE as i32 * 2);
            let mut stream: *mut AAudioStream = ptr::null_mut();
            let ok = AAudioStreamBuilder_openStream(builder, &mut stream) == AAUDIO_OK && !stream.is_null();
            AAudioStreamBuilder_delete(builder);
            if !ok || AAudioStream_requestStart(stream) != AAUDIO_OK {
                eprintln!("AAudio stream unavailable, running silent");
                return PresenterAudioOut { stream: ptr::null_mut() };
            }
            PresenterAudioOut { stream }
        }
    }

    pub fn play(&self, buffer: &[u32; PRESENTER_AUDIO_OUT_BUF_SIZE]) {
        if !self.stream.is_null() {
            // Blocking write — returns once all frames are queued, which is exactly the
            // back-pressure contract the SDL busy-wait provides on Linux.
            unsafe { AAudioStream_write(self.stream, buffer.as_ptr() as _, PRESENTER_AUDIO_OUT_BUF_SIZE as i32, i64::MAX) };
        }
    }
}

pub struct PresenterAudioIn;

unsafe impl Send for PresenterAudioIn {}

impl PresenterAudioIn {
    pub fn receive(&self, _: &mut [i16; PRESENTER_AUDIO_IN_BUF_SIZE]) {}
}

// -------------------------------------------------------------------------- presenter

pub struct Presenter {
    presenter_audio_out: PresenterAudioOut,
    egl: Egl,
    keymap: u32,
}

impl Presenter {
    #[cold]
    pub fn new() -> Option<Self> {
        // Wait for the Activity to deliver the first surface.
        let window = {
            let mut guard = SURFACE.lock().unwrap();
            while *guard == 0 {
                guard = SURFACE_COND.wait(guard).unwrap();
            }
            *guard as *mut c_void
        };

        let egl = Egl::new(window);
        gl::load_with(|name| {
            let name = CString::new(name).unwrap();
            unsafe { eglGetProcAddress(name.as_ptr()) as *const _ }
        });

        Some(Presenter {
            presenter_audio_out: PresenterAudioOut::new(),
            egl,
            keymap: 0xFFFFFFFF,
        })
    }

    /// Blocks until the Activity's rom browser calls nativeLaunch, then assembles the
    /// launch tuple exactly like the ImGui browser does on the other platforms (same
    /// saves/, settings/, global_settings/ layout under the storage dir).
    pub fn present_ui(&mut self, screen_layouts: &mut ScreenLayouts, ra_context: &mut RaContext, default_keybinding: KeyBinding) -> Option<(CartridgeIo, GlobalSettings, Settings, PathBuf)> {
        let storage = storage_dir();
        let saves_path = storage.join("saves");
        let settings_path = storage.join("settings");
        let overlays_path = storage.join("overlays");
        for dir in [&saves_path, &settings_path, &overlays_path] {
            std::fs::create_dir_all(dir).ok();
        }

        let rom_path = {
            let mut guard = LAUNCH_REQUEST.lock().unwrap();
            loop {
                if let Some(path) = guard.take() {
                    break path;
                }
                guard = LAUNCH_COND.wait(guard).unwrap();
            }
        };
        println!("launching {}", rom_path.display());

        let preview = match CartridgePreview::new(rom_path.clone()) {
            Ok(preview) => preview,
            Err(err) => {
                eprintln!("failed to open rom {}: {err}", rom_path.display());
                return None;
            }
        };

        let mut global_settings = GlobalSettings::new(storage.join("global_settings"), default_keybinding).unwrap();
        screen_layouts.populate_custom_layouts(&global_settings.custom_layouts);
        screen_layouts.set_overlays_dir(overlays_path);
        ra_context.set_cache_dir(storage.join("ra"));

        let save_file = saves_path.join(format!("{}.sav", preview.file_name));
        let mut config = SettingsConfig::new(settings_path.join(format!("{}.ini", preview.file_name)));
        config.settings.populate_screen_layouts(screen_layouts);
        config.settings.populate_controls(&global_settings.default_control, &global_settings.custom_controls);

        match CartridgeIo::from_preview(preview, save_file) {
            Ok(cartridge_io) => Some((cartridge_io, global_settings, config.settings, config.settings_file_path)),
            Err(err) => {
                eprintln!("failed to load rom {}: {err}", rom_path.display());
                None
            }
        }
    }

    pub fn destroy_ui(&self) {}

    pub fn on_game_launched(&self) {}

    /// The Activity shows the pause dialog; block until it reports the choice. While
    /// blocked, the live settings are published for the runtime-settings screen and any
    /// queued edits are applied (and persisted) on this thread before returning.
    pub fn present_pause(&mut self, _gpu_renderer: &GpuRenderer, settings: &mut Settings, settings_file_path: &Path, _rom_path: &Path) -> UiPauseMenuReturn {
        *PAUSE_SETTINGS_JSON.lock().unwrap() = serialize_settings(settings);
        PENDING_RUNTIME_SETTINGS.lock().unwrap().clear();

        let choice = {
            let mut guard = PAUSE_CHOICE.lock().unwrap();
            loop {
                if let Some(choice) = guard.take() {
                    break choice;
                }
                guard = PAUSE_COND.wait(guard).unwrap();
            }
        };

        let pending = std::mem::take(&mut *PENDING_RUNTIME_SETTINGS.lock().unwrap());
        if !pending.is_empty() {
            for (idx, value) in pending {
                if let Some(setting) = settings.get_all_mut().get_mut(idx) {
                    if setting.runtime {
                        apply_setting(setting, value);
                    }
                }
            }
            if !settings_file_path.as_os_str().is_empty() {
                let mut config = SettingsConfig::from(settings.clone());
                config.settings_file_path = settings_file_path.to_path_buf();
                config.dirty = true;
                config.flush();
            }
        }
        choice
    }

    pub fn present_progress(&mut self, current_name: impl AsRef<str>, progress: usize, total: usize) {
        println!("progress: {} {progress}/{total}", current_name.as_ref());
    }

    pub fn present_savestate_progress(&mut self, _gpu_renderer: &GpuRenderer, text: impl AsRef<str>, progress: usize) {
        println!("savestate: {} {progress}", text.as_ref());
    }

    pub fn get_default_key_mapping() -> [u32; crate::key_bindings::NUM_KEYS] {
        [0; crate::key_bindings::NUM_KEYS]
    }

    pub fn get_default_hotkey_mapping() -> [u32; crate::key_bindings::NUM_HOTKEYS] {
        [0; crate::key_bindings::NUM_HOTKEYS]
    }

    pub fn set_key_mapping(&mut self, _: &KeyBinding) {}

    pub fn get_savestate_path(&self) -> Option<PathBuf> {
        None
    }

    pub fn poll_event(&mut self, _: &Settings) -> PresentEvent {
        if PAUSE_REQUESTED.swap(false, Ordering::Relaxed) {
            return PresentEvent::Pause;
        }

        self.keymap = !DS_KEYS_HELD.load(Ordering::Relaxed);

        // Touch arrives in surface pixels; map through the letterbox rect into the
        // 960x544 logical space the shared normalize path expects.
        let touch = match TOUCH.load(Ordering::Relaxed) {
            u32::MAX => None,
            packed => {
                let ((ox, _, w, h), (_, sh)) = self.present_rect();
                let oy_top = (sh - h) / 2;
                let x = ((packed >> 16) as i32 - ox) * PRESENTER_SCREEN_WIDTH as i32 / w.max(1);
                let y = ((packed & 0xFFFF) as i32 - oy_top) * PRESENTER_SCREEN_HEIGHT as i32 / h.max(1);
                Some((x.clamp(0, PRESENTER_SCREEN_WIDTH as i32 - 1) as i16, y.clamp(0, PRESENTER_SCREEN_HEIGHT as i32 - 1) as i16))
            }
        };

        PresentEvent::Inputs {
            keymap: self.keymap,
            touch,
            #[cfg(debug_assertions)]
            debug_touch: None,
        }
    }

    /// Aspect-fit letterbox of the 960x544 frame into the surface (GL bottom-left rect).
    pub fn present_rect(&self) -> ((i32, i32, i32, i32), (i32, i32)) {
        let (sw, sh) = self.egl.surface_size;
        let w = PRESENTER_SCREEN_WIDTH as i64;
        let h = PRESENTER_SCREEN_HEIGHT as i64;
        let (dw, dh) = if (sw as i64) * h <= (sh as i64) * w {
            (sw, ((sw as i64) * h / w) as i32)
        } else {
            (((sh as i64) * w / h) as i32, sh)
        };
        (((sw - dw) / 2, (sh - dh) / 2, dw, dh), (sw, sh))
    }

    pub fn gl_swap_window(&self) {
        unsafe { eglSwapBuffers(self.egl.display, self.egl.surface) };
    }

    pub fn get_presenter_audio_out(&self) -> PresenterAudioOut {
        self.presenter_audio_out.clone()
    }

    pub fn get_presenter_audio_in(&self) -> PresenterAudioIn {
        PresenterAudioIn
    }

    pub fn wait_vsync(&self) {}

    // Only reachable through the vita arms of runtime cfg! branches; present so those
    // branches typecheck (same as the Linux presenter).
    pub fn gl_create_depth_tex() -> gl::types::GLuint {
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

pub fn default_key_binding() -> KeyBinding {
    KeyBinding::new("Default".to_string(), Presenter::get_default_key_mapping(), Presenter::get_default_hotkey_mapping())
}
