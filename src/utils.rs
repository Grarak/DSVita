use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

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
