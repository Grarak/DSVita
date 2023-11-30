use crate::utils;
use std::ffi::CString;
use std::io::{Error, ErrorKind};
use std::ops::{Deref, DerefMut};
use std::ptr::null_mut;
use std::{io, slice};
use std::{mem, ptr};

pub struct Mmap {
    pub block_uid: vitasdk_sys::SceUID,
    ptr: *mut vitasdk_sys::c_void,
    size: u32,
}

impl Mmap {
    pub fn new(name: &str, exec: bool, size: u32) -> io::Result<Self> {
        // let mut opts: vitasdk_sys::SceKernelAllocMemBlockKernelOpt = unsafe { mem::zeroed() };
        // opts.size = mem::size_of::<vitasdk_sys::SceKernelAllocMemBlockKernelOpt>() as _;
        // opts.attr = 0x1;
        // opts.field_C = base as _;
        //
        // println!("Trying to map at {:x}", base as u32);
        //
        // let sce_uid = unsafe {
        //     kubridge_binding::kuKernelAllocMemBlock(
        //         "code\0".as_ptr() as _,
        //         vitasdk_sys::SCE_KERNEL_MEMBLOCK_TYPE_USER_MAIN_RX,
        //         size as _,
        //         &mut opts as _,
        //     )
        // };

        let c_name = CString::new(name).unwrap();

        let block_uid = if exec {
            unsafe { vitasdk_sys::sceKernelAllocMemBlockForVM(c_name.as_c_str().as_ptr(), size) }
        } else {
            let mut opts: vitasdk_sys::SceKernelAllocMemBlockOpt = unsafe { mem::zeroed() };
            opts.size = mem::size_of::<vitasdk_sys::SceKernelAllocMemBlockOpt>() as _;
            opts.attr = vitasdk_sys::SCE_KERNEL_ALLOC_MEMBLOCK_ATTR_HAS_ALIGNMENT;
            opts.alignment = 256 * 1024;
            unsafe {
                vitasdk_sys::sceKernelAllocMemBlock(
                    c_name.as_c_str().as_ptr() as _,
                    vitasdk_sys::SCE_KERNEL_MEMBLOCK_TYPE_USER_RW,
                    utils::align_up(size as _, opts.alignment),
                    ptr::addr_of_mut!(opts),
                )
            }
        };

        if block_uid < vitasdk_sys::SCE_OK as i32 {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        } else {
            let mut base: *mut vitasdk_sys::c_void = null_mut();
            let ret = unsafe {
                vitasdk_sys::sceKernelGetMemBlockBase(block_uid, ptr::addr_of_mut!(base))
            };

            if ret < vitasdk_sys::SCE_OK as i32 {
                Err(Error::from(ErrorKind::AddrNotAvailable))
            } else {
                Ok(Mmap {
                    block_uid,
                    ptr: base,
                    size,
                })
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
