use std::intrinsics::unlikely;
use std::marker::PhantomData;
use std::ptr;

pub trait LinkedListAllocator<T> {
    fn allocate(&mut self, value: T) -> *mut LinkedListEntry<T>;
    fn deallocate(&mut self, entry: *mut LinkedListEntry<T>);
}

pub struct LinkedListEntry<T> {
    pub value: T,
    pub previous: *mut LinkedListEntry<T>,
    pub next: *mut LinkedListEntry<T>,
}

impl<T> LinkedListEntry<T> {
    fn insert_begin(&mut self, new_next: *mut LinkedListEntry<T>) {
        if !self.previous.is_null() {
            unsafe {
                (*self.previous).next = new_next;
                (*new_next).previous = self.previous;
            }
        }
        self.previous = new_next;
        unsafe { (*new_next).next = self };
    }

    fn insert_end(&mut self, new_next: *mut LinkedListEntry<T>) {
        if !self.next.is_null() {
            unsafe {
                (*self.next).previous = new_next;
                (*new_next).next = self.next;
            }
        }
        self.next = new_next;
        unsafe { (*new_next).previous = self };
    }

    fn remove(&mut self) {
        if !self.previous.is_null() {
            unsafe {
                (*self.previous).next = self.next;
            }
        }
        if !self.next.is_null() {
            unsafe {
                (*self.next).previous = self.previous;
            }
        }
    }
}

pub struct LinkedList<T, A: LinkedListAllocator<T>> {
    pub root: *mut LinkedListEntry<T>,
    pub end: *mut LinkedListEntry<T>,
    allocator: A,
    size: usize,
}

impl<T, A: Default + LinkedListAllocator<T>> LinkedList<T, A> {
    pub fn new() -> Self {
        LinkedList {
            root: ptr::null_mut(),
            end: ptr::null_mut(),
            allocator: A::default(),
            size: 0,
        }
    }

    pub fn insert_begin(&mut self, value: T) -> *mut LinkedListEntry<T> {
        let new_entry = self.allocator.allocate(value);
        self.size += 1;
        if self.root.is_null() && self.end.is_null() {
            self.root = new_entry;
            self.end = new_entry;
            return unsafe { new_entry.as_mut().unwrap_unchecked() };
        }

        unsafe {
            (*self.root).previous = new_entry;
            (*new_entry).next = self.root;
        }
        self.root = new_entry;
        new_entry
    }

    pub fn insert_entry_begin(&mut self, entry: *mut LinkedListEntry<T>, value: T) -> *mut LinkedListEntry<T> {
        let new_entry = self.allocator.allocate(value);
        self.size += 1;
        unsafe { (*entry).insert_begin(new_entry) };

        if self.root == entry {
            self.root = new_entry;
        }

        unsafe { new_entry.as_mut().unwrap_unchecked() }
    }

    pub fn insert_entry_end(&mut self, entry: *mut LinkedListEntry<T>, value: T) -> *mut LinkedListEntry<T> {
        let new_entry = self.allocator.allocate(value);

        self.size += 1;
        unsafe { (*entry).insert_end(new_entry) };

        if self.end == entry {
            self.end = new_entry;
        }

        unsafe { new_entry.as_mut().unwrap_unchecked() }
    }

    pub fn insert_end(&mut self, value: T) -> *mut LinkedListEntry<T> {
        let new_entry = self.allocator.allocate(value);
        self.size += 1;
        if self.root.is_null() && self.end.is_null() {
            self.root = new_entry;
            self.end = new_entry;
            return unsafe { new_entry.as_mut().unwrap_unchecked() };
        }

        unsafe {
            (*self.end).next = new_entry;
            (*new_entry).previous = self.end;
        }
        self.end = new_entry;
        new_entry
    }

    #[inline]
    pub fn remove_begin(&mut self) -> T {
        self.size -= 1;
        let root = self.root;
        let ret = unsafe {
            self.root = (*root).next;
            if !self.root.is_null() {
                (*self.root).previous = ptr::null_mut();
            }
            ptr::addr_of!((*root).value).read()
        };
        if self.size == 0 {
            self.end = ptr::null_mut();
        }
        self.allocator.deallocate(root);
        ret
    }

    pub fn remove_entry(&mut self, entry: *mut LinkedListEntry<T>) {
        self.size -= 1;
        unsafe {
            if self.root == entry {
                self.root = (*entry).next;
            }
            if self.end == entry {
                self.end = (*entry).previous;
            }
            (*entry).remove();
        }
        self.allocator.deallocate(entry);
    }

    pub fn iter(&self) -> LinkedListIter<T> {
        LinkedListIter {
            entry: self.root,
            size: self.size,
            phantom_data: PhantomData,
        }
    }

    pub fn iter_rev(&self) -> LinkedListListRevIter<T> {
        LinkedListListRevIter {
            entry: self.end,
            size: self.size,
            phantom_data: PhantomData,
        }
    }

    pub fn deref(entry: *mut LinkedListEntry<T>) -> &'static mut LinkedListEntry<T> {
        unsafe { entry.as_mut_unchecked() }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T, A: LinkedListAllocator<T>> Drop for LinkedList<T, A> {
    fn drop(&mut self) {
        let mut current = self.root;
        let mut freed_count = 0;
        while !current.is_null() {
            let next = unsafe { (*current).next };
            self.allocator.deallocate(current);
            current = next;
            freed_count += 1;
        }
        assert_eq!(self.size, freed_count);
    }
}

pub struct LinkedListIter<'a, T> {
    entry: *mut LinkedListEntry<T>,
    size: usize,
    phantom_data: PhantomData<&'a LinkedListEntry<T>>,
}

impl<'a, T> Iterator for LinkedListIter<'a, T> {
    type Item = &'a LinkedListEntry<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if unlikely(self.entry.is_null()) {
            None
        } else {
            let entry = unsafe { self.entry.as_ref_unchecked() };
            self.entry = entry.next;
            Some(entry)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.size, Some(self.size))
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.size
    }
}

pub struct LinkedListListRevIter<'a, T> {
    entry: *mut LinkedListEntry<T>,
    size: usize,
    phantom_data: PhantomData<&'a LinkedListEntry<T>>,
}

impl<'a, T> Iterator for LinkedListListRevIter<'a, T> {
    type Item = &'a LinkedListEntry<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if unlikely(self.entry.is_null()) {
            None
        } else {
            let entry = unsafe { self.entry.as_ref_unchecked() };
            self.entry = entry.previous;
            Some(entry)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.size, Some(self.size))
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.size
    }
}
