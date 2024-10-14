use libc::*;
use std::ffi::CString;
use std::io::{Error, ErrorKind};
use std::ops::{Deref, DerefMut};
use std::{io, ptr, slice};

pub struct Mmap {
    ptr: *mut u8,
    size: usize,
}

impl Mmap {
    pub fn executable(_: impl AsRef<str>, size: usize) -> io::Result<Self> {
        Mmap::new(PROT_READ | PROT_WRITE | PROT_EXEC, size)
    }

    fn new(prot: i32, size: usize) -> io::Result<Self> {
        let ptr = unsafe { mmap(ptr::null_mut(), size as _, prot, libc::MAP_ANON | libc::MAP_PRIVATE, -1, 0) };
        if ptr != MAP_FAILED {
            Ok(Mmap { ptr: ptr as _, size })
        } else {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        }
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

impl Drop for Mmap {
    fn drop(&mut self) {
        unsafe { munmap(self.ptr as _, self.size as _) };
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

pub struct ShmMem(i32);

impl ShmMem {
    pub fn new(name: impl AsRef<str>, size: usize) -> io::Result<Self> {
        let name = CString::new(name.as_ref())?;
        let fd = unsafe { shm_open(name.as_ptr() as _, O_CREAT | O_EXCL | O_RDWR, S_IREAD | S_IWRITE) };
        unsafe { shm_unlink(name.as_ptr() as _) };
        if fd >= 0 {
            if unsafe { ftruncate(fd, size as _) == 0 } {
                Ok(ShmMem(fd))
            } else {
                unsafe { close(fd) };
                Err(Error::from(ErrorKind::AddrNotAvailable))
            }
        } else {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        }
    }
}

impl Drop for ShmMem {
    fn drop(&mut self) {
        unsafe { close(self.0) };
    }
}

pub struct VirtualMemMap {
    ptr: *mut u8,
    size: usize,
    page_size: usize,
}

impl VirtualMemMap {
    fn new(ptr: *mut u8, size: usize, page_size: usize) -> Self {
        VirtualMemMap { ptr, size, page_size }
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

impl Drop for VirtualMemMap {
    fn drop(&mut self) {
        for addr_offset in (0..self.size).step_by(self.page_size) {
            unsafe { munmap(self.ptr.add(addr_offset) as _, self.page_size as _) };
        }
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

pub struct VirtualMem {
    ptr: *mut u8,
    size: usize,
}

impl VirtualMem {
    pub fn new(virtual_size: usize) -> io::Result<Self> {
        let ptr = unsafe { mmap(ptr::null_mut(), virtual_size as _, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0) };
        if ptr == MAP_FAILED {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        } else {
            Ok(VirtualMem { ptr: ptr as _, size: virtual_size })
        }
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

    pub fn create_mapping(&mut self, shm_mem: &ShmMem, shm_mem_offset: usize, map_size: usize, start_addr: usize, end_addr: usize, allow_write: bool) -> io::Result<VirtualMemMap> {
        if map_size == 0 {
            let range = end_addr - start_addr;
            if unsafe { mprotect(self.ptr.add(start_addr) as _, range as _, PROT_NONE) } != 0 {
                return Err(Error::from(ErrorKind::AddrNotAvailable));
            }
        } else {
            let prot = if allow_write { PROT_READ | PROT_WRITE } else { PROT_READ };
            for addr_offset in (start_addr..end_addr).step_by(map_size) {
                if unsafe { mmap(self.ptr.add(addr_offset) as _, map_size, prot, MAP_SHARED | MAP_FIXED, shm_mem.0, shm_mem_offset as _) } == MAP_FAILED {
                    return Err(Error::from(ErrorKind::AddrNotAvailable));
                }
            }
        }
        Ok(VirtualMemMap::new(unsafe { self.ptr.add(start_addr) as _ }, end_addr - start_addr, map_size))
    }
}

impl Drop for VirtualMem {
    fn drop(&mut self) {
        unsafe { munmap(self.ptr as _, self.size) };
    }
}

impl Deref for VirtualMem {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
}

impl DerefMut for VirtualMem {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) }
    }
}

impl AsRef<[u8]> for VirtualMem {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl AsMut<[u8]> for VirtualMem {
    fn as_mut(&mut self) -> &mut [u8] {
        self.deref_mut()
    }
}
