use std::io::{Error, ErrorKind};
use std::ops::{Deref, DerefMut};
use std::ptr::null_mut;
use std::{io, slice};

pub struct Mmap {
    ptr: *mut libc::c_void,
    size: u32,
}

impl Mmap {
    pub fn new(_: &str, exec: bool, size: u32) -> io::Result<Self> {
        let mut prot = libc::PROT_READ | libc::PROT_WRITE;
        if exec {
            prot |= libc::PROT_EXEC;
        }
        let ptr =
            unsafe {
                libc::mmap(
                    null_mut(),
                    size as _,
                    prot,
                    libc::MAP_ANON | libc::MAP_PRIVATE,
                    -1,
                    0,
                )
            };
        if ptr != libc::MAP_FAILED {
            Ok(Mmap { ptr, size })
        } else {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        }
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr as _
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr as _
    }

    pub fn len(&self) -> usize {
        self.size as _
    }
}

impl Drop for Mmap {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr, self.size as _) };
    }
}

impl Deref for Mmap {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
}

impl DerefMut for Mmap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) }
    }
}

impl AsRef<[u8]> for Mmap {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl AsMut<[u8]> for Mmap {
    fn as_mut(&mut self) -> &mut [u8] {
        self.deref_mut()
    }
}
