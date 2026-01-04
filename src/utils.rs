use std::alloc::Layout;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::hash::{BuildHasher, Hasher};
use std::ops::{Deref, DerefMut};
use std::{mem, ptr, slice};

pub const fn align_up(n: usize, align: usize) -> usize {
    (n + align - 1) & !(align - 1)
}

pub trait Convert: Copy + Into<u32> {
    fn from(value: u32) -> Self;
}

impl Convert for u8 {
    fn from(value: u32) -> Self {
        value as u8
    }
}

impl Convert for u16 {
    fn from(value: u32) -> Self {
        value as u16
    }
}

impl Convert for u32 {
    fn from(value: u32) -> Self {
        value
    }
}

#[inline(always)]
pub fn read_from_mem<T: Clone>(mem: &[u8], addr: u32) -> T {
    debug_assert!(addr as usize <= mem.len() - size_of::<T>());
    unsafe { (mem.as_ptr().add(addr as usize) as *const T).read() }
}

#[inline(always)]
pub fn read_from_mem_slice<T: Copy>(mem: &[u8], addr: u32, slice: &mut [T]) {
    debug_assert!(addr as usize <= mem.len() - size_of_val(slice));
    unsafe { (mem.as_ptr().add(addr as usize) as *const T).copy_to(slice.as_mut_ptr(), slice.len()) };
}

#[inline(always)]
pub fn write_to_mem<T>(mem: &mut [u8], addr: u32, value: T) {
    debug_assert!(addr as usize <= mem.len() - size_of::<T>());
    unsafe { (mem.as_mut_ptr().add(addr as usize) as *mut T).write(value) }
}

#[inline(always)]
pub fn write_to_mem_slice<T: Copy>(mem: &mut [u8], addr: usize, slice: &[T]) {
    debug_assert!(addr <= mem.len() - size_of_val(slice));
    unsafe { (mem.as_mut_ptr().add(addr) as *mut T).copy_from(slice.as_ptr(), slice.len()) };
}

#[inline(always)]
pub fn write_memset<T: Copy>(mem: &mut [u8], addr: usize, value: T, size: usize) {
    unsafe { slice::from_raw_parts_mut(mem.as_mut_ptr().add(addr) as *mut T, size) }.fill(value)
}

pub struct StrErr {
    str: String,
}

impl StrErr {
    pub fn new(str: impl Into<String>) -> Self {
        StrErr { str: str.into() }
    }
}

impl Debug for StrErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.str, f)
    }
}

impl Display for StrErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.str, f)
    }
}

impl Error for StrErr {}

pub type HeapArrayU8<const SIZE: usize> = HeapArray<u8, SIZE>;
pub type HeapArrayU16<const SIZE: usize> = HeapArray<u16, SIZE>;
pub type HeapArrayI16<const SIZE: usize> = HeapArray<i16, SIZE>;
pub type HeapArrayU32<const SIZE: usize> = HeapArray<u32, SIZE>;
pub type HeapArrayUsize<const SIZE: usize> = HeapArray<usize, SIZE>;

pub struct HeapArray<T, const SIZE: usize>(*mut T);

impl<T, const SIZE: usize> HeapArray<T, SIZE> {
    unsafe fn uninitialized() -> Self {
        HeapArray(std::alloc::alloc(Layout::array::<T>(SIZE).unwrap_unchecked()) as *mut T)
    }

    pub unsafe fn zeroed() -> Self
    where
        [(); size_of::<T>() * SIZE]:,
    {
        let instance = Self::uninitialized();
        let buf: &mut [u8; size_of::<T>() * SIZE] = mem::transmute(instance.0);
        buf.fill(0);
        instance
    }
}

impl<T: Default, const SIZE: usize> Default for HeapArray<T, SIZE> {
    fn default() -> Self {
        unsafe {
            let mut instance = Self::uninitialized();
            instance.fill_with(|| T::default());
            instance
        }
    }
}

impl<T, const SIZE: usize> Drop for HeapArray<T, SIZE> {
    fn drop(&mut self) {
        unsafe { std::alloc::dealloc(self.0 as _, Layout::array::<T>(SIZE).unwrap_unchecked()) };
    }
}

impl<T, const SIZE: usize> Deref for HeapArray<T, SIZE> {
    type Target = [T; SIZE];

    fn deref(&self) -> &Self::Target {
        unsafe { mem::transmute(self.0) }
    }
}

impl<T, const SIZE: usize> DerefMut for HeapArray<T, SIZE> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { mem::transmute(self.0) }
    }
}

impl<T: Copy, const SIZE: usize> Clone for HeapArray<T, SIZE> {
    fn clone(&self) -> Self {
        let mut instance = unsafe { Self::uninitialized() };
        instance.copy_from_slice(self.deref());
        instance
    }
}

impl<T: Debug, const SIZE: usize> Debug for HeapArray<T, SIZE> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_map();
        for (i, v) in self.deref().iter().enumerate() {
            list.entry(&i, v);
        }
        list.finish()
    }
}

