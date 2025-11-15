use std::{
    fmt::{Debug, Formatter},
    mem,
};

pub struct FixedFifo<T, const SIZE: u16>
where
    [(); SIZE as usize]:,
{
    start: u16,
    len: u16,
    end: u16,
    fifo: [T; SIZE as usize],
}

impl<T, const SIZE: u16> FixedFifo<T, SIZE>
where
    [(); SIZE as usize]:,
{
    pub fn new() -> Self {
        FixedFifo {
            fifo: unsafe { mem::zeroed() },
            len: 0,
            start: 0,
            end: 0,
        }
    }

    pub fn len(&self) -> u16 {
        self.len
    }

    pub fn push_front(&mut self, value: T) {
        self.start = self.start.wrapping_sub(1) % SIZE;
        unsafe { *self.fifo.get_unchecked_mut(self.start as usize) = value };
        self.len += 1;
        debug_assert!(self.len <= SIZE);
    }

    pub fn push_back(&mut self, value: T) {
        unsafe { *self.fifo.get_unchecked_mut(self.end as usize) = value };
        self.end = (self.end + 1) % SIZE;
        self.len += 1;
        debug_assert!(self.len <= SIZE);
    }

    pub fn front(&self) -> &T {
        unsafe { self.fifo.get_unchecked(self.start as usize) }
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

impl<T: Copy, const SIZE: u16> FixedFifo<T, SIZE>
where
    [(); SIZE as usize]:,
{
    pub fn push_back_multiple(&mut self, values: &[T]) {
        let end = self.end;
        self.end = (end + values.len() as u16) % SIZE;
        self.len += values.len() as u16;
        for i in 0..values.len() {
            unsafe { *self.fifo.get_unchecked_mut(((end + i as u16) % SIZE) as usize) = values[i] };
        }
    }
}

impl<T: Default, const SIZE: u16> Default for FixedFifo<T, SIZE>
where
    [(); SIZE as usize]:,
{
    fn default() -> Self {
        FixedFifo::new()
    }
}

impl<T: Debug, const SIZE: u16> Debug for FixedFifo<T, SIZE>
where
    [(); SIZE as usize]:,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_list();
        for i in 0..self.len() {
            let start = (self.start.wrapping_add(i)) % SIZE;
            list.entry(&self.fifo[start as usize]);
        }
        list.finish()
    }
}
