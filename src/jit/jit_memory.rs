use crate::hle::memory::regions;
use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::mmap::Mmap;
use crate::utils;
use crate::utils::NoHashMap;
#[cfg(not(debug_assertions))]
use std::hint::unreachable_unchecked;
use std::mem;
use std::mem::ManuallyDrop;
use std::rc::Rc;

const JIT_MEMORY_SIZE: u32 = 16 * 1024 * 1024;

type JitBlockStartAddr = u32;
type JitBlockSize = u32;

#[derive(Clone)]
pub struct GuestPcInfo {
    pub guest_insts_cycle_counts: Rc<Vec<u8>>,
    pub guest_pc_start: u32,
    pub guest_pc_end: u32,
    pub jit_addr: u32,
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

type FastGuestInfo = Option<ManuallyDrop<Box<GuestPcInfo>>>;

struct FastGuestPcMap {
    mapping: Mmap,
    size: usize,
}

impl FastGuestPcMap {
    fn new(name: &str, size: u32) -> Self {
        FastGuestPcMap {
            mapping: Mmap::rw(name, size * mem::size_of::<FastGuestPcMap>() as u32).unwrap(),
            size: size as usize,
        }
    }

    fn insert(&mut self, guest_pc: u32, info: GuestPcInfo) {
        let info = ManuallyDrop::new(Box::new(info));
        let index = guest_pc as usize & (self.size - 1);
        let mapping: &mut [FastGuestInfo] = unsafe { mem::transmute(self.mapping.as_mut()) };
        mapping[index] = Some(info);
    }

    fn get(&self, guest_pc: u32) -> Option<&GuestPcInfo> {
        let index = guest_pc as usize & (self.size - 1);
        let mapping: &[FastGuestInfo] = unsafe { mem::transmute(self.mapping.as_ref()) };
        mapping[index].as_ref().map(|info| info.as_ref())
    }

    fn invalidate(&mut self, guest_pc: u32) {
        let index = guest_pc as usize & (self.size - 1);
        let mapping: &mut [FastGuestInfo] = unsafe { mem::transmute(self.mapping.as_mut()) };
        if let Some(info) = &mut mapping[index] {
            unsafe { ManuallyDrop::drop(info) };
        }
        mapping[index] = None;
    }
}

pub struct JitMemory {
    memory: Mmap,
    blocks: Vec<(JitBlockStartAddr, JitBlockSize)>,
    guest_pc_mapping: NoHashMap<GuestPcInfo>,
    main_memory_guest_pc_mapping: FastGuestPcMap,
}

impl JitMemory {
    pub fn new() -> Self {
        JitMemory {
            memory: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            blocks: Vec::new(),
            guest_pc_mapping: NoHashMap::default(),
            main_memory_guest_pc_mapping: FastGuestPcMap::new(
                "main_memory_guest_pc_mapping",
                regions::MAIN_MEMORY_SIZE,
            ),
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

    pub fn insert_block<const CPU: CpuType>(
        &mut self,
        opcodes: &[u32],
        guest_start_pc: Option<u32>,
        guest_pc_to_jit_addr_offset: Option<NoHashMap<u16>>,
        guest_insts_cycle_counts: Option<Vec<u8>>,
        guest_pc_end: Option<u32>,
    ) -> u32 {
        #[cfg(target_os = "linux")]
        let aligned_size = utils::align_up((opcodes.len() << 2) as u32, 4096);
        #[cfg(target_os = "vita")]
        let aligned_size = utils::align_up((opcodes.len() << 2) as u32, 16);
        let new_addr = self.find_free_start(aligned_size);

        utils::write_to_mem_slice(&mut self.memory, new_addr, opcodes);
        self.flush_cache(new_addr, (opcodes.len() << 2) as u32);

        match self
            .blocks
            .binary_search_by_key(&new_addr, |(addr, _)| *addr)
        {
            Ok(_) => {
                #[cfg(debug_assertions)]
                panic!();
                #[cfg(not(debug_assertions))]
                unsafe {
                    unreachable_unchecked()
                };
            }
            Err(index) => self.blocks.insert(index, (new_addr, aligned_size)),
        };

        let new_addr = new_addr + self.memory.as_ptr() as u32;
        if let Some(guest_start_pc) = guest_start_pc {
            let cycle_counts = Rc::new(guest_insts_cycle_counts.unwrap());
            let end_pc = guest_pc_end.unwrap();

            let addr_base = guest_start_pc & 0xFF000000;
            match addr_base {
                regions::MAIN_MEMORY_OFFSET => {
                    self.main_memory_guest_pc_mapping.insert(
                        guest_start_pc,
                        GuestPcInfo::new(cycle_counts.clone(), guest_start_pc, end_pc, new_addr),
                    );
                    for (guest_pc, offset) in guest_pc_to_jit_addr_offset.unwrap() {
                        self.main_memory_guest_pc_mapping.insert(
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
                _ => {
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
            }
        }

        #[cfg(debug_assertions)]
        {
            let allocated_space = self.blocks.iter().fold(0u32, |sum, (_, size)| sum + *size);
            let per = (allocated_space * 100) as f32 / JIT_MEMORY_SIZE as f32;
            debug_println!(
                "{:?} Insert new jit block with size {}, {}% allocated with guest pc {:x}",
                CPU,
                aligned_size,
                per,
                guest_start_pc.unwrap_or(0)
            )
        }
        new_addr
    }

    pub fn get_jit_start_addr<const CPU: CpuType>(&self, guest_pc: u32) -> Option<&GuestPcInfo> {
        let addr_base = guest_pc & 0xFF000000;
        match addr_base {
            regions::MAIN_MEMORY_OFFSET => self.main_memory_guest_pc_mapping.get(guest_pc),
            _ => self.guest_pc_mapping.get(&guest_pc),
        }
    }

    pub fn invalidate_block<const CPU: CpuType>(&mut self, guest_pc: u32) {
        if let Some(info) = self.guest_pc_mapping.remove(&guest_pc) {
            debug_println!(
                "Removing jit block at {:x} with guest start pc {:x}",
                info.jit_addr,
                info.guest_pc_start
            );

            if let Some(start_info) = self.guest_pc_mapping.get(&info.guest_pc_start) {
                if let Ok(index) = self.blocks.binary_search_by_key(
                    &(start_info.jit_addr - self.memory.as_ptr() as u32),
                    |(addr, _)| *addr,
                ) {
                    self.blocks.remove(index);
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    pub fn open(&mut self) {}

    #[cfg(target_os = "linux")]
    pub fn close(&mut self) {}

    #[cfg(target_os = "linux")]
    fn flush_cache(&self, _: JitBlockStartAddr, _: JitBlockSize) {}

    #[cfg(target_os = "vita")]
    pub fn open(&mut self) {
        let ret = unsafe { vitasdk_sys::sceKernelOpenVMDomain() };
        if ret < vitasdk_sys::SCE_OK as _ {
            panic!("Can't open vm domain {}", ret);
        }
    }

    #[cfg(target_os = "vita")]
    pub fn close(&mut self) {
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
        #[cfg(debug_assertions)]
        if ret < vitasdk_sys::SCE_OK as _ {
            panic!("Can't sync vm domain {}", ret)
        }
    }
}
