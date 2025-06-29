use crate::mmap::platform::kubridge::{
    kuKernelAllocMemBlock, kuKernelFlushCaches, kuKernelMemCommit, kuKernelMemDecommit, kuKernelMemProtect, kuKernelMemReserve, kuKernelRegisterAbortHandler, KuKernelAbortContext,
    KuKernelAbortHandler, KuKernelMemCommitOpt, KU_KERNEL_MEM_COMMIT_ATTR_HAS_BASE, KU_KERNEL_PROT_EXEC, KU_KERNEL_PROT_NONE, KU_KERNEL_PROT_READ, KU_KERNEL_PROT_WRITE,
};
use crate::mmap::{ArmContext, MemRegion, VirtualMemMap, PAGE_SIZE};
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
    pub ptr: *mut c_void,
    size: usize,
}

impl Mmap {
    pub fn rw(name: impl AsRef<str>, addr: usize, size: usize) -> io::Result<Self> {
        let c_name = CString::new(name.as_ref())?;

        let block_uid = unsafe {
            let mut opt = mem::zeroed::<kubridge::SceKernelAllocMemBlockKernelOpt>();
            opt.size = size_of::<kubridge::SceKernelAllocMemBlockKernelOpt>() as _;
            opt.attr = 0x1;
            opt.field_C = addr as _;
            kuKernelAllocMemBlock(c_name.as_ptr(), SCE_KERNEL_MEMBLOCK_TYPE_USER_RW, size as u32, &mut opt)
        };
        let mmap = Mmap::new(block_uid, size)?;
        if addr == 0 || addr == mmap.ptr as usize {
            Ok(mmap)
        } else {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        }
    }

    pub fn executable(name: impl AsRef<str>, size: usize) -> io::Result<Self> {
        let c_name = CString::new(name.as_ref())?;

        let block_uid = unsafe {
            let mut opt = mem::zeroed::<kubridge::SceKernelAllocMemBlockKernelOpt>();
            opt.size = size_of::<kubridge::SceKernelAllocMemBlockKernelOpt>() as _;
            opt.attr = 0x1;
            opt.field_C = 0x98000000;
            kuKernelAllocMemBlock(c_name.as_ptr(), SCE_KERNEL_MEMBLOCK_TYPE_USER_RW, size as u32, &mut opt)
        };
        let mmap = Mmap::new(block_uid, size)?;
        unsafe { set_protection(mmap.as_ptr() as _, size, true, true, true) };
        Ok(mmap)
    }

