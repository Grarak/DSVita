use std::cell::RefCell;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};

pub const fn align_up(n: u32, align: u32) -> u32 {
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

pub fn negative<T: Convert>(n: T) -> T {
    T::from(!(n.into() - 1))
}

pub fn read_from_mem<T: Clone>(mem: &[u8], addr: u32) -> T {
    let (_, aligned, _) = unsafe { mem[addr as usize..].align_to::<T>() };
    aligned[0].clone()
}

pub fn read_from_mem_slice<T: Copy>(mem: &[u8], addr: u32, slice: &mut [T]) {
    let (_, aligned, _) = unsafe { mem[addr as usize..].align_to::<T>() };
    slice.copy_from_slice(&aligned[..slice.len()]);
}

pub fn write_to_mem<T>(mem: &mut [u8], addr: u32, value: T) {
    let (_, aligned, _) = unsafe { mem[addr as usize..].align_to_mut::<T>() };
    aligned[0] = value
}

pub fn write_to_mem_slice<T: Copy>(mem: &mut [u8], addr: u32, slice: &[T]) {
    let (_, aligned, _) = unsafe { mem[addr as usize..].align_to_mut::<T>() };
    aligned[..slice.len()].copy_from_slice(slice)
}

pub struct StrErr {
    str: String,
}

impl StrErr {
    pub fn new(str: String) -> Self {
        StrErr { str }
    }
}

impl From<&str> for StrErr {
    fn from(value: &str) -> Self {
        StrErr::new(value.to_string())
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

pub type FastCell<T> = RefCell<T>;
// Might give better performance
// pub struct FastCell<T: ?Sized> {
//     value: UnsafeCell<T>,
// }
//
// impl<T> FastCell<T> {
//     pub const fn new(value: T) -> Self {
//         FastCell {
//             value: UnsafeCell::new(value),
//         }
//     }
// }
//
// impl<T: ?Sized> FastCell<T> {
//     pub fn borrow(&self) -> &T {
//         unsafe { self.value.get().as_ref().unwrap() }
//     }
//
//     pub fn borrow_mut(&self) -> &mut T {
//         unsafe { self.value.get().as_mut().unwrap() }
//     }
//
//     pub fn as_ptr(&self) -> *mut T {
//         self.value.get()
//     }
// }

pub struct HeapMem<const T: usize>(Box<[u8; T]>);

impl<const T: usize> HeapMem<T> {
    pub fn new() -> Self {
        HeapMem(Box::new([0u8; T]))
    }
}

impl<const T: usize> Deref for HeapMem<T> {
    type Target = Box<[u8; T]>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const T: usize> DerefMut for HeapMem<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<const T: usize> Default for HeapMem<T> {
    fn default() -> Self {
        HeapMem::new()
    }
}

pub const fn crc16(mut crc: u32, buf: &[u8], start: usize, size: usize) -> u16 {
    const TABLE: [u16; 8] = [
        0xC0C1, 0xC181, 0xC301, 0xC601, 0xCC01, 0xD801, 0xF001, 0xA001,
    ];

    let mut i = start;
    while i < start + size {
        crc ^= buf[i] as u32;
        let mut j = 0;
        while j < TABLE.len() {
            crc = (crc >> 1)
                ^ if crc & 1 != 0 {
                    (TABLE[j] as u32) << (7 - j)
                } else {
                    0
                };
            j += 1;
        }
        i += 1;
    }
    crc as u16
}
