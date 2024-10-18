use crate::mmap::platform::kubridge::{
    kuKernelMemCommit, kuKernelMemDecommit, kuKernelMemProtect, kuKernelMemReserve, KuKernelMemCommitOpt, KU_KERNEL_MEM_COMMIT_ATTR_HAS_BASE, KU_KERNEL_PROT_NONE, KU_KERNEL_PROT_READ,
    KU_KERNEL_PROT_WRITE,
};
use crate::mmap::{MemRegion, VirtualMemMap, PAGE_SIZE};
use crate::utils;
use std::ffi::CString;
use std::io::{Error, ErrorKind};
use std::ops::{Deref, DerefMut};
use std::{io, slice};
use std::{mem, ptr};
use vitasdk_sys::*;

mod kubridge {
    #![allow(warnings, unused)]
    include!(concat!(env!("OUT_DIR"), "/kubridge_bindings.rs"));
}

pub struct Mmap {
    pub block_uid: SceUID,
    ptr: *mut c_void,
    size: usize,
}

impl Mmap {
    pub fn executable(name: impl AsRef<str>, size: usize) -> io::Result<Self> {
        let c_name = CString::new(name.as_ref())?;

        let block_uid = unsafe { sceKernelAllocMemBlockForVM(c_name.as_c_str().as_ptr(), size as u32) };
        Mmap::new(block_uid, size)
    }

    fn new(block_uid: SceUID, size: usize) -> io::Result<Self> {
        if block_uid < SCE_OK as i32 {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        } else {
            let mut base = ptr::null_mut();
            let ret = unsafe { sceKernelGetMemBlockBase(block_uid, &mut base) };

            if ret < SCE_OK as i32 {
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
        unsafe { sceKernelFreeMemBlock(self.block_uid) };
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

pub struct Shm(SceUID);

impl Shm {
    pub fn new(name: impl AsRef<str>, size: usize) -> io::Result<Self> {
        let c_name = CString::new(name.as_ref())?;

        let block_uid = unsafe {
            sceKernelAllocMemBlock(
                c_name.as_c_str().as_ptr() as _,
                SCE_KERNEL_MEMBLOCK_TYPE_USER_RW,
                utils::align_up(size, PAGE_SIZE) as u32,
                ptr::null_mut(),
            )
        };

        if block_uid < SCE_OK as i32 {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        } else {
            Ok(Shm(block_uid))
        }
    }
}

impl Drop for Shm {
    fn drop(&mut self) {
        unsafe { sceKernelFreeMemBlock(self.0) };
    }
}

pub struct VirtualMem {
    ptr: *mut u8,
    vmem_block: SceUID,
    size: usize,
}

impl VirtualMem {
    pub fn new(virtual_size: usize) -> io::Result<Self> {
        let mut ptr = ptr::null_mut();
        let vmem_block = unsafe { kuKernelMemReserve(&mut ptr, virtual_size as _, SCE_KERNEL_MEMBLOCK_TYPE_USER_RW) };
        if vmem_block >= 0 {
            Ok(VirtualMem {
                ptr: ptr as _,
                vmem_block,
                size: virtual_size,
            })
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

    pub fn create_mapping(&mut self, shm_mem: &Shm, mem_region: &MemRegion) -> io::Result<VirtualMemMap> {
        if mem_region.size == 0 {
            if unsafe { kuKernelMemProtect(self.ptr.add(mem_region.start) as _, mem_region.region_size() as _, KU_KERNEL_PROT_NONE) } != 0 {
                return Err(Error::from(ErrorKind::AddrNotAvailable));
            }
        } else {
            let mut opt = unsafe { mem::zeroed::<KuKernelMemCommitOpt>() };
            opt.size = size_of::<KuKernelMemCommitOpt>() as _;
            opt.attr = KU_KERNEL_MEM_COMMIT_ATTR_HAS_BASE;
            opt.baseBlock = shm_mem.0;

            let prot = if mem_region.allow_write {
                KU_KERNEL_PROT_READ | KU_KERNEL_PROT_WRITE
            } else {
                KU_KERNEL_PROT_READ
            };
            for offset in (0..mem_region.region_size()).step_by(PAGE_SIZE) {
                let virtual_offset = mem_region.start + offset;
                let physical_offset = mem_region.shm_offset + (offset & (mem_region.size - 1));
                opt.baseOffset = physical_offset as _;
                if unsafe { kuKernelMemCommit(self.ptr.add(virtual_offset) as _, PAGE_SIZE as _, prot, &mut opt) } != 0 {
                    return Err(Error::from(ErrorKind::AddrNotAvailable));
                }
            }
        }
        Ok(VirtualMemMap::new(unsafe { self.ptr.add(mem_region.start) as _ }, mem_region.region_size()))
    }

    pub fn create_page_mapping(&mut self, shm_mem: &Shm, shm_start: usize, shm_offset: usize, shm_size: usize, addr: usize, allow_write: bool) -> io::Result<VirtualMemMap> {
        debug_assert_eq!(addr & (PAGE_SIZE - 1), 0);
        let prot = if allow_write { KU_KERNEL_PROT_READ | KU_KERNEL_PROT_WRITE } else { KU_KERNEL_PROT_READ };

        let shm_offset = shm_start + (shm_offset & (shm_size - 1));

        let mut opt = unsafe { mem::zeroed::<KuKernelMemCommitOpt>() };
        opt.size = size_of::<KuKernelMemCommitOpt>() as _;
        opt.attr = KU_KERNEL_MEM_COMMIT_ATTR_HAS_BASE;
        opt.baseBlock = shm_mem.0;
        opt.baseOffset = shm_offset as _;

        if unsafe { kuKernelMemCommit(self.ptr.add(addr) as _, PAGE_SIZE as _, prot, &mut opt) } != 0 {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        } else {
            Ok(VirtualMemMap::new(unsafe { self.ptr.add(addr) as _ }, PAGE_SIZE))
        }
    }

    pub fn destroy_page_mapping(&mut self, addr: usize) {
        debug_assert_eq!(addr & (PAGE_SIZE - 1), 0);
        unsafe { kuKernelMemDecommit(self.ptr.add(addr) as _, PAGE_SIZE as _) };
    }
}

impl Drop for VirtualMem {
    fn drop(&mut self) {
        unsafe { sceKernelFreeMemBlock(self.vmem_block) };
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
