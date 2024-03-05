use crate::hle::memory::regions;
use crate::logging::debug_println;
use crate::mmap::Mmap;
use crate::simple_tree_map::SimpleTreeMap;
use crate::{utils, DEBUG_LOG};
use std::cell::RefCell;
use std::{mem, ptr};

const JIT_MEMORY_SIZE: u32 = 16 * 1024 * 1024;
#[cfg(target_os = "linux")]
const JIT_PAGE_SIZE: u32 = 4096;
#[cfg(target_os = "vita")]
const JIT_PAGE_SIZE: u32 = 16;

type GuestPcStart = u32;
type JitAddr = u32;
type JitBlockSize = u32;

struct JitBlockInfo {
    guest_insts_cycle_counts: Vec<u16>,
    guest_pc_start: u32,
    addr: u32,
    addr_offsets: Vec<u16>,
    used: RefCell<u64>,
}

impl JitBlockInfo {
    fn new(
        guest_insts_cycle_counts: Vec<u16>,
        guest_pc_start: u32,
        addr: u32,
        addr_offsets: Vec<u16>,
    ) -> Self {
        JitBlockInfo {
            guest_insts_cycle_counts,
            guest_pc_start,
            addr,
            addr_offsets,
            used: RefCell::new(0),
        }
    }

    fn get_jit_addr<const THUMB: bool>(&self, guest_pc: u32) -> Option<u32> {
        let pc_offset = (guest_pc - self.guest_pc_start) >> if THUMB { 1 } else { 2 };
        if pc_offset == 0 {
            Some(self.addr)
        } else if pc_offset as usize - 1 < self.addr_offsets.len() {
            Some(self.addr + self.addr_offsets[pc_offset as usize - 1] as u32)
        } else {
            None
        }
    }
}

struct FastMap {
    mem_map: Mmap,
}

impl FastMap {
    fn new(name: &str, size: u32) -> Self {
        FastMap {
            mem_map: Mmap::rw(name, size * mem::size_of::<*const JitBlockInfo>() as u32).unwrap(),
        }
    }

    fn get(&self, index: usize) -> Option<&JitBlockInfo> {
        let ptr = unsafe {
            (self.mem_map.as_ptr() as *const *const JitBlockInfo)
                .add(index)
                .read()
        };
        if !ptr.is_null() {
            Some(unsafe { ptr.as_ref().unwrap_unchecked() })
        } else {
            None
        }
    }

    fn remove(&mut self, index: usize) {
        let ptr = unsafe { (self.mem_map.as_mut_ptr() as *mut *const JitBlockInfo).add(index) };
        unsafe { ptr.write(ptr::null()) };
    }

    fn remove_block<const THUMB: bool>(&mut self, start: u32, end: u32) {
        for i in (start..=end).step_by(if THUMB { 2 } else { 4 }) {
            self.remove(i as usize);
        }
    }

    fn insert(&mut self, index: usize, block: &JitBlockInfo) {
        let ptr = unsafe { (self.mem_map.as_mut_ptr() as *mut *const JitBlockInfo).add(index) };
        unsafe { ptr.write(block as _) };
    }

    fn insert_block<const THUMB: bool>(&mut self, start: u32, end: u32, block: &JitBlockInfo) {
        for i in (start..=end).step_by(if THUMB { 2 } else { 4 }) {
            self.insert(i as usize, block);
        }
    }
}

pub struct JitInsertArgs {
    guest_start_pc: u32,
    guest_pc_to_jit_addr_offset: Vec<u16>,
    guest_insts_cycle_counts: Vec<u16>,
}

pub struct JitGetAddrRet<'a> {
    pub jit_addr: u32,
    pub guest_start_pc: u32,
    pub guest_end_pc: u32,
    pub guest_insts_cycle_counts: &'a Vec<u16>,
}

impl<'a> JitGetAddrRet<'a> {
    fn new(
        jit_addr: u32,
        guest_start_pc: u32,
        guest_end_pc: u32,
        guest_insts_cycle_counts: &'a Vec<u16>,
    ) -> Self {
        JitGetAddrRet {
            jit_addr,
            guest_start_pc,
            guest_end_pc,
            guest_insts_cycle_counts,
        }
    }
}

impl JitInsertArgs {
    pub fn new(
        guest_start_pc: u32,
        guest_pc_to_jit_addr_offset: Vec<u16>,
        guest_insts_cycle_counts: Vec<u16>,
    ) -> Self {
        JitInsertArgs {
            guest_start_pc,
            guest_pc_to_jit_addr_offset,
            guest_insts_cycle_counts,
        }
    }
}

pub struct JitMemory {
    mem: Mmap,
    main_jit_blocks: FastMap,
    jit_blocks: [SimpleTreeMap<GuestPcStart, Box<JitBlockInfo>>; 2],
    mem_blocks: SimpleTreeMap<JitAddr, JitBlockSize>,
    blocks_to_remove: Vec<u32>,
}

