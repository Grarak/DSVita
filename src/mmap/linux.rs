use std::alloc::{AllocError, Allocator, Layout};
use std::io::{Error, ErrorKind};
use std::ops::{Deref, DerefMut};
use std::ptr::{null_mut, NonNull};
use std::{io, slice};

pub struct Mmap {
    ptr: *mut libc::c_void,
    size: u32,
}

impl Mmap {
    pub fn rw(_: &str, size: u32) -> io::Result<Self> {
        Mmap::new(libc::PROT_READ | libc::PROT_WRITE, size)
    }

    pub fn executable(_: &str, size: u32) -> io::Result<Self> {
        Mmap::new(libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC, size)
    }

    fn new(prot: i32, size: u32) -> io::Result<Self> {
        let ptr = unsafe { libc::mmap(null_mut(), size as _, prot, libc::MAP_ANON | libc::MAP_PRIVATE, -1, 0) };
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

pub struct MmapAllocator;

unsafe impl Allocator for MmapAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let ret = unsafe { libc::mmap(null_mut(), layout.size(), libc::PROT_READ | libc::PROT_WRITE, libc::MAP_PRIVATE | libc::MAP_ANONYMOUS, -1, 0) };
        if ret == libc::MAP_FAILED {
            Err(AllocError)
        } else {
            Ok(unsafe { NonNull::from(slice::from_raw_parts_mut(ret as *mut u8, layout.size())) })
        }
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe { libc::munmap(ptr.as_ptr() as _, layout.size() as _) };
    }
}
