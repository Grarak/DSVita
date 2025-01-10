use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::hash::{BuildHasher, Hasher};
use std::ops::{Deref, DerefMut};
use std::{cmp, slice};

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

pub fn read_from_mem<T: Clone>(mem: &[u8], addr: u32) -> T {
    unsafe { (mem.as_ptr().add(addr as usize) as *const T).read() }
}

pub fn read_from_mem_slice<T: Copy>(mem: &[u8], addr: u32, slice: &mut [T]) -> usize {
    let read_amount = cmp::min(mem.len() / size_of::<T>(), slice.len());
    unsafe { (mem.as_ptr().add(addr as usize) as *const T).copy_to(slice.as_mut_ptr(), read_amount) };
    read_amount
}

pub fn write_to_mem<T>(mem: &mut [u8], addr: u32, value: T) {
    unsafe { (mem.as_mut_ptr().add(addr as usize) as *mut T).write(value) }
}

pub fn write_to_mem_slice<T: Copy>(mem: &mut [u8], addr: usize, slice: &[T]) -> usize {
    let write_amount = cmp::min(mem.len() / size_of::<T>(), slice.len());
    unsafe { (mem.as_mut_ptr().add(addr) as *mut T).copy_from(slice.as_ptr(), write_amount) };
    write_amount
}

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

pub type HeapMemU8<const SIZE: usize> = HeapMem<u8, SIZE>;
pub type HeapMemU16<const SIZE: usize> = HeapMem<u16, SIZE>;
pub type HeapMemU32<const SIZE: usize> = HeapMem<u32, SIZE>;
pub type HeapMemUsize<const SIZE: usize> = HeapMem<usize, SIZE>;

pub struct HeapMem<T: Sized, const SIZE: usize>(Box<[T; SIZE]>);

impl<T: Sized, const SIZE: usize> HeapMem<T, SIZE> {
    pub unsafe fn zeroed() -> Self {
        let mem: Box<[T; SIZE]> = Box::new_zeroed().assume_init();
        HeapMem(mem)
    }
}

impl<T: Sized + Default, const SIZE: usize> HeapMem<T, SIZE> {
    pub fn new() -> Self {
        HeapMem::default()
    }
}

impl<T: Sized, const SIZE: usize> Deref for HeapMem<T, SIZE> {
    type Target = [T; SIZE];

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<T: Sized, const SIZE: usize> DerefMut for HeapMem<T, SIZE> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.deref_mut()
    }
}

impl<T: Sized + Default, const SIZE: usize> Default for HeapMem<T, SIZE> {
    fn default() -> Self {
        let mut mem: Box<[T; SIZE]> = unsafe { Box::new_zeroed().assume_init() };
        mem.fill_with(|| T::default());
        HeapMem(mem)
    }
}

impl<T: Sized + Copy, const SIZE: usize> Clone for HeapMem<T, SIZE> {
    fn clone(&self) -> Self {
        let mut mem: Box<[T; SIZE]> = unsafe { Box::new_zeroed().assume_init() };
        mem.copy_from_slice(self.deref());
        HeapMem(mem)
    }
}

impl<T: Debug, const SIZE: usize> Debug for HeapMem<T, SIZE> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_map();
        for (i, v) in self.deref().iter().enumerate() {
            list.entry(&i, v);
        }
        list.finish()
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
}

#[cfg(target_os = "linux")]
pub fn set_thread_prio_affinity(_: ThreadPriority, affinity: ThreadAffinity) {
    if (affinity as usize) < affinity::get_core_num() {
        affinity::set_thread_affinity(&[affinity as usize]).unwrap();
    }
}

#[cfg(target_os = "vita")]
pub fn set_thread_prio_affinity(thread_priority: ThreadPriority, thread_affinity: ThreadAffinity) {
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
        vitasdk_sys::sceKernelChangeThreadCpuAffinityMask(
            id,
            match thread_affinity {
                ThreadAffinity::Core0 => vitasdk_sys::SCE_KERNEL_CPU_MASK_USER_0,
                ThreadAffinity::Core1 => vitasdk_sys::SCE_KERNEL_CPU_MASK_USER_1,
                ThreadAffinity::Core2 => vitasdk_sys::SCE_KERNEL_CPU_MASK_USER_2,
            } as _,
        );
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

pub fn rgb6_to_rgb8(color: u32) -> u32 {
    let r = (color & 0x3F) * 255 / 63;
    let g = ((color >> 6) & 0x3F) * 255 / 63;
    let b = ((color >> 12) & 0x3F) * 255 / 63;
    (0xFFu32 << 24) | (b << 16) | (g << 8) | r
}

pub fn rgb5_to_float8(color: u16) -> (f32, f32, f32) {
    let r = (color & 0x1F) as f32;
    let g = ((color >> 5) & 0x1F) as f32;
    let b = ((color >> 10) & 0x1F) as f32;
    (r / 31f32, g / 31f32, b / 31f32)
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
