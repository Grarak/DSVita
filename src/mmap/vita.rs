use crate::utils;
use std::ffi::CString;
use std::io::{Error, ErrorKind};
use std::ops::{Deref, DerefMut};
use std::{io, slice};
use std::{mem, ptr};

pub struct Mmap {
    pub block_uid: vitasdk_sys::SceUID,
    ptr: *mut vitasdk_sys::c_void,
    size: usize,
}

impl Mmap {
    pub fn rw(name: &str, size: usize) -> io::Result<Self> {
        let c_name = CString::new(name)?;

        let mut opts: vitasdk_sys::SceKernelAllocMemBlockOpt = unsafe { mem::zeroed() };
        opts.size = size_of::<vitasdk_sys::SceKernelAllocMemBlockOpt>() as _;
        opts.attr = vitasdk_sys::SCE_KERNEL_MEMORY_ACCESS_R;
        opts.alignment = 4 * 1024;

        let block_uid = unsafe {
            vitasdk_sys::sceKernelAllocMemBlock(
                c_name.as_c_str().as_ptr() as _,
                vitasdk_sys::SCE_KERNEL_MEMBLOCK_TYPE_USER_RW,
                utils::align_up(size, opts.alignment as usize) as u32,
                &mut opts,
            )
        };

        Mmap::new(block_uid, size)
    }

    pub fn executable(name: &str, size: usize) -> io::Result<Self> {
        let c_name = CString::new(name)?;

        let block_uid = unsafe { vitasdk_sys::sceKernelAllocMemBlockForVM(c_name.as_c_str().as_ptr(), size as u32) };
        Mmap::new(block_uid, size)
    }

    fn new(block_uid: vitasdk_sys::SceUID, size: usize) -> io::Result<Self> {
        if block_uid < vitasdk_sys::SCE_OK as i32 {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        } else {
            let mut base = ptr::null_mut();
            let ret = unsafe { vitasdk_sys::sceKernelGetMemBlockBase(block_uid, &mut base) };

            if ret < vitasdk_sys::SCE_OK as i32 {
                Err(Error::from(ErrorKind::AddrNotAvailable))
            } else {
                Ok(Mmap { block_uid, ptr: base, size })
            }
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
        unsafe { vitasdk_sys::sceKernelFreeMemBlock(self.block_uid) };
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