impl JitMemory {
    pub fn new() -> Self {
        JitMemory {
            mem: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            main_jit_blocks: FastMap::new("main_jit_blocks", regions::MAIN_MEMORY_SIZE),
            jit_blocks: [SimpleTreeMap::new(), SimpleTreeMap::new()],
            mem_blocks: SimpleTreeMap::new(),
            blocks_to_remove: Vec::new(),
        }
    }

    fn find_free_start(&mut self, required_size: u32) -> u32 {
        let mut previous_end = self.mem.as_ptr() as u32;
        for (start_addr, block_size) in &self.mem_blocks {
            if (start_addr - previous_end) >= required_size {
                return previous_end;
            }
            previous_end = *start_addr + *block_size;
        }

        if previous_end + required_size >= self.mem.as_ptr() as u32 + JIT_MEMORY_SIZE {
            for i in 0..self.jit_blocks.len() {
                let mut min_used = u64::MAX;
                for (addr, info) in &self.jit_blocks[i] {
                    let used = *info.used.borrow();
                    if used < min_used {
                        self.blocks_to_remove.clear();
                        self.blocks_to_remove.push(*addr);
                        min_used = used;
                    } else if used == min_used {
                        self.blocks_to_remove.push(*addr);
                    }
                }

                while let Some(addr) = self.blocks_to_remove.pop() {
                    let (guest_pc_start, info) = self.jit_blocks[i].remove(&addr);
                    if guest_pc_start & 0xFF000000 == regions::MAIN_MEMORY_OFFSET {
                        let start = guest_pc_start & (regions::MAIN_MEMORY_SIZE - 1);
                        if i == 0 {
                            let end = Self::get_guest_pc_end::<false>(
                                guest_pc_start,
                                info.addr_offsets.len(),
                            ) & (regions::MAIN_MEMORY_SIZE - 1);
                            self.main_jit_blocks.remove_block::<false>(start, end);
                        } else {
                            let end = Self::get_guest_pc_end::<true>(
                                guest_pc_start,
                                info.addr_offsets.len(),
                            ) & (regions::MAIN_MEMORY_SIZE - 1);
                            self.main_jit_blocks.remove_block::<true>(start, end);
                        }
                    }
                    self.mem_blocks.remove(&info.addr);
                }

                for (_, info) in &self.jit_blocks[i] {
                    *info.used.borrow_mut() = 0;
                }
            }
            self.find_free_start(required_size)
        } else {
            previous_end
        }
    }

    const fn get_guest_pc_end<const THUMB: bool>(
        guest_pc_start: GuestPcStart,
        offsets_len: usize,
    ) -> u32 {
        guest_pc_start + ((offsets_len as u32) << if THUMB { 1 } else { 2 })
    }

    pub fn insert_block<const THUMB: bool>(
        &mut self,
        opcodes: &[u32],
        args: Option<JitInsertArgs>,
    ) -> u32 {
        if let Some(args) = &args {
            let jit_blocks = &mut self.jit_blocks[THUMB as usize];
            if let Some((next_block_index, (next_block_guest_pc_start, next_block_info))) =
                jit_blocks.get_next(&args.guest_start_pc)
            {
                if Self::get_guest_pc_end::<THUMB>(
                    args.guest_start_pc,
                    args.guest_pc_to_jit_addr_offset.len(),
                ) >= *next_block_guest_pc_start
                {
                    self.mem_blocks.remove(&next_block_info.addr);
                    jit_blocks.remove_at(next_block_index);
                }
            }
        }

        let aligned_size = utils::align_up((opcodes.len() << 2) as u32, JIT_PAGE_SIZE);
        let new_addr = self.find_free_start(aligned_size);
        let mem_start_addr = self.mem.as_ptr() as u32;
        let offset_addr = new_addr - mem_start_addr;

        self.set_rw(offset_addr, aligned_size);
        utils::write_to_mem_slice(&mut self.mem, offset_addr, opcodes);
        self.flush_cache(offset_addr, aligned_size);
        self.set_rx(offset_addr, aligned_size);

        self.mem_blocks.insert(new_addr, aligned_size);

        if let Some(args) = args {
            let info = JitBlockInfo::new(
                args.guest_insts_cycle_counts,
                args.guest_start_pc,
                new_addr,
                args.guest_pc_to_jit_addr_offset,
            );
            {
                let blocks = &mut self.jit_blocks[THUMB as usize];
                let insert_index = blocks.insert(args.guest_start_pc, Box::new(info));
                if args.guest_start_pc & 0xFF000000 == regions::MAIN_MEMORY_OFFSET {
                    let block = blocks[insert_index].1.as_ref();
                    self.main_jit_blocks.insert_block::<THUMB>(
                        block.guest_pc_start & (regions::MAIN_MEMORY_SIZE - 1),
                        Self::get_guest_pc_end::<THUMB>(
                            block.guest_pc_start,
                            block.addr_offsets.len(),
                        ) & (regions::MAIN_MEMORY_SIZE - 1),
                        block,
                    );
                }
            }

            if DEBUG_LOG {
                let allocated_space = self
                    .mem_blocks
                    .iter()
                    .fold(0u32, |sum, (_, block_size)| sum + *block_size);
                let per = (allocated_space * 100) as f32 / JIT_MEMORY_SIZE as f32;
                debug_println!(
                    "Insert new jit ({:x}) block with size {} at {:x}, {}% allocated with guest pc {:x}",
                    mem_start_addr,
                    aligned_size,
                    new_addr,
                    per,
                    args.guest_start_pc
                );
            }
        } else {
            debug_println!(
                "Insert new jit ({:x}) block with size {} at {:x}",
                mem_start_addr,
                aligned_size,
                new_addr,
            );
        }
        new_addr
    }

