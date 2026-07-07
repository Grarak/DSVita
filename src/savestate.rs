use crate::utils::{HeapArray, HeapMem};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;
use std::{ptr, slice};

// Quick-save trigger (F11 / SIGUSR1): a bare atomic so the signal handler stays signal-safe
static SAVE_REQUEST: AtomicBool = AtomicBool::new(false);

// UI-driven requests carry payloads and come from the main thread, never a signal handler
pub enum SavestateRequest {
    Save { screenshot: Vec<u8> },
    Load { data: Vec<u8> },
}

static REQUEST: Mutex<Option<SavestateRequest>> = Mutex::new(None);

pub fn request_save() {
    SAVE_REQUEST.store(true, Ordering::Relaxed);
}

pub fn request_save_with_screenshot(screenshot: Vec<u8>) {
    *REQUEST.lock().unwrap() = Some(SavestateRequest::Save { screenshot });
}

pub fn request_load(data: Vec<u8>) {
    *REQUEST.lock().unwrap() = Some(SavestateRequest::Load { data });
}

// Consumed once per frame at the vblank hook on the cpu thread. UI requests
// first: they drive the pause-menu progress dialog whose wait loop wakes the cpu
// for exactly one frame, so a pending quick-save must not shadow them.
pub fn take_request() -> Option<SavestateRequest> {
    if let Some(request) = REQUEST.lock().unwrap().take() {
        return Some(request);
    }
    if SAVE_REQUEST.swap(false, Ordering::Relaxed) {
        return Some(SavestateRequest::Save { screenshot: Vec::new() });
    }
    None
}

// Cross-thread status for the pause-menu progress dialog: the ui thread arms it
// (op_begin) before queuing a request, the cpu thread reports while performing
// the save/load, the main thread polls until a terminal state and clears it.
// Quick-saves (F11/SIGUSR1) never arm it, so their reports are dropped and no
// dialog is shown for them.
const OP_IDLE: u8 = 0;
const OP_WORKING: u8 = 1;
const OP_DONE_SAVE: u8 = 2;
const OP_DONE_LOAD: u8 = 3;
const OP_FAILED: u8 = 4;

static OP_STATE: AtomicU8 = AtomicU8::new(OP_IDLE);
static OP_PHASE: AtomicU8 = AtomicU8::new(0);
static OP_PROGRESS: AtomicU8 = AtomicU8::new(0);
static OP_BYTES: AtomicU32 = AtomicU32::new(0);

#[derive(Copy, Clone)]
pub enum OpPhase {
    Serialize = 0,
    Compress = 1,
    Write = 2,
    Decompress = 3,
    Apply = 4,
}

impl OpPhase {
    pub fn label(self) -> &'static str {
        match self {
            OpPhase::Serialize => "Serializing state",
            OpPhase::Compress => "Compressing",
            OpPhase::Write => "Writing file",
            OpPhase::Decompress => "Decompressing",
            OpPhase::Apply => "Applying state",
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => OpPhase::Serialize,
            1 => OpPhase::Compress,
            2 => OpPhase::Write,
            3 => OpPhase::Decompress,
            _ => OpPhase::Apply,
        }
    }
}

pub enum OpView {
    Working { phase: OpPhase, progress: u8 },
    DoneSave { bytes: u32 },
    DoneLoad { bytes: u32 },
    Failed,
}

pub fn op_begin() {
    OP_PHASE.store(OpPhase::Serialize as u8, Ordering::Relaxed);
    OP_PROGRESS.store(0, Ordering::Relaxed);
    OP_STATE.store(OP_WORKING, Ordering::Release);
}

pub fn op_active() -> bool {
    OP_STATE.load(Ordering::Acquire) != OP_IDLE
}

pub fn op_clear() {
    OP_STATE.store(OP_IDLE, Ordering::Release);
}

