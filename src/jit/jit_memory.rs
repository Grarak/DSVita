use crate::mmap::Mmap;
use crate::utils::{align_up, write_to_mem};
use std::mem;

pub struct JitMemory {
    pub memory: Mmap,
    pub ptr: u32,
    is_open: bool,
}

impl JitMemory {
    pub fn new() -> Self {
        JitMemory {
            memory: Mmap::new("code", true, 8 * 1024 * 1024).unwrap(),
            ptr: 0,
            is_open: false,
        }
    }

    pub fn align_up(&mut self) {
        let current_addr = self.memory.as_ptr() as u32 + self.ptr;
        let aligned_addr = align_up(current_addr, 16);
        self.ptr += aligned_addr - current_addr;
    }

    pub fn write<T: Into<u32>>(&mut self, value: T) {
        debug_assert!(self.is_open);
        write_to_mem(&mut self.memory, self.ptr, value);
        self.ptr += mem::size_of::<T>() as u32;
    }

    pub fn write_array<T: Into<u32>>(&mut self, value: &[T]) {
        debug_assert!(self.is_open);
        let (_, aligned_value, _) = unsafe { value.align_to::<u8>() };
        self.memory[self.ptr as usize..self.ptr as usize + aligned_value.len()]
            .copy_from_slice(aligned_value);
        self.ptr += (mem::size_of::<T>() * value.len()) as u32;
    }

    #[cfg(target_os = "linux")]
    pub fn open(&mut self) -> u32 {
        self.is_open = true;
        self.ptr
    }

    #[cfg(target_os = "linux")]
    pub fn close(&mut self) -> u32 {
        self.is_open = false;
        self.ptr
    }

    #[cfg(target_os = "linux")]
    pub fn flush_cache(&self, _: u32, _: u32) {}

    #[cfg(target_os = "vita")]
    pub fn open(&mut self) -> u32 {
        let ret = unsafe { vitasdk_sys::sceKernelOpenVMDomain() };
        if ret < vitasdk_sys::SCE_OK as _ {
            panic!("Can't open vm domain {}", ret);
        }
        self.is_open = true;
        self.ptr
    }

    #[cfg(target_os = "vita")]
    pub fn close(&mut self) -> u32 {
        let ret = unsafe { vitasdk_sys::sceKernelCloseVMDomain() };
        if ret < vitasdk_sys::SCE_OK as _ {
            panic!("Can't close vm domain {}", ret);
        }
        self.is_open = false;
        self.ptr
    }

    #[cfg(target_os = "vita")]
    pub fn flush_cache(&self, begin: u32, end: u32) {
        let ret = unsafe {
            vitasdk_sys::sceKernelSyncVMDomain(
                self.memory.block_uid,
                (self.memory.as_ptr() as u32 + begin) as _,
                end - begin,
            )
        };
        if ret < vitasdk_sys::SCE_OK as _ {
            panic!("Can't sync vm domain {}", ret)
        }
    }
}
