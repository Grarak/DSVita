use crate::mmap::{Shm, VirtualMem, PAGE_SIZE};
use std::marker::PhantomData;
use std::ops::Index;
use std::{
    fmt::{Debug, Formatter},
    mem,
};

pub struct FastFixedFifo<T, const SIZE: u16> {
    start: u16,
    len: u16,
    end: u16,
    _shm: Shm,
    vmem: VirtualMem,
    _type: PhantomData<T>,
}

impl<T, const SIZE: u16> FastFixedFifo<T, SIZE> {
    pub fn new() -> Self {
        let total_size = size_of::<T>() * SIZE as usize;
        debug_assert!(total_size > PAGE_SIZE);
        debug_assert_eq!(total_size % SIZE as usize, 0);

        let mut vmem = VirtualMem::new(total_size * 2, 0).unwrap();
        let shm = Shm::new("fast_fixed_fifo", total_size).unwrap();
        for addr in (0..total_size).step_by(PAGE_SIZE) {
            vmem.create_map(&shm, addr, addr, PAGE_SIZE, true, true, false).unwrap();
            vmem.create_map(&shm, addr, addr + total_size, PAGE_SIZE, true, true, false).unwrap();
        }
        FastFixedFifo {
            len: 0,
            start: 0,
            end: 0,
            _shm: shm,
            vmem,
            _type: PhantomData,
        }
    }

    pub fn len(&self) -> u16 {
        self.len
    }

    #[inline(always)]
    pub fn push_front(&mut self, value: T) {
        self.start = self.start.wrapping_sub(1) % SIZE;
        unsafe { (self.vmem.as_mut_ptr() as *mut T).add(self.start as usize).write(value) };
        self.len += 1;
        debug_assert!(self.len <= SIZE);
    }

    #[inline(always)]
    pub fn push_back(&mut self, value: T) {
        unsafe { (self.vmem.as_mut_ptr() as *mut T).add(self.end as usize).write(value) };
        self.end = (self.end + 1) % SIZE;
        self.len += 1;
        debug_assert!(self.len <= SIZE);
    }

    pub fn front(&self) -> &T {
        unsafe { mem::transmute((self.vmem.as_ptr() as *const T).add(self.start as usize)) }
    }

    pub fn front_ptr(&self) -> *const T {
        unsafe { (self.vmem.as_ptr() as *const T).add(self.start as usize) }
    }

    pub fn pop_front(&mut self) {
        self.start = (self.start + 1) % SIZE;
        self.len -= 1;
    }

    pub fn pop_front_multiple(&mut self, count: u16) {
        self.start = (self.start + count) % SIZE;
        self.len -= count;
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_full(&self) -> bool {
        self.len() == SIZE
    }

    pub fn clear(&mut self) {
        self.len = 0;
        self.start = 0;
        self.end = 0;
    }

    pub fn pos_front(&self) -> u16 {
        self.start
    }

    pub fn pos_end(&self) -> u16 {
        self.end
    }
}

impl<T: Copy, const SIZE: u16> FastFixedFifo<T, SIZE> {
    #[inline(always)]
    pub fn push_back_multiple<const MEMCPY: bool>(&mut self, values: &[T]) {
        let end = self.end;
        self.end = (end + values.len() as u16) % SIZE;
        self.len += values.len() as u16;
        debug_assert!(self.len <= SIZE);
        unsafe {
            let ptr = (self.vmem.as_mut_ptr() as *mut T).add(end as usize);
            if MEMCPY {
                ptr.copy_from_nonoverlapping(values.as_ptr(), values.len());
            } else {
                for i in 0..values.len() {
                    ptr.add(i).write(values[i]);
                }
            }
        }
    }
}

impl<T: Default, const SIZE: u16> Default for FastFixedFifo<T, SIZE> {
    fn default() -> Self {
        FastFixedFifo::new()
    }
}

impl<T: Debug, const SIZE: u16> Debug for FastFixedFifo<T, SIZE> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_list();
        for i in 0..self.len() {
            list.entry(&self[i]);
        }
        list.finish()
    }
}

impl<T, const SIZE: u16> Index<u16> for FastFixedFifo<T, SIZE> {
    type Output = T;

    fn index(&self, index: u16) -> &Self::Output {
        let index = (index + self.start) % SIZE;
        unsafe { mem::transmute((self.vmem.as_ptr() as *const T).add(index as usize)) }
    }
}