// Reported from the cpu thread; dropped unless the dialog armed the op
pub fn op_report(phase: OpPhase, progress: u8) {
    if OP_STATE.load(Ordering::Relaxed) == OP_WORKING {
        OP_PHASE.store(phase as u8, Ordering::Relaxed);
        OP_PROGRESS.store(progress, Ordering::Relaxed);
    }
}

pub fn op_finish_save(bytes: usize) {
    if OP_STATE.load(Ordering::Relaxed) == OP_WORKING {
        OP_BYTES.store(bytes as u32, Ordering::Relaxed);
        OP_STATE.store(OP_DONE_SAVE, Ordering::Release);
    }
}

pub fn op_finish_load(bytes: usize) {
    if OP_STATE.load(Ordering::Relaxed) == OP_WORKING {
        OP_BYTES.store(bytes as u32, Ordering::Relaxed);
        OP_STATE.store(OP_DONE_LOAD, Ordering::Release);
    }
}

pub fn op_fail() {
    if OP_STATE.load(Ordering::Relaxed) == OP_WORKING {
        OP_STATE.store(OP_FAILED, Ordering::Release);
    }
}

pub fn op_poll() -> OpView {
    match OP_STATE.load(Ordering::Acquire) {
        OP_DONE_SAVE => OpView::DoneSave {
            bytes: OP_BYTES.load(Ordering::Relaxed),
        },
        OP_DONE_LOAD => OpView::DoneLoad {
            bytes: OP_BYTES.load(Ordering::Relaxed),
        },
        OP_FAILED => OpView::Failed,
        _ => OpView::Working {
            phase: OpPhase::from_u8(OP_PHASE.load(Ordering::Relaxed)),
            progress: OP_PROGRESS.load(Ordering::Relaxed),
        },
    }
}

pub use dsvita_macros::Savestate;

pub const SAVESTATE_MAGIC: u32 = u32::from_le_bytes(*b"DSVS");
pub const SAVESTATE_VERSION: u16 = 2;

// File layout: magic u32 | version u16 | arm7_emu u8 | screenshot_len u32 | screenshot |
// deflate-compressed state. The header stays uncompressed so the list UI can read the
// screenshot (jpeg) without touching the multi-MB payload; arm7_emu gates loading to the
// matching emulation mode. Compression is miniz_oxide, which the tree already carries
// transitively through png.
const HEADER_FIXED_LEN: usize = 11;
const STATE_DECOMPRESS_LIMIT: usize = 64 << 20;

pub struct SavestateMeta {
    pub arm7_emu: u8,
    pub screenshot: Vec<u8>,
}

fn parse_fixed_header(fixed: &[u8; HEADER_FIXED_LEN]) -> Option<(u8, usize)> {
    if u32::from_le_bytes(fixed[0..4].try_into().unwrap()) != SAVESTATE_MAGIC || u16::from_le_bytes(fixed[4..6].try_into().unwrap()) != SAVESTATE_VERSION {
        return None;
    }
    let screenshot_len = u32::from_le_bytes(fixed[7..11].try_into().unwrap());
    if screenshot_len > 16 << 20 {
        return None;
    }
    Some((fixed[6], screenshot_len as usize))
}

// Header-only read for the savestate list UI
pub fn peek_meta(path: &Path) -> Option<SavestateMeta> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut fixed = [0u8; HEADER_FIXED_LEN];
    file.read_exact(&mut fixed).ok()?;
    let (arm7_emu, screenshot_len) = parse_fixed_header(&fixed)?;
    let mut screenshot = vec![0; screenshot_len];
    file.read_exact(&mut screenshot).ok()?;
    Some(SavestateMeta { arm7_emu, screenshot })
}

