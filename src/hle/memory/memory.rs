use crate::hle::memory::handler::Convert;
use crate::hle::memory::regions;
use crate::hle::CpuType;
use crate::mmap::Mmap;
use crate::utils;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::{ptr, slice};

struct SharedWramMap<'a> {
    shared_ptr: *const u8,
    size: usize,
    phantom_data: PhantomData<&'a mut [u8]>,
}

impl SharedWramMap<'_> {
    fn new(shared_ptr: *const u8, size: usize) -> Self {
        SharedWramMap {
            shared_ptr,
            size,
            phantom_data: PhantomData,
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.shared_ptr
    }

    pub fn len(&self) -> usize {
        self.size
    }
}

impl Deref for SharedWramMap<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
}

impl AsRef<[u8]> for SharedWramMap<'_> {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

struct SharedWramMapMut<'a> {
    shared_wram_map: SharedWramMap<'a>,
}

impl SharedWramMapMut<'_> {
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.shared_wram_map.as_ptr() as _
    }
}

impl<'a> From<SharedWramMap<'a>> for SharedWramMapMut<'a> {
    fn from(value: SharedWramMap<'a>) -> Self {
        SharedWramMapMut {
            shared_wram_map: value,
        }
    }
}

impl Deref for SharedWramMapMut<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.shared_wram_map.as_ptr(), self.shared_wram_map.len()) }
    }
}

impl AsRef<[u8]> for SharedWramMapMut<'_> {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl DerefMut for SharedWramMapMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.shared_wram_map.len()) }
    }
}

impl AsMut<[u8]> for SharedWramMapMut<'_> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.deref_mut()
    }
}

struct Wram {
    cnt: u8,
    shared: Box<[u8; regions::SHARED_WRAM_SIZE as usize]>,
    arm7: Box<[u8; regions::ARM7_WRAM_SIZE as usize]>,
    arm9_ptr: *mut u8,
    arm9_size: usize,
    arm7_ptr: *mut u8,
    arm7_size: usize,
}

impl Wram {
    fn new() -> Self {
        let mut instance = Wram {
            cnt: 0,
            shared: Box::new([0u8; regions::SHARED_WRAM_SIZE as usize]),
            arm7: Box::new([0u8; regions::ARM7_WRAM_SIZE as usize]),
            arm9_ptr: ptr::null_mut(),
            arm9_size: 0,
            arm7_ptr: ptr::null_mut(),
            arm7_size: 0,
        };
        instance.set_cnt(0);
        instance
    }

    fn set_cnt(&mut self, value: u8) {
        self.cnt = value & 0x3;
        let shared_len = self.shared.len();

        match self.cnt {
            0 => {
                self.arm9_ptr = self.shared.as_mut_ptr();
                self.arm9_size = shared_len;
                self.arm7_ptr = ptr::null_mut();
                self.arm7_size = 0;
            }
            1 => {
                self.arm9_ptr = self.shared[shared_len / 2..].as_mut_ptr();
                self.arm9_size = shared_len / 2;
                self.arm7_ptr = self.shared.as_mut_ptr();
                self.arm7_size = shared_len / 2;
            }
            2 => {
                self.arm9_ptr = self.shared.as_mut_ptr();
                self.arm9_size = shared_len / 2;
                self.arm7_ptr = self.shared[shared_len / 2..].as_mut_ptr();
                self.arm7_size = shared_len / 2;
            }
            3 => {
                self.arm9_ptr = ptr::null_mut();
                self.arm9_size = 0;
                self.arm7_ptr = self.shared.as_mut_ptr();
                self.arm7_size = shared_len;
            }
            _ => panic!(),
        }
    }

    fn get_map_arm9(&self) -> SharedWramMap {
        SharedWramMap::new(self.arm9_ptr, self.arm9_size)
    }

    fn get_map_arm9_mut(&mut self) -> SharedWramMapMut {
        SharedWramMapMut::from(self.get_map_arm9())
    }

    fn get_map_arm7(&self, addr: u32) -> SharedWramMap {
        if addr & regions::ARM7_WRAM_OFFSET != 0 {
            SharedWramMap::new(self.arm7.as_ptr(), self.arm7.len())
        } else {
            SharedWramMap::new(self.arm7_ptr, self.arm7_size)
        }
    }

    fn get_map_arm7_mut(&mut self, addr: u32) -> SharedWramMapMut {
        SharedWramMapMut::from(self.get_map_arm7(addr))
    }
}

unsafe impl Send for Wram {}
unsafe impl Sync for Wram {}

pub struct Memory {
    main: Mmap,
    wram: Wram,
}

impl Memory {
    pub fn new() -> Self {
        Memory {
            main: Mmap::rw("main", regions::MAIN_MEMORY_ADDRESS_SPACE).unwrap(),
            wram: Wram::new(),
        }
    }

    pub fn set_wram_cnt(&mut self, value: u8) {
        self.wram.set_cnt(value)
    }

    pub fn read_wram_slice<T: Convert>(
        &self,
        cpu_type: CpuType,
        addr_offset: u32,
        slice: &mut [T],
    ) {
        let mem = match cpu_type {
            CpuType::ARM9 => self.wram.get_map_arm9(),
            CpuType::ARM7 => self.wram.get_map_arm7(addr_offset),
        };
        utils::read_from_mem_slice(&mem, addr_offset & (mem.len() - 1) as u32, slice);
    }

    pub fn write_wram_slice<T: Convert>(
        &mut self,
        cpu_type: CpuType,
        addr_offset: u32,
        slice: &[T],
    ) {
        let mut mem = match cpu_type {
            CpuType::ARM9 => self.wram.get_map_arm9_mut(),
            CpuType::ARM7 => self.wram.get_map_arm7_mut(addr_offset),
        };
        let mem_size = mem.len();
        utils::write_to_mem_slice(&mut mem, addr_offset & (mem_size - 1) as u32, slice);
    }

    pub fn read_main<T: Convert>(&self, addr_offset: u32) -> T {
        utils::read_from_mem(&self.main, addr_offset & (regions::MAIN_MEMORY_OFFSET - 1))
    }

    pub fn read_main_slice<T: Convert>(&self, addr_offset: u32, slice: &mut [T]) {
        utils::read_from_mem_slice(
            &self.main,
            addr_offset & (regions::MAIN_MEMORY_OFFSET - 1),
            slice,
        )
    }

    pub fn write_main<T: Convert>(&mut self, addr_offset: u32, value: T) {
        utils::write_to_mem(
            &mut self.main,
            addr_offset & (regions::MAIN_MEMORY_OFFSET - 1),
            value,
        )
    }

    pub fn write_main_slice<T: Convert>(&mut self, addr_offset: u32, slice: &[T]) {
        utils::write_to_mem_slice(
            &mut self.main,
            addr_offset & (regions::MAIN_MEMORY_OFFSET - 1),
            slice,
        )
    }
}
