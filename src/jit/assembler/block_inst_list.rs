use crate::linked_list::{LinkedList, LinkedListAllocator, LinkedListEntry};
use std::alloc::{GlobalAlloc, Layout, System};
use std::ptr;

static mut USABLE_ENTRIES: Vec<*mut BlockInstListEntry> = Vec::new();
static mut USABLE_ENTRIES_INDEX: usize = 0;

pub unsafe fn reset_inst_list_entries() {
    USABLE_ENTRIES_INDEX = 0;
}

#[derive(Default)]
pub struct BlockInstListEntryAllocator;

impl LinkedListAllocator<usize> for BlockInstListEntryAllocator {
    fn allocate(&mut self, value: usize) -> *mut LinkedListEntry<usize> {
        unsafe {
            if USABLE_ENTRIES_INDEX == USABLE_ENTRIES.len() {
                let entry = System.alloc(Layout::new::<BlockInstListEntry>()) as *mut BlockInstListEntry;
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

    fn deallocate(&mut self, _: *mut LinkedListEntry<usize>) {}
}

pub type BlockInstListEntry = LinkedListEntry<usize>;
pub type BlockInstList = LinkedList<usize, BlockInstListEntryAllocator>;
