use std::fmt::{Debug, Formatter};
use std::mem;

#[repr(C)]
pub struct FixedFifo<T, const SIZE: usize> {
    start: usize,
    len: usize,
    end: usize,
    fifo: [T; SIZE],
}

impl<T, const SIZE: usize> FixedFifo<T, SIZE> {
    pub fn new() -> Self {
        FixedFifo {
            fifo: unsafe { mem::zeroed() },
            len: 0,
            start: 0,
            end: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn push_front(&mut self, value: T) {
        self.start = self.start.wrapping_sub(1) % SIZE;
        unsafe { *self.fifo.get_unchecked_mut(self.start) = value };
        self.len += 1;
    }

    pub fn push_back(&mut self, value: T) {
        unsafe { *self.fifo.get_unchecked_mut(self.end) = value };
        self.end = (self.end + 1) % SIZE;
        self.len += 1;
    }

    pub fn front(&self) -> &T {
        unsafe { self.fifo.get_unchecked(self.start) }
    }

    pub fn pop_front(&mut self) {
        self.start = (self.start + 1) % SIZE;
        self.len -= 1;
    }

    pub fn pop_front_multiple(&mut self, count: usize) {
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
}

impl<T: Default, const SIZE: usize> Default for FixedFifo<T, SIZE> {
    fn default() -> Self {
        FixedFifo::new()
    }
}

impl<T: Debug, const SIZE: usize> Debug for FixedFifo<T, SIZE> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_list();
        for i in 0..self.len() {
            let start = (self.start.wrapping_add(i)) % SIZE;
            list.entry(&self.fifo[start]);
        }
        list.finish()
    }
}