    fn new(block_uid: SceUID, size: usize) -> io::Result<Self> {
        if block_uid < SCE_OK as i32 {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        } else {
            let mut base = ptr::null_mut();
            let ret = unsafe { sceKernelGetMemBlockBase(block_uid, &mut base) };

            if ret < SCE_OK as i32 {
                unsafe { sceKernelFreeMemBlock(block_uid) };
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

pub struct Shm {
    block_uid: SceUID,
    ptr: *mut u8,
    size: usize,
}

impl Shm {
    pub fn new(name: impl AsRef<str>, size: usize) -> io::Result<Self> {
        let c_name = CString::new(name.as_ref())?;

        let block_uid = unsafe { sceKernelAllocMemBlock(c_name.as_ptr() as _, SCE_KERNEL_MEMBLOCK_TYPE_USER_RW, utils::align_up(size, PAGE_SIZE) as u32, ptr::null_mut()) };

        if block_uid < SCE_OK as i32 {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        } else {
            let mut ptr = ptr::null_mut();
            let ret = unsafe { sceKernelGetMemBlockBase(block_uid, &mut ptr) };
            if ret < SCE_OK as i32 {
                unsafe { sceKernelFreeMemBlock(block_uid) };
                Err(Error::from(ErrorKind::AddrNotAvailable))
            } else {
                Ok(Shm { block_uid, ptr: ptr as _, size })
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

impl Drop for Shm {
    fn drop(&mut self) {
        unsafe { sceKernelFreeMemBlock(self.block_uid) };
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
    vmem_block: SceUID,
    size: usize,
}

impl VirtualMem {
    pub fn new(virtual_size: usize, addr: usize) -> io::Result<Self> {
        let mut ptr = addr as *mut c_void;
        let vmem_block = unsafe { kuKernelMemReserve(&mut ptr, virtual_size as _, SCE_KERNEL_MEMBLOCK_TYPE_USER_RW) };
        if vmem_block >= 0 && ptr as usize == addr {
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

    pub fn create_region_map(&mut self, shm_mem: &Shm, mem_region: &MemRegion) -> io::Result<VirtualMemMap> {
        let mut opt = unsafe { mem::zeroed::<KuKernelMemCommitOpt>() };
        opt.size = size_of::<KuKernelMemCommitOpt>() as _;
        opt.attr = KU_KERNEL_MEM_COMMIT_ATTR_HAS_BASE;
        opt.baseBlock = shm_mem.block_uid;
        opt.baseOffset = mem_region.shm_offset as _;

        let prot = if mem_region.allow_write {
            KU_KERNEL_PROT_READ | KU_KERNEL_PROT_WRITE
        } else {
            KU_KERNEL_PROT_READ
        };
        for addr_offset in (mem_region.start..mem_region.end).step_by(mem_region.size) {
            if unsafe { kuKernelMemCommit(self.ptr.add(addr_offset) as _, mem_region.size as _, prot, &mut opt) } != 0 {
                return Err(Error::from(ErrorKind::AddrNotAvailable));
            }
        }
        Ok(VirtualMemMap::new(unsafe { self.ptr.add(mem_region.start) as _ }, mem_region.region_size()))
    }

    pub fn destroy_region_map(&mut self, mem_region: &MemRegion) {
        for addr_offset in (mem_region.start..mem_region.end).step_by(mem_region.size) {
            unsafe { kuKernelMemDecommit(self.ptr.add(addr_offset as _) as _, mem_region.size as _) };
        }
    }

    pub fn create_page_map(&mut self, shm_mem: &Shm, shm_start: usize, shm_offset: usize, shm_size: usize, addr: usize, page_size: usize, allow_write: bool) -> io::Result<VirtualMemMap> {
        debug_assert_eq!(addr & (page_size - 1), 0);
        let prot = if allow_write { KU_KERNEL_PROT_READ | KU_KERNEL_PROT_WRITE } else { KU_KERNEL_PROT_READ };

        let shm_offset = shm_start + (shm_offset & (shm_size - 1));

        let mut opt = unsafe { mem::zeroed::<KuKernelMemCommitOpt>() };
        opt.size = size_of::<KuKernelMemCommitOpt>() as _;
        opt.attr = KU_KERNEL_MEM_COMMIT_ATTR_HAS_BASE;
        opt.baseBlock = shm_mem.block_uid;
        opt.baseOffset = shm_offset as _;

        if unsafe { kuKernelMemCommit(self.ptr.add(addr) as _, page_size as _, prot, &mut opt) } != 0 {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        } else {
            Ok(VirtualMemMap::new(unsafe { self.ptr.add(addr) as _ }, page_size))
        }
    }

    pub fn set_protection(&mut self, start: usize, size: usize, read: bool, write: bool, exe: bool) {
        unsafe { set_protection(self.ptr.add(start), size, read, write, exe) }
    }

    pub fn create_map(&mut self, shm_mem: &Shm, shm_offset: usize, addr: usize, size: usize, read: bool, write: bool, exe: bool) -> io::Result<VirtualMemMap> {
        let mut prot = KU_KERNEL_PROT_NONE;
        if read {
            prot |= KU_KERNEL_PROT_READ;
        }
        if write {
            prot |= KU_KERNEL_PROT_WRITE;
        }
        if exe {
            prot |= KU_KERNEL_PROT_EXEC;
        }

        let mut opt = unsafe { mem::zeroed::<KuKernelMemCommitOpt>() };
        opt.size = size_of::<KuKernelMemCommitOpt>() as _;
        opt.attr = KU_KERNEL_MEM_COMMIT_ATTR_HAS_BASE;
        opt.baseBlock = shm_mem.block_uid;
        opt.baseOffset = shm_offset as _;

        if unsafe { kuKernelMemCommit(self.ptr.add(addr) as _, size as _, prot, &mut opt) } != 0 {
            Err(Error::from(ErrorKind::AddrNotAvailable))
        } else {
            Ok(VirtualMemMap::new(unsafe { self.ptr.add(addr) as _ }, size))
        }
    }

    pub fn destroy_map(&mut self, start: usize, size: usize) -> i32 {
        unsafe { kuKernelMemDecommit(self.ptr.add(start) as _, size as _) }
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
    let mut prot = KU_KERNEL_PROT_NONE;
    if read {
        prot |= KU_KERNEL_PROT_READ;
    }
    if write {
        prot |= KU_KERNEL_PROT_WRITE;
    }
    if exe {
        prot |= KU_KERNEL_PROT_EXEC;
    }
    kuKernelMemProtect(start as _, size as _, prot);
}

pub unsafe fn flush_icache(start: *const u8, size: usize) {
    kuKernelFlushCaches(start as _, size as _);
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

static mut DELEGATE_FUN: *const fn(usize, &mut usize, &ArmContext) -> bool = ptr::null();
static mut NEXT_ABORT_HANDLER: KuKernelAbortHandler = None;

unsafe extern "C" fn abort_handler(abort_context: *mut KuKernelAbortContext) {
    let context = &mut (*abort_context);

    let delegate_fun: fn(usize, &mut usize, &ArmContext) -> bool = mem::transmute(DELEGATE_FUN);
    let arm_context: &ArmContext = unsafe { mem::transmute(&context.r0) };
    let mut pc = context.pc as usize;
    if delegate_fun(context.FAR as usize, &mut pc, arm_context) {
        context.pc = pc as _;
        return;
    }

    if let Some(next_abort_handler) = NEXT_ABORT_HANDLER {
        next_abort_handler(abort_context);
    } else {
        panic!()
    }
}

pub unsafe fn register_abort_handler(delegate: fn(usize, &mut usize, &ArmContext) -> bool) -> io::Result<()> {
    DELEGATE_FUN = delegate as *const _;
    if kuKernelRegisterAbortHandler(Some(abort_handler), ptr::addr_of_mut!(NEXT_ABORT_HANDLER), ptr::null_mut()) != 0 {
        Err(Error::from(ErrorKind::Other))
    } else {
        Ok(())
    }
}