pub fn encode_savestate_file(arm7_emu: u8, screenshot: &[u8], state: &[u8], mut on_progress: impl FnMut(usize, usize)) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_FIXED_LEN + screenshot.len() + state.len() / 4);
    out.extend_from_slice(&SAVESTATE_MAGIC.to_le_bytes());
    out.extend_from_slice(&SAVESTATE_VERSION.to_le_bytes());
    out.push(arm7_emu);
    out.extend_from_slice(&(screenshot.len() as u32).to_le_bytes());
    out.extend_from_slice(screenshot);

    // Level 1: the payload is dominated by sparse guest ram, which even the fastest
    // level shrinks massively, and saves happen on the vita's cpu. Streamed in
    // chunks (same flags compress_to_vec(_, 1) uses, so the format is unchanged)
    // because compression dominates the save time and feeds the progress dialog.
    use miniz_oxide::deflate::core::{compress, create_comp_flags_from_zip_params, CompressorOxide, TDEFLFlush, TDEFLStatus};
    const CHUNK: usize = 512 << 10;
    let mut compressor = CompressorOxide::new(create_comp_flags_from_zip_params(1, 0, 0));
    let header_len = out.len();
    out.resize(header_len + (state.len() / 2).max(64), 0);
    let mut in_pos = 0;
    let mut out_pos = header_len;
    loop {
        let in_end = (in_pos + CHUNK).min(state.len());
        let flush = if in_end == state.len() { TDEFLFlush::Finish } else { TDEFLFlush::None };
        let (status, bytes_in, bytes_out) = compress(&mut compressor, &state[in_pos..in_end], &mut out[out_pos..], flush);
        in_pos += bytes_in;
        out_pos += bytes_out;
        on_progress(in_pos, state.len());
        match status {
            TDEFLStatus::Done => break,
            TDEFLStatus::Okay => {
                if out.len() - out_pos < 64 {
                    let grow = out.len();
                    out.resize(grow * 2, 0);
                }
            }
            // Can't happen with valid flags on a fresh compressor
            _ => {
                debug_assert!(false, "savestate deflate failed: {status:?}");
                break;
            }
        }
    }
    out.truncate(out_pos);
    out
}

// The decompressed state stream, or None on corrupt input, version mismatch or a
// state taken under a different arm7 emulation mode
pub fn decode_savestate_file(data: &[u8], expected_arm7_emu: u8) -> Option<Vec<u8>> {
    if data.len() < HEADER_FIXED_LEN {
        return None;
    }
    let (arm7_emu, screenshot_len) = parse_fixed_header(data[0..HEADER_FIXED_LEN].try_into().unwrap())?;
    if arm7_emu != expected_arm7_emu {
        return None;
    }
    let state_offset = HEADER_FIXED_LEN.checked_add(screenshot_len)?;
    let compressed = data.get(state_offset..)?;
    miniz_oxide::inflate::decompress_to_vec_with_limit(compressed, STATE_DECOMPRESS_LIMIT).ok()
}

// Single context drives both directions so save and load can never drift apart:
// the same #[derive(Savestate)] field walk either appends to or consumes from buf.
// Load never panics on corrupt input; it sets the error flag, subsequent ops no-op
// and the caller checks is_load_successful before using the emu state.
pub struct SavestateContext {
    buf: Vec<u8>,
    pos: usize,
    save: bool,
    error: bool,
}

impl SavestateContext {
    pub fn new_save() -> Self {
        SavestateContext {
            // Main RAM + VRAM + WRAM dominate; avoid Vec growth copies of multi-MB buffers
            buf: Vec::with_capacity(8 << 20),
            pos: 0,
            save: true,
            error: false,
        }
    }

    pub fn new_load(data: Vec<u8>) -> Self {
        SavestateContext {
            buf: data,
            pos: 0,
            save: false,
            error: false,
        }
    }

    pub fn is_save(&self) -> bool {
        self.save
    }

    pub fn has_error(&self) -> bool {
        self.error
    }

    pub fn set_error(&mut self) {
        self.error = true;
    }

    pub fn into_data(self) -> Option<Vec<u8>> {
        debug_assert!(self.save);
        (!self.error).then_some(self.buf)
    }

    // Load must consume the file exactly; leftover bytes mean layout drift
    pub fn is_load_successful(&self) -> bool {
        debug_assert!(!self.save);
        !self.error && self.pos == self.buf.len()
    }