unsafe impl<T: Sync, const SIZE: usize> Sync for HeapArray<T, SIZE> {}
unsafe impl<T: Send, const SIZE: usize> Send for HeapArray<T, SIZE> {}

pub struct HeapMem<T>(*mut T);

impl<T> HeapMem<T> {
    unsafe fn uninitialized() -> Self {
        HeapMem(std::alloc::alloc(Layout::new::<T>()) as *mut T)
    }

    pub unsafe fn zeroed() -> Self {
        let instance = Self::uninitialized();
        instance.0.write_bytes(0, size_of::<T>());
        instance
    }
}

impl<T: Default> Default for HeapMem<T> {
    fn default() -> Self {
        unsafe {
            let instance = Self::uninitialized();
            instance.0.write(T::default());
            instance
        }
    }
}

impl<T> Drop for HeapMem<T> {
    fn drop(&mut self) {
        unsafe { std::alloc::dealloc(self.0 as _, Layout::new::<T>()) };
    }
}

impl<T> Deref for HeapMem<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_ref_unchecked() }
    }
}

impl<T> DerefMut for HeapMem<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.as_mut_unchecked() }
    }
}

pub const fn crc16(mut crc: u32, buf: &[u8], start: usize, size: usize) -> u16 {
    const TABLE: [u16; 8] = [0xC0C1, 0xC181, 0xC301, 0xC601, 0xCC01, 0xD801, 0xF001, 0xA001];

    let mut i = start;
    while i < start + size {
        crc ^= buf[i] as u32;
        let mut j = 0;
        while j < TABLE.len() {
            crc = (crc >> 1) ^ if crc & 1 != 0 { (TABLE[j] as u32) << (7 - j) } else { 0 };
            j += 1;
        }
        i += 1;
    }
    crc as u16
}

pub struct NoHasher {
    state: u32,
}

impl Hasher for NoHasher {
    fn finish(&self) -> u64 {
        self.state as u64
    }

    fn write(&mut self, _: &[u8]) {
        unreachable!()
    }

    fn write_u16(&mut self, i: u16) {
        self.state = i as u32;
    }

    fn write_u32(&mut self, i: u32) {
        self.state = i;
    }

    fn write_usize(&mut self, i: usize) {
        self.state = i as u32;
    }

    fn write_i32(&mut self, i: i32) {
        self.state = i as u32;
    }
}

#[derive(Clone, Default)]
pub struct BuildNoHasher;

impl BuildHasher for BuildNoHasher {
    type Hasher = NoHasher;
    fn build_hasher(&self) -> NoHasher {
        NoHasher { state: 0 }
    }
}

pub type NoHashMap<T, V> = HashMap<T, V, BuildNoHasher>;
pub type NoHashSet<T> = HashSet<T, BuildNoHasher>;

pub enum ThreadPriority {
    Low,
    Default,
    High,
}

#[derive(Copy, Clone)]
#[repr(u8)]
pub enum ThreadAffinity {
    Core0 = 0,
    Core1 = 1,
    Core2 = 2,
    Core3 = 3,
}

#[cfg(target_os = "linux")]
pub fn set_thread_prio_affinity(_: ThreadPriority, affinity: &[ThreadAffinity]) {
    let affinity = affinity
        .iter()
        .filter_map(|affinity| ((*affinity as usize) < affinity::get_core_num()).then(|| *affinity as usize))
        .collect::<Vec<_>>();
    affinity::set_thread_affinity(&affinity).unwrap();
}

#[cfg(target_os = "vita")]
pub fn set_thread_prio_affinity(thread_priority: ThreadPriority, thread_affinity: &[ThreadAffinity]) {
    unsafe {
        let id = vitasdk_sys::sceKernelGetThreadId();
        vitasdk_sys::sceKernelChangeThreadPriority(
            id,
            match thread_priority {
                ThreadPriority::Low => vitasdk_sys::SCE_KERNEL_PROCESS_PRIORITY_USER_LOW,
                ThreadPriority::Default => vitasdk_sys::SCE_KERNEL_PROCESS_PRIORITY_USER_DEFAULT,
                ThreadPriority::High => vitasdk_sys::SCE_KERNEL_PROCESS_PRIORITY_USER_HIGH,
            } as _,
        );
        let mut affinity_mask = 0;
        for affinity in thread_affinity {
            affinity_mask |= match affinity {
                ThreadAffinity::Core0 => vitasdk_sys::SCE_KERNEL_CPU_MASK_USER_0,
                ThreadAffinity::Core1 => vitasdk_sys::SCE_KERNEL_CPU_MASK_USER_1,
                ThreadAffinity::Core2 => vitasdk_sys::SCE_KERNEL_CPU_MASK_USER_2,
                ThreadAffinity::Core3 => vitasdk_sys::SCE_KERNEL_CPU_MASK_USER_2 << 1,
            };
        }
        vitasdk_sys::sceKernelChangeThreadCpuAffinityMask(id, affinity_mask as _);
    }
}

