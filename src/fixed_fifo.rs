use crate::utils::HeapMem;
use std::fmt::{Debug, Formatter};

pub struct FixedFifo<T, const SIZE: usize> {
    fifo: HeapMem<T, SIZE>,
    len: usize,
    start: usize,
    end: usize,
}

impl<T, const SIZE: usize> FixedFifo<T, SIZE> {
    pub fn new() -> Self {
        FixedFifo {
            fifo: unsafe { HeapMem::zeroed() },
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
        FixedFifo {
            fifo: HeapMem::default(),
            len: 0,
            start: 0,
            end: 0,
        }
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
