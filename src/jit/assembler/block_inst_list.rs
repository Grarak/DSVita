use std::alloc::{GlobalAlloc, Layout, System};
use std::intrinsics::unlikely;
use std::marker::PhantomData;
use std::{mem, ptr};

static mut USABLE_ENTRIES: Vec<*mut BlockInstListEntry> = Vec::new();
static mut USABLE_ENTRIES_INDEX: usize = 0;

pub unsafe fn reset_inst_list_entries() {
    USABLE_ENTRIES_INDEX = 0;
}

#[derive(Copy, Clone)]
pub struct BlockInstListEntry {
    pub value: usize,
    pub previous: *mut BlockInstListEntry,
    pub next: *mut BlockInstListEntry,
}

impl Default for BlockInstListEntry {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

impl BlockInstListEntry {
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

    fn new_entry(&mut self, value: usize) -> *mut BlockInstListEntry {
        unsafe {
            if USABLE_ENTRIES_INDEX == USABLE_ENTRIES.len() {
                let entry = System.alloc(Layout::new::<BlockInstListEntry>()) as *mut BlockInstListEntry;
                entry.write(BlockInstListEntry::default());
                USABLE_ENTRIES.push(entry);
            }
            let entry = USABLE_ENTRIES[USABLE_ENTRIES_INDEX];
            (*entry).value = value;
            (*entry).previous = ptr::null_mut();
            (*entry).next = ptr::null_mut();
            USABLE_ENTRIES_INDEX += 1;
            entry
        }
    }

    pub fn insert_begin(&mut self, value: usize) -> *mut BlockInstListEntry {
        let new_entry = self.new_entry(value);
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
        let new_entry = self.new_entry(value);
        self.size += 1;
        unsafe { (*entry).insert_begin(new_entry) };

        if self.root == entry {
            self.root = new_entry;
        }

        unsafe { new_entry.as_mut().unwrap_unchecked() }
    }

    pub fn insert_entry_end(&mut self, entry: *mut BlockInstListEntry, value: usize) -> *mut BlockInstListEntry {
        let new_entry = self.new_entry(value);

        self.size += 1;
        unsafe { (*entry).insert_end(new_entry) };

        if self.end == entry {
            self.end = new_entry;
        }

        unsafe { new_entry.as_mut().unwrap_unchecked() }
    }

    pub fn insert_end(&mut self, value: usize) -> *mut BlockInstListEntry {
        let new_entry = self.new_entry(value);
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

pub struct BlockIntListIter<'a> {
    entry: *mut BlockInstListEntry,
    size: usize,
    phantom_data: PhantomData<&'a BlockInstListEntry>,
}

impl<'a> Iterator for BlockIntListIter<'a> {
    type Item = &'a BlockInstListEntry;

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

pub struct BlockIntListRevIter<'a> {
    entry: *mut BlockInstListEntry,
    size: usize,
    phantom_data: PhantomData<&'a BlockInstListEntry>,
}

impl<'a> Iterator for BlockIntListRevIter<'a> {
    type Item = &'a BlockInstListEntry;

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
