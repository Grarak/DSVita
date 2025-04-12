pub use self::platform::*;
use static_assertions::const_assert_eq;
use std::ops::{Deref, DerefMut};
use std::slice;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;

#[cfg(target_os = "vita")]
#[path = "vita.rs"]
mod platform;

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_SHIFT: usize = 12;
const_assert_eq!(PAGE_SIZE, 1 << PAGE_SHIFT);

pub struct ArmContext {
    pub gp_regs: [usize; 13],
    pub sp: usize,
    pub lr: usize,
}

pub struct MemRegion {
    pub start: usize,
    pub end: usize,
    pub size: usize,
    pub shm_offset: usize,
    pub allow_write: bool,
}

impl MemRegion {
    pub const fn new(start: usize, end: usize, size: usize, shm_offset: usize, allow_write: bool) -> Self {
        MemRegion {
            start,
            end,
            size,
            shm_offset,
            allow_write,
        }
    }

    pub const fn region_size(&self) -> usize {
        self.end - self.start
    }
}

pub struct VirtualMemMap {
    ptr: *mut u8,
    size: usize,
}

impl VirtualMemMap {
    fn new(ptr: *mut u8, size: usize) -> Self {
        VirtualMemMap { ptr, size }
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr as _
    }

    pub fn len(&self) -> usize {
        self.size as _
    }
}

impl Deref for VirtualMemMap {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
}

impl DerefMut for VirtualMemMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) }
    }
}

impl AsRef<[u8]> for VirtualMemMap {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl AsMut<[u8]> for VirtualMemMap {
    fn as_mut(&mut self) -> &mut [u8] {
        self.deref_mut()
    }
}

struct HostContext {
    pc: usize,
}
