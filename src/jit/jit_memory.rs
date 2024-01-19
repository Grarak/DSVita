use crate::logging::debug_println;
use crate::mmap::Mmap;
use crate::utils::NoHashMap;
use crate::{utils, DEBUG};
use std::rc::Rc;
use std::thread;

const JIT_MEMORY_SIZE: u32 = 16 * 1024 * 1024;

type JitBlockStartAddr = u32;
type JitBlockSize = u32;

struct GuestPcInfo {
    guest_insts_cycle_counts: Rc<Vec<u8>>,
    guest_pc_start: u32,
    guest_pc_end: u32,
    jit_addr: u32,
}

impl GuestPcInfo {
    fn new(
        guest_insts_cycle_counts: Rc<Vec<u8>>,
        guest_pc_start: u32,
        guest_pc_end: u32,
        jit_addr: u32,
    ) -> Self {
        GuestPcInfo {
            guest_insts_cycle_counts,
            guest_pc_start,
            guest_pc_end,
            jit_addr,
        }
    }
}

pub struct JitMemory {
    memory: Mmap,
    blocks: Vec<(JitBlockStartAddr, JitBlockSize)>,
    guest_pc_mapping: NoHashMap<GuestPcInfo>,
    current_thread_holder: Option<thread::ThreadId>,
}

impl JitMemory {
    pub fn new() -> Self {
        JitMemory {
            memory: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            blocks: Vec::new(),
            guest_pc_mapping: NoHashMap::default(),
            current_thread_holder: None,
        }
    }

    fn find_free_start(&self, required_size: u32) -> u32 {
        if self.blocks.is_empty() {
            return 0;
        }

        let mut previous_end = 0;
        for (start_addr, size) in &self.blocks {
            if (start_addr - previous_end) >= required_size {
                return previous_end;
            }
            previous_end = start_addr + size;
        }

        if (previous_end + required_size) >= JIT_MEMORY_SIZE {
            todo!("Reordering of blocks")
        }
        previous_end
    }

    pub fn insert_block(
        &mut self,
        opcodes: &[u32],
        guest_start_pc: Option<u32>,
        guest_pc_to_jit_addr_offset: Option<NoHashMap<u16>>,
        guest_insts_cycle_counts: Option<Vec<u8>>,
        guest_pc_end: Option<u32>,
    ) -> u32 {
        let aligned_size = utils::align_up((opcodes.len() * 4) as u32, 16);
        let new_addr = self.find_free_start(aligned_size);

        let current_thread_id = thread::current().id();
        match self.current_thread_holder {
            Some(thread_id) => {
                if thread_id != current_thread_id {
                    self.close();
                    self.open();
                    self.current_thread_holder = Some(current_thread_id);
                }
            }
            None => {
                self.open();
                self.current_thread_holder = Some(current_thread_id);
            }
        }

        utils::write_to_mem_slice(&mut self.memory, new_addr, opcodes);
        self.flush_cache(new_addr, (opcodes.len() * 4) as u32);

        match self
            .blocks
            .binary_search_by_key(&new_addr, |(addr, _)| *addr)
        {
            Ok(_) => panic!(),
            Err(index) => self.blocks.insert(index, (new_addr, aligned_size)),
        };

        if let Some(guest_start_pc) = guest_start_pc {
            let cycle_counts = Rc::new(guest_insts_cycle_counts.unwrap());
            let end_pc = guest_pc_end.unwrap();
            self.guest_pc_mapping.insert(
                guest_start_pc,
                GuestPcInfo::new(cycle_counts.clone(), guest_start_pc, end_pc, new_addr),
            );
            for (guest_pc, offset) in guest_pc_to_jit_addr_offset.unwrap() {
                self.guest_pc_mapping.insert(
                    guest_pc,
                    GuestPcInfo::new(
                        cycle_counts.clone(),
                        guest_start_pc,
                        end_pc,
                        new_addr + offset as u32,
                    ),
                );
            }
        }

        if DEBUG {
            let allocated_space = self.blocks.iter().fold(0u32, |sum, (_, size)| sum + *size);
            let per = (allocated_space * 100) as f32 / JIT_MEMORY_SIZE as f32;
            debug_println!(
                "Insert new jit block with size {}, {}% allocated",
                aligned_size,
                per
            )
        }
        new_addr + self.memory.as_ptr() as u32
    }

    pub fn get_jit_start_addr(
        &mut self,
        guest_pc: u32,
    ) -> Option<(
        u32,         /* jit_addr */
        u32,         /* guest_pc_start */
        u32,         /* guest_pc_end */
        Rc<Vec<u8>>, /* cycle_counts */
    )> {
        match self.guest_pc_mapping.get(&guest_pc) {
            None => None,
            Some(info) => Some((
                self.memory.as_ptr() as u32 + info.jit_addr,
                info.guest_pc_start,
                info.guest_pc_end,
                info.guest_insts_cycle_counts.clone(),
            )),
        }
    }

    pub fn invalidate_block(&mut self, guest_pc: u32) {
        loop {
            match self.guest_pc_mapping.get(&guest_pc) {
                None => {
                    break;
                }
                Some(info) => {
                    debug_println!(
                        "Removing jit block at {:x} with guest start pc {:x}",
                        self.memory.as_ptr() as u32 + info.jit_addr,
                        info.guest_pc_start
                    );

                    if let Some(start_info) = self.guest_pc_mapping.get(&info.guest_pc_start) {
                        match self
                            .blocks
                            .binary_search_by_key(&start_info.jit_addr, |(addr, _)| *addr)
                        {
                            Ok(index) => {
                                self.blocks.remove(index);
                            }
                            Err(_) => {}
                        }
                    }
                }
            }
            self.guest_pc_mapping.remove(&guest_pc);
        }
    }

    #[cfg(target_os = "linux")]
    fn open(&mut self) {}

    #[cfg(target_os = "linux")]
    fn close(&mut self) {}

    #[cfg(target_os = "linux")]
    fn flush_cache(&self, _: JitBlockStartAddr, _: JitBlockSize) {}

    #[cfg(target_os = "vita")]
    fn open(&mut self) {
        let ret = unsafe { vitasdk_sys::sceKernelOpenVMDomain() };
        if ret < vitasdk_sys::SCE_OK as _ {
            panic!("Can't open vm domain {}", ret);
        }
    }

    #[cfg(target_os = "vita")]
    fn close(&mut self) {
        let ret = unsafe { vitasdk_sys::sceKernelCloseVMDomain() };
        if ret < vitasdk_sys::SCE_OK as _ {
            panic!("Can't close vm domain {}", ret);
        }
    }

    #[cfg(target_os = "vita")]
    fn flush_cache(&self, start_addr: JitBlockStartAddr, size: JitBlockSize) {
        let ret = unsafe {
            vitasdk_sys::sceKernelSyncVMDomain(
                self.memory.block_uid,
                (self.memory.as_ptr() as u32 + start_addr) as _,
                size,
            )
        };
        if ret < vitasdk_sys::SCE_OK as _ {
            panic!("Can't sync vm domain {}", ret)
        }
    }
}
