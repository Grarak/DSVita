use crate::utils::HeapMem;

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

    pub unsafe fn push_back_unchecked(&mut self, value: T) {
        *self.fifo.get_unchecked_mut(self.end) = value;
        self.end = (self.end + 1) % SIZE;
        self.len += 1;
    }

    pub unsafe fn front_unchecked(&self) -> &T {
        self.fifo.get_unchecked(self.start)
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