    fn load_remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn put_u8(&mut self, value: u8) {
        debug_assert!(self.save);
        if self.error {
            return;
        }
        self.buf.push(value);
    }

    pub fn take_u8(&mut self) -> u8 {
        debug_assert!(!self.save);
        if self.error || self.pos >= self.buf.len() {
            self.set_error();
            return 0;
        }
        let value = self.buf[self.pos];
        self.pos += 1;
        value
    }

    // Raw memcpy of any value; T must be plain old data without padding and with no
    // bit patterns that are invalid for its type. The Copy bound rejects the dangerous
    // cases (Vec, Box, HeapMem, ...) where copying bytes would clone/clobber a pointer.
    pub fn bytes_of<T: Copy>(&mut self, value: &mut T) {
        self.bytes_of_raw(value);
    }

    // Same as bytes_of without the Copy guard, for type-level #[savestate(bytes)] on
    // non-Copy pod types (bilge register structs). Never point this at anything owning heap memory.
    pub fn bytes_of_raw<T>(&mut self, value: &mut T) {
        if self.error {
            return;
        }
        let size = size_of::<T>();
        if self.save {
            self.buf.extend_from_slice(unsafe { slice::from_raw_parts(value as *const T as *const u8, size) });
        } else {
            if self.load_remaining() < size {
                self.set_error();
                return;
            }
            unsafe { ptr::copy_nonoverlapping(self.buf.as_ptr().add(self.pos), value as *mut T as *mut u8, size) };
            self.pos += size;
        }
    }

    // Raw memcpy of a whole slice; same pod requirements as bytes_of
    pub fn pod_slice<T: Copy>(&mut self, values: &mut [T]) {
        if self.error {
            return;
        }
        let size = size_of_val(values);
        if self.save {
            self.buf.extend_from_slice(unsafe { slice::from_raw_parts(values.as_ptr() as *const u8, size) });
        } else {
            if self.load_remaining() < size {
                self.set_error();
                return;
            }
            unsafe { ptr::copy_nonoverlapping(self.buf.as_ptr().add(self.pos), values.as_mut_ptr() as *mut u8, size) };
            self.pos += size;
        }
    }
}

pub trait Savestate {
    fn savestate(&mut self, state: &mut SavestateContext);
}

macro_rules! impl_savestate_pod {
    ($($t:ty),+) => {
        $(
            impl Savestate for $t {
                fn savestate(&mut self, state: &mut SavestateContext) {
                    state.bytes_of(self);
                }
            }
        )+
    };
}

impl_savestate_pod!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, f32, f64);

// Generates a memcpy Savestate impl for pod types that can't carry the derive,
// e.g. bilge register structs defined through #[bitsize]
macro_rules! impl_savestate_bytes {
    ($($t:ty),+) => {
        $(
            impl crate::savestate::Savestate for $t {
                fn savestate(&mut self, state: &mut crate::savestate::SavestateContext) {
                    state.bytes_of_raw(self);
                }
            }
        )+
    };
}
pub(crate) use impl_savestate_bytes;

impl Savestate for bool {
    fn savestate(&mut self, state: &mut SavestateContext) {
        if state.is_save() {
            state.put_u8(*self as u8);
        } else {
            *self = state.take_u8() != 0;
        }
    }
}

// usize/isize are stored as 64-bit so savestates are portable between the
// 32-bit (vita, thumbv7neon) and 64-bit (aarch64) targets
impl Savestate for usize {
    fn savestate(&mut self, state: &mut SavestateContext) {
        let mut value = *self as u64;
        state.bytes_of(&mut value);
        *self = value as usize;
    }
}

impl Savestate for isize {
    fn savestate(&mut self, state: &mut SavestateContext) {
        let mut value = *self as i64;
        state.bytes_of(&mut value);
        *self = value as isize;
    }
}

// Per-element walk; mark large primitive buffers #[savestate(bytes)] instead to get a memcpy
impl<T: Savestate, const N: usize> Savestate for [T; N] {
    fn savestate(&mut self, state: &mut SavestateContext) {
        for value in self {
            value.savestate(state);
        }
    }
}

