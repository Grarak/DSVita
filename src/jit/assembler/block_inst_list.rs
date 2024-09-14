use std::alloc::{GlobalAlloc, Layout, System};
use std::marker::PhantomData;
use std::ptr;

pub struct BlockInstListEntry {
    pub value: usize,
    pub previous: *mut BlockInstListEntry,
    pub next: *mut BlockInstListEntry,
}

impl BlockInstListEntry {
    fn alloc(value: usize) -> *mut Self {
        unsafe {
            let entry = System.alloc(Layout::new::<BlockInstListEntry>()) as *mut BlockInstListEntry;
            (*entry).previous = ptr::null_mut();
            (*entry).value = value;
            (*entry).next = ptr::null_mut();
            entry
        }
    }

    fn dealloc(entry: *mut Self) {
        unsafe { System.dealloc(entry as _, Layout::new::<BlockInstListEntry>()) }
    }

    fn insert_begin(&mut self, new_next: *mut BlockInstListEntry) {
        if !self.previous.is_null() {
            unsafe {
                (*self.previous).next = new_next;
                (*new_next).previous = self.previous;
            }
        }
        self.previous = new_next;
        unsafe { (*new_next).next = self };
    }

    fn insert_end(&mut self, new_next: *mut BlockInstListEntry) {
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

pub struct BlockInstList {
    pub root: *mut BlockInstListEntry,
    pub end: *mut BlockInstListEntry,
    size: usize,
}

impl BlockInstList {
    pub fn new() -> Self {
        BlockInstList {
            root: ptr::null_mut(),
            end: ptr::null_mut(),
            size: 0,
        }
    }

    pub fn insert_begin(&mut self, value: usize) -> *mut BlockInstListEntry {
        let new_entry = BlockInstListEntry::alloc(value);
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

    pub fn insert_entry_begin(&mut self, entry: *mut BlockInstListEntry, value: usize) -> *mut BlockInstListEntry {
        let new_entry = BlockInstListEntry::alloc(value);
        self.size += 1;
        unsafe { (*entry).insert_begin(new_entry) };

        if self.root == entry {
            self.root = new_entry;
        }

        unsafe { new_entry.as_mut().unwrap_unchecked() }
    }

    pub fn insert_entry_end(&mut self, entry: *mut BlockInstListEntry, value: usize) -> *mut BlockInstListEntry {
        let new_entry = BlockInstListEntry::alloc(value);
        self.size += 1;
        unsafe { (*entry).insert_end(new_entry) };

        if self.end == entry {
            self.end = new_entry;
        }

        unsafe { new_entry.as_mut().unwrap_unchecked() }
    }

    pub fn insert_end(&mut self, value: usize) -> *mut BlockInstListEntry {
        let new_entry = BlockInstListEntry::alloc(value);
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

    pub fn remove_entry(&mut self, entry: *mut BlockInstListEntry) {
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
        BlockInstListEntry::dealloc(entry);
    }

    pub fn iter(&self) -> BlockIntListIter {
        BlockIntListIter {
            entry: self.root,
            size: self.size,
            phantom_data: PhantomData,
        }
    }

    pub fn iter_rev(&self) -> BlockIntListRevIter {
        BlockIntListRevIter {
            entry: self.end,
            size: self.size,
            phantom_data: PhantomData,
        }
    }

    pub fn deref(entry: *mut BlockInstListEntry) -> &'static mut BlockInstListEntry {
        unsafe { entry.as_mut().unwrap() }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Drop for BlockInstList {
    fn drop(&mut self) {
        let mut current = self.root;
        let mut freed_count = 0;
        while !current.is_null() {
            let next = unsafe { (*current).next };
            BlockInstListEntry::dealloc(current);
            current = next;
            freed_count += 1;
        }
        assert_eq!(self.size, freed_count);
    }
}

pub struct BlockIntListIter<'a> {
    entry: *mut BlockInstListEntry,
    size: usize,
    phantom_data: PhantomData<&'a BlockInstListEntry>,
}

impl<'a> Iterator for BlockIntListIter<'a> {
    type Item = &'a BlockInstListEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.entry.is_null() {
            None
        } else {
            let entry = unsafe { self.entry.as_ref() };
            self.entry = entry?.next;
            entry
        }
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.size
    }
}

pub struct BlockIntListRevIter<'a> {
    entry: *mut BlockInstListEntry,
    size: usize,
    phantom_data: PhantomData<&'a BlockInstListEntry>,
}

impl<'a> Iterator for BlockIntListRevIter<'a> {
    type Item = &'a BlockInstListEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.entry.is_null() {
            None
        } else {
            let entry = unsafe { self.entry.as_ref() };
            self.entry = entry?.previous;
            entry
        }
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.size
    }
}