    pub fn get_jit_start_addr<const THUMB: bool, const INCREMENT_USED: bool>(
        &self,
        guest_pc: u32,
    ) -> Option<JitGetAddrRet> {
        if guest_pc & 0xFF000000 == regions::MAIN_MEMORY_OFFSET {
            match self
                .main_jit_blocks
                .get((guest_pc & (regions::MAIN_MEMORY_SIZE - 1)) as usize)
            {
                Some(info) => {
                    if INCREMENT_USED {
                        unsafe { *info.used.as_ptr() += 1 };
                    }
                    Some(JitGetAddrRet::new(
                        info.get_jit_addr::<THUMB>(guest_pc).unwrap(),
                        info.guest_pc_start,
                        Self::get_guest_pc_end::<THUMB>(
                            info.guest_pc_start,
                            info.addr_offsets.len(),
                        ),
                        &info.guest_insts_cycle_counts,
                    ))
                }
                None => None,
            }
        } else {
            match self.jit_blocks[THUMB as usize].get_prev(&guest_pc) {
                Some((_, (_, info))) => {
                    if let Some(jit_addr) = info.get_jit_addr::<THUMB>(guest_pc) {
                        unsafe { *info.used.as_ptr() += 1 };
                        Some(JitGetAddrRet::new(
                            jit_addr,
                            info.guest_pc_start,
                            Self::get_guest_pc_end::<THUMB>(
                                info.guest_pc_start,
                                info.addr_offsets.len(),
                            ),
                            &info.guest_insts_cycle_counts,
                        ))
                    } else {
                        None
                    }
                }
                None => None,
            }
        }
    }

    pub fn invalidate_block<const THUMB: bool>(&mut self, guest_pc_from: u32, guest_pc_to: u32) {
        let jit_blocks = &mut self.jit_blocks[THUMB as usize];
        if let Some((last_index, _)) = jit_blocks.get_prev(&guest_pc_to) {
            let mut index = last_index;
            loop {
                let (_, info) = &jit_blocks[index];
                if info.guest_pc_start < guest_pc_from {
                    if info.get_jit_addr::<THUMB>(guest_pc_from).is_none() {
                        index += 1;
                    }
                    break;
                }
                if index == 0 {
                    break;
                }
                index -= 1;
            }
            for (_, block_info) in jit_blocks.drain(index..last_index + 1) {
                self.mem_blocks.remove(&block_info.addr);
            }
        }
    }

    #[cfg(target_os = "linux")]
    pub fn open(&self) {}

    #[cfg(target_os = "linux")]
    pub fn close(&self) {}

    #[cfg(target_os = "linux")]
    fn flush_cache(&self, _: u32, _: JitBlockSize) {}

    #[cfg(target_os = "linux")]
    fn set_rw(&self, start_addr: u32, size: JitBlockSize) {
        unsafe {
            libc::mprotect(
                (self.mem.as_ptr() as u32 + start_addr) as _,
                size as _,
                libc::PROT_READ | libc::PROT_WRITE,
            )
        };
    }

    #[cfg(target_os = "linux")]
    fn set_rx(&self, start_addr: u32, size: JitBlockSize) {
        unsafe {
            libc::mprotect(
                (self.mem.as_ptr() as u32 + start_addr) as _,
                size as _,
                libc::PROT_READ | libc::PROT_EXEC,
            )
        };
    }

    #[cfg(target_os = "vita")]
    pub fn open(&self) {
        unsafe { vitasdk_sys::sceKernelOpenVMDomain() };
    }

    #[cfg(target_os = "vita")]
    pub fn close(&self) {
        unsafe { vitasdk_sys::sceKernelCloseVMDomain() };
    }

    #[cfg(target_os = "vita")]
    fn flush_cache(&self, start_addr: u32, size: JitBlockSize) {
        unsafe {
            vitasdk_sys::sceKernelSyncVMDomain(
                self.mem.block_uid,
                (self.mem.as_ptr() as u32 + start_addr) as _,
                size,
            )
        };
    }

    #[cfg(target_os = "vita")]
    fn set_rw(&self, _: u32, _: JitBlockSize) {}

    #[cfg(target_os = "vita")]
    fn set_rx(&self, _r: u32, _: JitBlockSize) {}
}
