use crate::logging::debug_println;
use crate::mmap::Mmap;
use crate::{utils, DEBUG};
use im::OrdMap;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;

const JIT_MEMORY_SIZE: u32 = 16 * 1024 * 1024;

type JitBlockStartAddr = u32;
type JitBlockSize = u32;
type GuestPc = u32;
type GuestStartPc = u32;

#[derive(Clone)]
struct CodeBlock {
    guest_pc_to_jit_addr_offset: HashMap<u32, u16>,
    guest_insts_cycle_counts: Arc<Vec<u8>>,
    guest_pc_end: u32,
    jit_start_addr: u32,
}

impl CodeBlock {
    fn new(
        guest_pc_to_jit_addr_offset: HashMap<u32, u16>,
        guest_insts_cycle_counts: Vec<u8>,
        guest_pc_end: u32,
        jit_start_addr: u32,
    ) -> Self {
        CodeBlock {
            guest_pc_to_jit_addr_offset,
            guest_insts_cycle_counts: Arc::new(guest_insts_cycle_counts),
            guest_pc_end,
            jit_start_addr,
        }
    }
}

pub struct JitMemory {
    pub memory: Mmap,
    blocks: OrdMap<JitBlockStartAddr, JitBlockSize>,
    guest_start_mapping: HashMap<GuestPc, (u32, Rc<CodeBlock>, u16)>,
    code_blocks: OrdMap<GuestStartPc, Rc<CodeBlock>>,
    current_thread_holder: Option<thread::ThreadId>,
}

impl JitMemory {
    pub fn new() -> Self {
        JitMemory {
            memory: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            blocks: OrdMap::new(),
            guest_start_mapping: HashMap::new(),
            code_blocks: OrdMap::new(),
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
        guest_start_pc: Option<GuestStartPc>,
        guest_pc_to_jit_addr_offset: Option<HashMap<u32, u16>>,
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

        self.blocks.insert(new_addr, aligned_size);

        if let Some(guest_start_pc) = guest_start_pc {
            self.code_blocks.insert(
                guest_start_pc,
                Rc::new(CodeBlock::new(
                    guest_pc_to_jit_addr_offset.unwrap(),
                    guest_insts_cycle_counts.unwrap(),
                    guest_pc_end.unwrap(),
                    new_addr,
                )),
            );
        }

        if DEBUG {
            let allocated_space = self.blocks.values().sum::<u32>();
            let per = (allocated_space * 100) as f32 / JIT_MEMORY_SIZE as f32;
            println!(
                "Insert new block with size {}, {}% allocated",
                aligned_size, per
            )
        }
        new_addr + self.memory.as_ptr() as u32
    }

    fn get_code_block(&mut self, guest_pc: u32) -> Option<(u32, Rc<CodeBlock>, u16)> {
        match self.guest_start_mapping.get(&guest_pc) {
            None => match self.code_blocks.get_prev(&guest_pc) {
                Some((guest_start_pc, code_block)) => {
                    if guest_pc == *guest_start_pc {
                        let ret = (*guest_start_pc, code_block.clone(), 0);
                        self.guest_start_mapping.insert(guest_pc, ret.clone());
                        Some(ret)
                    } else if let Some(offset) =
                        code_block.guest_pc_to_jit_addr_offset.get(&guest_pc)
                    {
                        let ret = (*guest_start_pc, code_block.clone(), *offset);
                        self.guest_start_mapping.insert(guest_pc, ret.clone());
                        Some(ret.clone())
                    } else {
                        None
                    }
                }
                None => None,
            },
            Some(ret) => Some(ret.clone()),
        }
    }

    pub fn get_jit_start_addr(&mut self, guest_pc: u32) -> Option<(u32, u32, u32, Arc<Vec<u8>>)> {
        match self.get_code_block(guest_pc) {
            Some((guest_start_pc, code_block, offset)) => Some((
                code_block.jit_start_addr + offset as u32 + self.memory.as_ptr() as u32,
                guest_start_pc,
                code_block.guest_pc_end,
                code_block.guest_insts_cycle_counts.clone(),
            )),
            None => None,
        }
    }

    pub fn invalidate_block(&mut self, guest_pc: u32) {
        if let Some((guest_start_pc, code_block, _)) = self.get_code_block(guest_pc) {
            debug_println!(
                "Removing jit block at {:x} with guest start pc {:x}",
                self.memory.as_ptr() as u32 + code_block.jit_start_addr,
                guest_start_pc
            );

            let jit_start_addr = code_block.jit_start_addr;
            self.guest_start_mapping.remove(&guest_pc);
            self.blocks.remove(&jit_start_addr);
            self.code_blocks.remove(&guest_start_pc);
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
