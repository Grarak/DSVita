use crate::hle::memory::regions;
use crate::hle::CpuType;
use crate::utils;
use crate::utils::{Convert, HeapMemU8};
use std::cell::RefCell;
use std::hint::unreachable_unchecked;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::{ptr, slice};

struct SharedWramMap<'a> {
    shared_ptr: *const u8,
    size: usize,
    phantom_data: PhantomData<&'a u8>,
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
    shared_ptr: *mut u8,
    size: usize,
    phantom_data: PhantomData<&'a mut u8>,
}

impl SharedWramMapMut<'_> {
    fn new(shared_ptr: *mut u8, size: usize) -> Self {
        SharedWramMapMut {
            shared_ptr,
            size,
            phantom_data: PhantomData,
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.shared_ptr
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.shared_ptr
    }

    pub fn len(&self) -> usize {
        self.size
    }
}

impl Deref for SharedWramMapMut<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
}

impl AsRef<[u8]> for SharedWramMapMut<'_> {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl DerefMut for SharedWramMapMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) }
    }
}

impl AsMut<[u8]> for SharedWramMapMut<'_> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.deref_mut()
    }
}

struct SharedWram {
    cnt: u8,
    mem: HeapMemU8<{ regions::SHARED_WRAM_SIZE as usize }>,
    arm9_ptr: *mut u8,
    arm9_size: usize,
    arm7_ptr: *mut u8,
    arm7_size: usize,
}

impl SharedWram {
    fn new() -> Self {
        let mut instance = SharedWram {
            cnt: 0,
            mem: HeapMemU8::new(),
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
        const SHARED_LEN: usize = regions::SHARED_WRAM_SIZE as usize;

        match self.cnt {
            0 => {
                self.arm9_ptr = self.mem.as_mut_ptr();
                self.arm9_size = SHARED_LEN;
                self.arm7_ptr = ptr::null_mut();
                self.arm7_size = 0;
            }
            1 => {
                self.arm9_ptr = self.mem[SHARED_LEN / 2..].as_mut_ptr();
                self.arm9_size = SHARED_LEN / 2;
                self.arm7_ptr = self.mem.as_mut_ptr();
                self.arm7_size = SHARED_LEN / 2;
            }
            2 => {
                self.arm9_ptr = self.mem.as_mut_ptr();
                self.arm9_size = SHARED_LEN / 2;
                self.arm7_ptr = self.mem[SHARED_LEN / 2..].as_mut_ptr();
                self.arm7_size = SHARED_LEN / 2;
            }
            3 => {
                self.arm9_ptr = ptr::null_mut();
                self.arm9_size = 0;
                self.arm7_ptr = self.mem.as_mut_ptr();
                self.arm7_size = SHARED_LEN;
            }
            _ => {
                unsafe { unreachable_unchecked() };
            }
        };
    }

    fn get_map_arm9(&self) -> SharedWramMap {
        SharedWramMap::new(self.arm9_ptr, self.arm9_size)
    }

    fn get_map_arm9_mut(&mut self) -> SharedWramMapMut {
        SharedWramMapMut::new(self.arm9_ptr, self.arm9_size)
    }

    fn get_map_arm7(&self) -> SharedWramMap {
        SharedWramMap::new(self.arm7_ptr, self.arm7_size)
    }

    fn get_map_arm7_mut(&mut self) -> SharedWramMapMut {
        SharedWramMapMut::new(self.arm7_ptr, self.arm7_size)
    }

    fn read_arm9<T: Convert>(&self, addr_offset: u32) -> T {
        let mem = self.get_map_arm9();
        utils::read_from_mem(&mem, addr_offset & (mem.len() - 1) as u32)
    }

    fn read_arm7<T: Convert>(&self, addr_offset: u32) -> T {
        let mem = self.get_map_arm7();
        utils::read_from_mem(&mem, addr_offset & (mem.len() - 1) as u32)
    }

    fn write_arm9<T: Convert>(&mut self, addr_offset: u32, value: T) {
        let mut mem = self.get_map_arm9_mut();
        let mem_len = mem.len();
        utils::write_to_mem(&mut mem, addr_offset & (mem_len - 1) as u32, value);
    }

    fn write_arm7<T: Convert>(&mut self, addr_offset: u32, value: T) {
        let mut mem = self.get_map_arm7_mut();
        let mem_len = mem.len();
        utils::write_to_mem(&mut mem, addr_offset & (mem_len - 1) as u32, value);
    }
}

pub struct WramContext {
    wram_arm7: RefCell<HeapMemU8<{ regions::ARM7_WRAM_SIZE as usize }>>,
    shared: SharedWram,
}

impl WramContext {
    pub fn new() -> Self {
        WramContext {
            wram_arm7: RefCell::new(HeapMemU8::new()),
            shared: SharedWram::new(),
        }
    }

    pub fn get_cnt(&self) -> u8 {
        self.shared.cnt
    }

    pub fn set_cnt(&mut self, value: u8) {
        self.shared.set_cnt(value);
    }

    pub fn read<const CPU: CpuType, T: Convert>(&self, addr_offset: u32) -> T {
        match CPU {
            CpuType::ARM9 => self.shared.read_arm9(addr_offset),
            CpuType::ARM7 => {
                if self.shared.cnt == 0 || addr_offset & regions::ARM7_WRAM_OFFSET != 0 {
                    utils::read_from_mem(
                        self.wram_arm7.borrow().as_slice(),
                        addr_offset & (regions::ARM7_WRAM_SIZE - 1),
                    )
                } else {
                    self.shared.read_arm7(addr_offset)
                }
            }
        }
    }

    pub fn write<const CPU: CpuType, T: Convert>(&mut self, addr_offset: u32, value: T) {
        match CPU {
            CpuType::ARM9 => self.shared.write_arm9(addr_offset, value),
            CpuType::ARM7 => {
                if self.shared.cnt == 0 || addr_offset & regions::ARM7_WRAM_OFFSET != 0 {
                    utils::write_to_mem(
                        self.wram_arm7.borrow_mut().as_mut_slice(),
                        addr_offset & (regions::ARM7_WRAM_SIZE - 1),
                        value,
                    )
                } else {
                    self.shared.write_arm7(addr_offset, value)
                }
            }
        }
    }
}