#[cfg(profiling)]
extern "C" {
    pub fn gprof_start();
    pub fn gprof_stop(filename: *const vitasdk_sys::c_char, should_dump: vitasdk_sys::c_int);
}

pub fn start_profiling() {
    #[cfg(profiling)]
    unsafe {
        gprof_start();
    }
}

pub fn stop_profiling() {
    #[cfg(profiling)]
    unsafe {
        gprof_stop(c"ux0:/data/gprof_dsvita.out".as_ptr(), 1);
    }
}

pub fn rgb5_to_rgb6(color: u32) -> u32 {
    let r = (color & 0x1F) << 1;
    let g = ((color >> 5) & 0x1F) << 1;
    let b = ((color >> 10) & 0x1F) << 1;
    (color & 0xFFFC0000) | (b << 12) | (g << 6) | r
}

pub fn rgb5_to_rgb8(color: u16) -> u32 {
    let r = (color & 0x1F) as u32 * 255 / 31;
    let g = ((color >> 5) & 0x1F) as u32 * 255 / 31;
    let b = ((color >> 10) & 0x1F) as u32 * 255 / 31;
    (0xFFu32 << 24) | (b << 16) | (g << 8) | r
}

pub fn rgba5_to_rgba8(color: u16) -> u32 {
    let r = (color & 0x1F) as u32 * 255 / 31;
    let g = ((color >> 5) & 0x1F) as u32 * 255 / 31;
    let b = ((color >> 10) & 0x1F) as u32 * 255 / 31;
    let a = !((color >> 15) as u32).wrapping_sub(1);
    (a << 24) | (b << 16) | (g << 8) | r
}

pub fn rgba5_to_rgba8_non_norm(color: u16) -> u32 {
    let r = (color & 0x1F) as u32;
    let g = ((color >> 5) & 0x1F) as u32;
    let b = ((color >> 10) & 0x1F) as u32;
    let a = (color >> 15) as u32;
    (a << 24) | (b << 16) | (g << 8) | r
}

pub fn rgb6_to_rgb8(color: u32) -> u32 {
    let r = (color & 0x3F) * 255 / 63;
    let g = ((color >> 6) & 0x3F) * 255 / 63;
    let b = ((color >> 12) & 0x3F) * 255 / 63;
    (0xFFu32 << 24) | (b << 16) | (g << 8) | r
}

pub fn rgb5_to_float8(color: u16) -> [f32; 3] {
    let r = (color & 0x1F) as f32;
    let g = ((color >> 5) & 0x1F) as f32;
    let b = ((color >> 10) & 0x1F) as f32;
    [r / 31f32, g / 31f32, b / 31f32]
}

pub fn rgb6_to_float8(color: u32) -> (f32, f32, f32) {
    let r = (color & 0x3F) as f32;
    let g = ((color >> 6) & 0x3F) as f32;
    let b = ((color >> 12) & 0x3F) as f32;
    (r / 63f32, g / 63f32, b / 63f32)
}

pub const fn const_bytes_equal(lhs: &[u8], rhs: &[u8]) -> bool {
    if lhs.len() != rhs.len() {
        return false;
    }
    let mut i = 0;
    while i < lhs.len() {
        if lhs[i] != rhs[i] {
            return false;
        }
        i += 1;
    }
    true
}

pub const fn const_str_equal(lhs: &str, rhs: &str) -> bool {
    const_bytes_equal(lhs.as_bytes(), rhs.as_bytes())
}

macro_rules! array_init {
    ($init:block; $size:expr) => {{
        [(); $size].map(|_| $init)
    }};
    ($index:ident; $init:block; $size:expr) => {{
        let mut $index = 0;
        [(); $size].map(|_| {
            let ret = $init;
            $index += 1;
            ret
        })
    }};
}

pub(crate) use array_init;

pub struct PtrWrapper<T>(*mut T);

impl<T> PtrWrapper<T> {
    pub fn new(ptr: *mut T) -> Self {
        PtrWrapper(ptr)
    }

    pub fn null() -> Self {
        Self::default()
    }
}

impl<T> Default for PtrWrapper<T> {
    fn default() -> Self {
        PtrWrapper(ptr::null_mut())
    }
}

impl<T> Deref for PtrWrapper<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        debug_assert_ne!(self.0, ptr::null_mut());
        unsafe { self.0.as_ref_unchecked() }
    }
}

impl<T> DerefMut for PtrWrapper<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        debug_assert_ne!(self.0, ptr::null_mut());
        unsafe { self.0.as_mut_unchecked() }
    }
}

pub struct OptionWrapper<T>(Option<T>);

impl<T> OptionWrapper<T> {
    pub fn new(value: T) -> Self {
        OptionWrapper(Some(value))
    }

    pub fn none() -> Self {
        Self::default()
    }
}

impl<T> Default for OptionWrapper<T> {
    fn default() -> Self {
        OptionWrapper(None)
    }
}

impl<T> Deref for OptionWrapper<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_ref().unwrap_unchecked() }
    }
}

impl<T> DerefMut for OptionWrapper<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.as_mut().unwrap_unchecked() }
    }
}