impl<T: Savestate> Savestate for Box<T> {
    fn savestate(&mut self, state: &mut SavestateContext) {
        (**self).savestate(state);
    }
}

impl<T: Savestate + Default> Savestate for Option<T> {
    fn savestate(&mut self, state: &mut SavestateContext) {
        if state.is_save() {
            match self {
                Some(value) => {
                    state.put_u8(1);
                    value.savestate(state);
                }
                None => state.put_u8(0),
            }
        } else if state.take_u8() != 0 {
            let mut value = T::default();
            value.savestate(state);
            *self = Some(value);
        } else {
            *self = None;
        }
    }
}

impl<T: Savestate + Default> Savestate for Vec<T> {
    fn savestate(&mut self, state: &mut SavestateContext) {
        let mut len = self.len() as u32;
        len.savestate(state);
        if state.is_save() {
            for value in self {
                value.savestate(state);
            }
        } else {
            if state.has_error() {
                return;
            }
            // Corrupt length guard: a valid len can never exceed the file size
            // (only zero-sized-serialization elements could, which don't belong in a Vec)
            if len as usize > state.buf.len() {
                state.set_error();
                return;
            }
            self.clear();
            self.resize_with(len as usize, T::default);
            for value in self {
                value.savestate(state);
            }
        }
    }
}

impl<T: Savestate + Default> Savestate for std::collections::VecDeque<T> {
    fn savestate(&mut self, state: &mut SavestateContext) {
        let mut len = self.len() as u32;
        len.savestate(state);
        if state.is_save() {
            for value in self {
                value.savestate(state);
            }
        } else {
            if state.has_error() {
                return;
            }
            // Same corrupt length guard as Vec
            if len as usize > state.buf.len() {
                state.set_error();
                return;
            }
            self.clear();
            self.resize_with(len as usize, T::default);
            for value in self {
                value.savestate(state);
            }
        }
    }
}

impl<T: Savestate> Savestate for HeapMem<T> {
    fn savestate(&mut self, state: &mut SavestateContext) {
        (**self).savestate(state);
    }
}

