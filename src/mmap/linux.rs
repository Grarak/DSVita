use crate::mmap::{ArmContext, MemRegion, VirtualMemMap};
use libc::*;
use std::ffi::CString;
use std::io::{Error, ErrorKind};
use std::ops::{Deref, DerefMut};
use std::{io, mem, ptr, slice};

pub struct Mmap {
    pub ptr: *mut u8,
    size: usize,
}

impl Mmap {
    pub fn rw(_: impl AsRef<str>, addr: usize, size: usize) -> io::Result<Self> {
        Mmap::new(PROT_READ | PROT_WRITE, addr, size)
    }

    pub fn executable(_: impl AsRef<str>, size: usize) -> io::Result<Self> {
        Mmap::new(PROT_READ | PROT_WRITE | PROT_EXEC, 0, size)
    }

    fn new(prot: i32, addr: usize, size: usize) -> io::Result<Self> {
        let ptr = unsafe { mmap(addr as _, size as _, prot, MAP_ANON | MAP_PRIVATE, -1, 0) };
        if ptr != MAP_FAILED && (addr == 0 || ptr as usize == addr) {
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

pub struct Shm {
    fd: i32,
    ptr: *mut u8,
    size: usize,
}

impl Shm {
    pub fn new(name: impl AsRef<str>, size: usize) -> io::Result<Self> {
        let name = format!("/dsvita_{}", name.as_ref());
        let name = CString::new(name)?;
        let fd = unsafe { shm_open(name.as_ptr(), O_CREAT | O_EXCL | O_RDWR, S_IREAD | S_IWRITE) };
        unsafe { shm_unlink(name.as_ptr()) };
        if fd >= 0 {
            if unsafe { ftruncate(fd, size as _) == 0 } {
                let ptr = unsafe { mmap(ptr::null_mut(), size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0) };
                if ptr == MAP_FAILED {
                    unsafe { close(fd) };
                    Err(Error::from(ErrorKind::AddrNotAvailable))
                } else {
                    Ok(Shm { fd, ptr: ptr as _, size })
                }
            } else {
                unsafe { close(fd) };
                Err(Error::from(ErrorKind::AddrNotAvailable))
            }
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

impl Drop for Shm {
    fn drop(&mut self) {
        unsafe {
            munmap(self.ptr as _, self.size);
            close(self.fd);
        }
    }
}

impl Deref for Shm {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
}

impl DerefMut for Shm {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) }
    }
}

impl AsRef<[u8]> for Shm {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl AsMut<[u8]> for Shm {
    fn as_mut(&mut self) -> &mut [u8] {
        self.deref_mut()
    }
}

pub struct VirtualMem {
    ptr: *mut u8,
    size: usize,
}

impl VirtualMem {
    pub fn new(virtual_size: usize, addr: usize) -> io::Result<Self> {
        let ptr = unsafe { mmap(addr as _, virtual_size as _, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0) };
        if ptr == MAP_FAILED || ptr as usize != addr {
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

    pub fn create_region_map(&mut self, shm_mem: &Shm, mem_region: &MemRegion) -> io::Result<VirtualMemMap> {
        let prot = if mem_region.allow_write { PROT_READ | PROT_WRITE } else { PROT_READ };
        for addr_offset in (mem_region.start..mem_region.end).step_by(mem_region.size) {
            if unsafe { mmap(self.ptr.add(addr_offset) as _, mem_region.size, prot, MAP_SHARED | MAP_FIXED, shm_mem.fd, mem_region.shm_offset as _) } == MAP_FAILED {
                return Err(Error::from(ErrorKind::AddrNotAvailable));
            }
        }
        Ok(VirtualMemMap::new(unsafe { self.ptr.add(mem_region.start) as _ }, mem_region.region_size()))
    }

    pub fn destroy_region_map(&mut self, mem_region: &MemRegion) {
        for addr_offset in (mem_region.start..mem_region.end).step_by(mem_region.size) {
            unsafe { munmap(self.ptr.add(addr_offset as _) as _, mem_region.size as _) };
        }
    }

    pub fn create_page_map(&mut self, shm: &Shm, shm_start: usize, shm_offset: usize, shm_size: usize, addr: usize, page_size: usize, allow_write: bool) -> io::Result<VirtualMemMap> {
        debug_assert_eq!(addr & (page_size - 1), 0);
        let prot = if allow_write { PROT_READ | PROT_WRITE } else { PROT_READ };
        let shm_offset = shm_start + (shm_offset & (shm_size - 1));
        if unsafe { mmap(self.ptr.add(addr) as _, page_size, prot, MAP_SHARED | MAP_FIXED, shm.fd, shm_offset as _) } == MAP_FAILED {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        } else {
            Ok(VirtualMemMap::new(unsafe { self.ptr.add(addr) as _ }, page_size))
        }
    }

    pub fn create_map(&mut self, shm: &Shm, shm_offset: usize, addr: usize, size: usize, read: bool, write: bool, exe: bool) -> io::Result<VirtualMemMap> {
        let mut prot = PROT_NONE;
        if read {
            prot |= PROT_READ;
        }
        if write {
            prot |= PROT_WRITE;
        }
        if exe {
            prot |= PROT_EXEC;
        }

        if unsafe { mmap(self.ptr.add(addr) as _, size, prot, MAP_SHARED | MAP_FIXED, shm.fd, shm_offset as _) } == MAP_FAILED {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        } else {
            Ok(VirtualMemMap::new(unsafe { self.ptr.add(addr) as _ }, size))
        }
    }

    pub fn destroy_map(&mut self, start: usize, size: usize) -> i32 {
        unsafe { munmap(self.ptr.add(start) as _, size as _) }
    }

    pub fn set_protection(&mut self, start: usize, size: usize, read: bool, write: bool, exe: bool) {
        unsafe { set_protection(self.ptr.add(start), size, read, write, exe) }
    }

    pub fn set_region_protection(&mut self, start: usize, size: usize, mem_region: &MemRegion, read: bool, write: bool, exe: bool) {
        let base_offset = start - mem_region.start;
        let base_offset = base_offset & (mem_region.size - 1);
        for addr_offset in (mem_region.start + base_offset..mem_region.end).step_by(mem_region.size) {
            unsafe { set_protection(self.ptr.add(addr_offset), size, read, write, exe) }
        }
    }
}

pub unsafe fn set_protection(start: *mut u8, size: usize, read: bool, write: bool, exe: bool) {
    let mut prot = PROT_NONE;
    if read {
        prot |= PROT_READ;
    }
    if write {
        prot |= PROT_WRITE;
    }
    if exe {
        prot |= PROT_EXEC;
    }
    mprotect(start as _, size as _, prot);
}

extern "C" {
    fn built_in_clear_cache(start: *const u8, end: *const u8);
}

pub unsafe fn flush_icache(start: *const u8, size: usize) {
    built_in_clear_cache(start, start.add(size));
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

static mut DELEGATE_FUN: *const fn(usize, &mut usize, &ArmContext) -> bool = ptr::null();
static mut NEXT_SEGV_HANDLER: sigaction = unsafe { mem::zeroed::<sigaction>() };

unsafe extern "C" fn sigsegv_handler(sig: i32, si: *mut siginfo_t, segfault_ctx: *mut c_void) {
    let si_addr = (*si).si_addr();
    let context = segfault_ctx as *mut ucontext_t;
    let context = &mut (*context).uc_mcontext;

    let delegate_fun: fn(usize, &mut usize, &ArmContext) -> bool = mem::transmute(DELEGATE_FUN);
    let arm_context: &ArmContext = unsafe { mem::transmute(&context.arm_r0) };
    let mut pc = context.arm_pc as usize;
    if delegate_fun(si_addr as usize, &mut pc, arm_context) {
        context.arm_pc = pc as _;
        return;
    }

    if NEXT_SEGV_HANDLER.sa_sigaction != 0 {
        let action: extern "C" fn(i32, *mut siginfo_t, *mut c_void) = mem::transmute(NEXT_SEGV_HANDLER.sa_sigaction);
        action(sig, si, segfault_ctx);
    } else {
        panic!();
    }
}

pub unsafe fn register_abort_handler(delegate: fn(usize, &mut usize, &ArmContext) -> bool) -> io::Result<()> {
    DELEGATE_FUN = delegate as *const _;
    let mut sa = mem::zeroed::<sigaction>();
    sa.sa_flags = SA_SIGINFO;
    sigemptyset(&mut sa.sa_mask);
    sa.sa_sigaction = sigsegv_handler as *const () as _;
    if sigaction(SIGSEGV, &sa, ptr::addr_of_mut!(NEXT_SEGV_HANDLER)) != 0 {
        Err(Error::from(ErrorKind::Other))
    } else {
        Ok(())
    }
}
