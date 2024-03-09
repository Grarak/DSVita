use crate::hle::memory::regions;
use crate::logging::debug_println;
use crate::mmap::Mmap;
use crate::simple_tree_map::SimpleTreeMap;
use crate::{utils, DEBUG_LOG};
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
        }
    }

    fn get_jit_addr<const THUMB: bool>(&self, guest_pc: u32) -> Option<u32> {
        let pc_offset = (guest_pc - self.guest_pc_start) >> if THUMB { 1 } else { 2 };
        if pc_offset == 0 {
            Some(self.addr)
        } else if pc_offset as usize - 1 < self.addr_offsets.len() {
            Some(
                self.addr
                    + unsafe { *self.addr_offsets.get_unchecked(pc_offset as usize - 1) as u32 },
            )
        } else {
            None
        }
    }

    fn get_jit_addr_unchecked<const THUMB: bool>(&self, guest_pc: u32) -> u32 {
        let pc_offset = (guest_pc - self.guest_pc_start) >> if THUMB { 1 } else { 2 };
        if pc_offset == 0 {
            self.addr
        } else {
            self.addr + unsafe { *self.addr_offsets.get_unchecked(pc_offset as usize - 1) as u32 }
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
    mem_common_offset: u32,
    mem_offset: u32,
    main_jit_blocks: FastMap,
    shared_wram_jit_blocks: FastMap,
    wram_arm7_jit_blocks: FastMap,
    jit_blocks: [SimpleTreeMap<GuestPcStart, Box<JitBlockInfo>>; 2],
}

impl JitMemory {
    pub fn new() -> Self {
        JitMemory {
            mem: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            mem_common_offset: 0,
            mem_offset: 0,
            main_jit_blocks: FastMap::new("main_jit_blocks", regions::MAIN_MEMORY_SIZE),
            shared_wram_jit_blocks: FastMap::new(
                "shared_wram_jit_blocks",
                regions::SHARED_WRAM_SIZE,
            ),
            wram_arm7_jit_blocks: FastMap::new("wram_arm7_jit_blocks", regions::ARM7_WRAM_SIZE),
            jit_blocks: [SimpleTreeMap::new(), SimpleTreeMap::new()],
        }
    }

    fn allocate_block(&mut self, required_size: u32) -> u32 {
        let offset = if self.mem_offset + required_size >= JIT_MEMORY_SIZE {
            self.mem_offset = self.mem_common_offset;
            self.jit_blocks[0].clear();
            self.jit_blocks[1].clear();
            self.main_jit_blocks.mem_map.fill(0);
            self.shared_wram_jit_blocks.mem_map.fill(0);
            self.wram_arm7_jit_blocks.mem_map.fill(0);
            self.mem_offset
        } else {
            let addr = self.mem_offset;
            self.mem_offset += required_size;
            addr
        };
        offset + self.mem.as_ptr() as u32
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
        let aligned_size = utils::align_up((opcodes.len() << 2) as u32, JIT_PAGE_SIZE);
        let new_addr = self.allocate_block(aligned_size);
        let mem_start_addr = self.mem.as_ptr() as u32;
        let offset_addr = new_addr - mem_start_addr;

        self.set_rw(offset_addr, aligned_size);
        utils::write_to_mem_slice(&mut self.mem, offset_addr, opcodes);
        self.flush_cache(offset_addr, aligned_size);
        self.set_rx(offset_addr, aligned_size);

        match args {
            Some(args) => {
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
                    let per = ((self.mem_offset - self.mem.as_ptr() as u32) * 100) as f32
                        / JIT_MEMORY_SIZE as f32;
                    debug_println!(
                    "Insert new jit ({:x}) block with size {} at {:x}, {}% allocated with guest pc {:x}",
                    mem_start_addr,
                    aligned_size,
                    new_addr,
                    per,
                    args.guest_start_pc
                );
                }
            }
            None => {
                // Inserts without args is common code, happens on initialization, adjust offset
                self.mem_common_offset = new_addr - self.mem.as_ptr() as u32 + aligned_size;
                debug_println!(
                    "Insert new jit ({:x}) block with size {} at {:x}",
                    mem_start_addr,
                    aligned_size,
                    new_addr,
                );
            }
        }
        new_addr
    }

    #[inline]
    pub fn get_jit_start_addr<const THUMB: bool>(&self, guest_pc: u32) -> Option<JitGetAddrRet> {
        if guest_pc & 0xFF000000 == regions::MAIN_MEMORY_OFFSET {
            match self
                .main_jit_blocks
                .get((guest_pc & (regions::MAIN_MEMORY_SIZE - 1)) as usize)
            {
                Some(info) => Some(JitGetAddrRet::new(
                    info.get_jit_addr_unchecked::<THUMB>(guest_pc),
                    info.guest_pc_start,
                    Self::get_guest_pc_end::<THUMB>(info.guest_pc_start, info.addr_offsets.len()),
                    &info.guest_insts_cycle_counts,
                )),
                None => None,
            }
        } else {
            match self.jit_blocks[THUMB as usize].get_prev(&guest_pc) {
                Some((_, (_, info))) => {
                    if let Some(jit_addr) = info.get_jit_addr::<THUMB>(guest_pc) {
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
