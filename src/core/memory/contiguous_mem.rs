use crate::core::memory::{regions, vram};
use crate::mmap::MmapAllocator;
use crate::utils::HeapMemU8;

const REGIONS: [(u32, u32); 5] = [
    (regions::INSTRUCTION_TCM_OFFSET, regions::INSTRUCTION_TCM_SIZE),
    (regions::MAIN_MEMORY_OFFSET, regions::MAIN_MEMORY_SIZE),
    (regions::SHARED_WRAM_OFFSET, regions::SHARED_WRAM_SIZE),
    (regions::ARM7_WRAM_OFFSET, regions::ARM7_WRAM_SIZE),
    (regions::VRAM_OFFSET, vram::TOTAL_SIZE as u32),
];

const SIZE_OFFSETS: [usize; REGIONS.len()] = {
    let mut offsets = [0; REGIONS.len()];
    let mut i = 0;
    let mut size = 0;
    while i < REGIONS.len() {
        offsets[i] = size;
        size += REGIONS[i].1 as usize;
        i += 1;
    }
    offsets
};

pub const TOTAL_SIZE: u32 = {
    let mut size = 0;
    let mut i = 0;
    while i < REGIONS.len() {
        size += REGIONS[i].1;
        i += 1;
    }
    size
};

const PAGE_SIZE_SHIFT: u32 = 14;
const PAGE_SIZE: u32 = 1 << 14;

const GUEST_PAGE_LOOKUP: [(u32, u32); (TOTAL_SIZE / PAGE_SIZE) as usize] = {
    let mut pages = [(0, 0); (TOTAL_SIZE / PAGE_SIZE) as usize];
    let mut i = 0;
    let mut j = 0;
    let mut offset = 0;
    while i < REGIONS.len() {
        let (guest_region, size) = REGIONS[i];
        assert!(size % PAGE_SIZE == 0);
        let mut k = 0;
        while k < (size / PAGE_SIZE) {
            pages[j] = (guest_region, offset);
            j += 1;
            k += 1;
        }
        offset += size;
        i += 1;
    }
    pages
};

pub struct ContiguousMem(pub HeapMemU8<{ TOTAL_SIZE as usize }, MmapAllocator>);

impl ContiguousMem {
    pub fn new() -> Self {
        ContiguousMem(HeapMemU8::new_with_allocator(MmapAllocator))
    }

    pub fn get_itcm_ptr(&mut self) -> *mut u8 {
        self.0.as_mut_ptr()
    }

    pub fn get_main_ptr(&mut self) -> *mut u8 {
        unsafe { self.0.as_mut_ptr().add(SIZE_OFFSETS[1]) }
    }

    pub fn get_shared_wram_ptr(&mut self) -> *mut u8 {
        unsafe { self.0.as_mut_ptr().add(SIZE_OFFSETS[2]) }
    }

    pub fn get_arm7_wram_ptr(&mut self) -> *mut u8 {
        unsafe { self.0.as_mut_ptr().add(SIZE_OFFSETS[3]) }
    }

    pub fn get_vram_ptr(&mut self) -> *mut u8 {
        unsafe { self.0.as_mut_ptr().add(SIZE_OFFSETS[4]) }
    }

    pub fn host_to_guest(&self, host_addr: u32) -> u32 {
        let addr_offset = host_addr - self.0.as_ptr() as u32;
        let page = addr_offset >> PAGE_SIZE_SHIFT;
        let (region, mem_offset) = GUEST_PAGE_LOOKUP[page as usize];
        region + addr_offset - mem_offset
    }
}