// HeapArray only ever holds flat pod buffers, so always take the memcpy path
impl<T: Copy, const SIZE: usize, const ALIGNMENT: usize> Savestate for HeapArray<T, SIZE, ALIGNMENT> {
    fn savestate(&mut self, state: &mut SavestateContext) {
        state.pod_slice(&mut **self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save<T: Savestate>(value: &mut T) -> Vec<u8> {
        let mut state = SavestateContext::new_save();
        value.savestate(&mut state);
        state.into_data().unwrap()
    }

    fn load<T: Savestate>(value: &mut T, data: Vec<u8>) -> bool {
        let mut state = SavestateContext::new_load(data);
        value.savestate(&mut state);
        state.is_load_successful()
    }

    #[derive(Savestate, Debug, PartialEq, Default)]
    struct Plain {
        a: u32,
        b: i16,
        c: bool,
        d: f32,
        arr: [u16; 4],
        us: usize,
    }

    fn plain() -> Plain {
        Plain {
            a: 0xDEADBEEF,
            b: -12345,
            c: true,
            d: 3.5,
            arr: [1, 2, 3, 4],
            us: 0x11223344,
        }
    }

    #[test]
    fn struct_roundtrip() {
        let mut original = plain();
        let data = save(&mut original);
        // 4 + 2 + 1 + 4 + 8 + 8 (usize widened to u64)
        assert_eq!(data.len(), 27);
        let mut loaded = Plain::default();
        assert!(load(&mut loaded, data));
        assert_eq!(loaded, original);
    }

    #[derive(Savestate)]
    struct Skipped {
        kept: u32,
        #[savestate(skip)]
        skipped: u32,
    }

    #[test]
    fn skip_preserves_target() {
        let mut original = Skipped { kept: 7, skipped: 13 };
        let data = save(&mut original);
        assert_eq!(data.len(), 4);
        let mut loaded = Skipped { kept: 0, skipped: 99 };
        assert!(load(&mut loaded, data));
        assert_eq!(loaded.kept, 7);
        assert_eq!(loaded.skipped, 99);
    }

    #[derive(Savestate, PartialEq, Debug)]
    struct BytesField {
        #[savestate(bytes)]
        buf: [u8; 64],
    }

    #[test]
    fn bytes_field_roundtrip() {
        let mut original = BytesField { buf: [0xAB; 64] };
        original.buf[5] = 3;
        let data = save(&mut original);
        assert_eq!(data.len(), 64);
        let mut loaded = BytesField { buf: [0; 64] };
        assert!(load(&mut loaded, data));
        assert_eq!(loaded, original);
    }

    fn complemented(value: &mut u32, state: &mut SavestateContext) {
        let mut stored = !*value;
        state.bytes_of(&mut stored);
        *value = !stored;
    }

    #[derive(Savestate)]
    struct WithField {
        #[savestate(with = "complemented")]
        v: u32,
    }

    #[test]
    fn with_custom_fn() {
        let mut original = WithField { v: 0x01020304 };
        let data = save(&mut original);
        assert_eq!(data, (!0x01020304u32).to_le_bytes());
        let mut loaded = WithField { v: 0 };
        assert!(load(&mut loaded, data));
        assert_eq!(loaded.v, 0x01020304);
    }

    #[derive(Savestate, PartialEq, Debug)]
    struct TupleStruct(u32, u8);

    #[derive(Savestate, PartialEq, Debug)]
    struct UnitStruct;

    #[test]
    fn tuple_and_unit_structs() {
        let mut original = TupleStruct(77, 8);
        let data = save(&mut original);
        assert_eq!(data.len(), 5);
        let mut loaded = TupleStruct(0, 0);
        assert!(load(&mut loaded, data));
        assert_eq!(loaded, original);

        assert!(save(&mut UnitStruct).is_empty());
    }

    #[derive(Savestate, PartialEq, Debug, Default)]
    struct Nested {
        plain: Plain,
        opt: Option<u32>,
        empty_opt: Option<u8>,
        vec: Vec<u16>,
        boxed: Box<u32>,
    }

    #[test]
    fn nested_roundtrip() {
        let mut original = Nested {
            plain: plain(),
            opt: Some(42),
            empty_opt: None,
            vec: vec![5, 6, 7],
            boxed: Box::new(0xCAFE),
        };
        let data = save(&mut original);
        let mut loaded = Nested::default();
        assert!(load(&mut loaded, data));
        assert_eq!(loaded, original);
    }

    #[derive(Savestate, PartialEq, Debug)]
    enum CLike {
        A,
        B,
        C,
    }

    #[derive(Savestate, PartialEq, Debug)]
    enum DataEnum {
        Empty,
        One(u32),
        Two(u16, #[savestate(skip)] u16),
        Named { x: u16, y: bool },
    }

    #[test]
    fn enum_roundtrip() {
        let mut original = CLike::B;
        let mut loaded = CLike::C;
        assert!(load(&mut loaded, save(&mut original)));
        assert_eq!(loaded, CLike::B);

        // variant change on load, including into and out of data variants
        let mut loaded = DataEnum::Empty;
        assert!(load(&mut loaded, save(&mut DataEnum::One(0x1234))));
        assert_eq!(loaded, DataEnum::One(0x1234));

        assert!(load(&mut loaded, save(&mut DataEnum::Named { x: 9, y: true })));
        assert_eq!(loaded, DataEnum::Named { x: 9, y: true });

        // skipped enum fields load as Default
        assert!(load(&mut loaded, save(&mut DataEnum::Two(3, 4))));
        assert_eq!(loaded, DataEnum::Two(3, 0));

        assert!(load(&mut loaded, save(&mut DataEnum::Empty)));
        assert_eq!(loaded, DataEnum::Empty);
    }

    #[test]
    fn enum_bad_tag_errors() {
        let mut loaded = CLike::A;
        assert!(!load(&mut loaded, vec![7]));
        assert_eq!(loaded, CLike::A);
    }

    #[derive(Savestate)]
    #[savestate(bytes)]
    struct WholeBytes {
        a: u16,
        b: u16,
    }

    #[test]
    fn type_level_bytes() {
        let mut original = WholeBytes { a: 0x1122, b: 0x3344 };
        let data = save(&mut original);
        assert_eq!(data.len(), size_of::<WholeBytes>());
        let mut loaded = WholeBytes { a: 0, b: 0 };
        assert!(load(&mut loaded, data));
        assert_eq!(loaded.a, 0x1122);
        assert_eq!(loaded.b, 0x3344);
    }

    #[derive(Savestate, PartialEq, Debug, Default)]
    struct Generic<T> {
        value: T,
    }

    #[test]
    fn generic_struct() {
        let mut original = Generic { value: 0x55AAu16 };
        let mut loaded = Generic::default();
        assert!(load(&mut loaded, save(&mut original)));
        assert_eq!(loaded, original);
    }

    #[test]
    fn heap_containers() {
        let mut original: HeapArray<u16, 8> = HeapArray::default();
        original[3] = 0x7777;
        let data = save(&mut original);
        assert_eq!(data.len(), 16);
        let mut loaded: HeapArray<u16, 8> = HeapArray::default();
        assert!(load(&mut loaded, data));
        assert_eq!(loaded[3], 0x7777);

        let mut original: HeapMem<[u32; 4]> = HeapMem::default();
        original[2] = 0xABCD;
        let mut loaded: HeapMem<[u32; 4]> = HeapMem::default();
        assert!(load(&mut loaded, save(&mut original)));
        assert_eq!(loaded[2], 0xABCD);
    }

    #[test]
    fn truncated_data_errors() {
        let mut original = plain();
        let mut data = save(&mut original);
        data.truncate(10);
        let mut loaded = Plain::default();
        assert!(!load(&mut loaded, data));
    }

    #[test]
    fn trailing_data_errors() {
        let mut original = plain();
        let mut data = save(&mut original);
        data.push(0);
        let mut loaded = Plain::default();
        assert!(!load(&mut loaded, data));
    }

    #[test]
    fn corrupt_vec_len_errors() {
        let mut data = Vec::new();
        data.extend_from_slice(&u32::MAX.to_le_bytes());
        let mut loaded: Vec<u16> = Vec::new();
        assert!(!load(&mut loaded, data));
        assert!(loaded.is_empty());
    }

    #[test]
    fn file_codec_roundtrip() {
        let screenshot = [1u8, 2, 3, 4, 5];
        // Compressible like real state payloads
        let mut state_bytes = vec![0u8; 64 * 1024];
        state_bytes[123] = 45;
        state_bytes[60000] = 99;

        let mut progress_calls = 0;
        let file = encode_savestate_file(2, &screenshot, &state_bytes, |done, total| {
            assert!(done <= total);
            progress_calls += 1;
        });
        assert!(progress_calls > 0);
        assert!(file.len() < state_bytes.len() / 2);

        assert_eq!(decode_savestate_file(&file, 2).unwrap(), state_bytes);

        // Chunked encode must produce the exact one-shot compress_to_vec format
        let reference = miniz_oxide::deflate::compress_to_vec(&state_bytes, 1);
        assert_eq!(&file[11 + screenshot.len()..], &reference[..]);

        // arm7 emulation mode mismatch is rejected
        assert!(decode_savestate_file(&file, 0).is_none());

        let mut bad_magic = file.clone();
        bad_magic[0] ^= 0xFF;
        assert!(decode_savestate_file(&bad_magic, 2).is_none());

        // corrupt compressed payload
        let mut bad_state = file.clone();
        let len = bad_state.len();
        bad_state.truncate(len - 8);
        assert!(decode_savestate_file(&bad_state, 2).is_none());
    }
}
