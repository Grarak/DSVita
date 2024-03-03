use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::mmap::Mmap;
use crate::simple_tree_map::SimpleTreeMap;
use crate::{utils, DEBUG_LOG};
use std::cell::RefCell;

const JIT_MEMORY_SIZE: u32 = 16 * 1024 * 1024;
#[cfg(target_os = "linux")]
const JIT_PAGE_SIZE: u32 = 4096;
#[cfg(target_os = "vita")]
const JIT_PAGE_SIZE: u32 = 16;

type GuestPcStart = u32;
type JitAddr = u32;
type JitBlockSize = u32;

pub struct JitBlockInfo {
    guest_insts_cycle_counts: Vec<u16>,
    addr: u32,
    addr_offsets: Vec<u16>,
    used: RefCell<u64>,
}

impl JitBlockInfo {
    fn new(guest_insts_cycle_counts: Vec<u16>, addr: u32, addr_offsets: Vec<u16>) -> Self {
        JitBlockInfo {
            guest_insts_cycle_counts,
            addr,
            addr_offsets,
            used: RefCell::new(0),
        }
    }

    fn get_jit_addr<const THUMB: bool>(&self, guest_pc_start: u32, guest_pc: u32) -> Option<u32> {
        let pc_offset = (guest_pc - guest_pc_start) >> if THUMB { 1 } else { 2 };
        if pc_offset == 0 {
            Some(self.addr)
        } else if pc_offset as usize - 1 < self.addr_offsets.len() {
            Some(self.addr + self.addr_offsets[pc_offset as usize - 1] as u32)
        } else {
            None
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
    jit_blocks: [SimpleTreeMap<GuestPcStart, JitBlockInfo>; 4],
    mem_blocks: SimpleTreeMap<JitAddr, JitBlockSize>,
    blocks_to_remove: Vec<u32>,
}

impl JitMemory {
    pub fn new() -> Self {
        JitMemory {
            mem: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            jit_blocks: [
                SimpleTreeMap::new(),
                SimpleTreeMap::new(),
                SimpleTreeMap::new(),
                SimpleTreeMap::new(),
            ],
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
            for i in 0..4 {
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
                    let (_, info) = self.jit_blocks[i].remove(&addr);
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

    fn get_guest_pc_end<const THUMB: bool>(
        guest_pc_start: GuestPcStart,
        offsets_len: usize,
    ) -> u32 {
        guest_pc_start + ((offsets_len as u32) << if THUMB { 1 } else { 2 })
    }

    pub fn insert_block<const CPU: CpuType, const THUMB: bool>(
        &mut self,
        opcodes: &[u32],
        args: Option<JitInsertArgs>,
    ) -> u32 {
        if let Some(args) = &args {
            let jit_blocks = &mut self.jit_blocks[CPU as usize + THUMB as usize];
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
                new_addr,
                args.guest_pc_to_jit_addr_offset,
            );
            self.jit_blocks[CPU as usize + THUMB as usize].insert(args.guest_start_pc, info);

            if DEBUG_LOG {
                let allocated_space = self
                    .mem_blocks
                    .iter()
                    .fold(0u32, |sum, (_, block_size)| sum + *block_size);
                let per = (allocated_space * 100) as f32 / JIT_MEMORY_SIZE as f32;
                debug_println!(
                    "{:?} Insert new jit ({:x}) block with size {} at {:x}, {}% allocated with guest pc {:x}",
                    CPU,
                    mem_start_addr,
                    aligned_size,
                    new_addr,
                    per,
                    args.guest_start_pc
                );
            }
        } else {
            debug_println!(
                "{:?} Insert new jit ({:x}) block with size {} at {:x}",
                CPU,
                mem_start_addr,
                aligned_size,
                new_addr,
            );
        }
        new_addr
    }

    pub fn get_jit_start_addr<const CPU: CpuType, const THUMB: bool, const INCREMENT_USED: bool>(
        &self,
        guest_pc: u32,
    ) -> Option<JitGetAddrRet> {
        match self.jit_blocks[CPU as usize + THUMB as usize].get_prev(&guest_pc) {
            Some((_, (guest_pc_start, info))) => {
                if let Some(jit_addr) = info.get_jit_addr::<THUMB>(*guest_pc_start, guest_pc) {
                    if INCREMENT_USED {
                        *info.used.borrow_mut() += 1;
                    }
                    Some(JitGetAddrRet::new(
                        jit_addr,
                        *guest_pc_start,
                        Self::get_guest_pc_end::<THUMB>(*guest_pc_start, info.addr_offsets.len()),
                        &info.guest_insts_cycle_counts,
                    ))
                } else {
                    None
                }
            }
            None => None,
        }
    }

    pub fn invalidate_block<const CPU: CpuType, const THUMB: bool>(
        &mut self,
        guest_pc_from: u32,
        guest_pc_to: u32,
    ) {
        let jit_blocks = &mut self.jit_blocks[CPU as usize + THUMB as usize];
        if let Some((last_index, _)) = jit_blocks.get_prev(&guest_pc_to) {
            let mut index = last_index;
            loop {
                let (guest_pc_start, info) = &jit_blocks[index];
                if *guest_pc_start < guest_pc_from {
                    if info
                        .get_jit_addr::<THUMB>(*guest_pc_start, guest_pc_from)
                        .is_none()
                    {
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
