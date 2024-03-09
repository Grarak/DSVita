use crate::hle::memory::regions;
use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::mmap::Mmap;
use crate::utils::HeapMem;
use crate::{utils, DEBUG_LOG};
use core::slice;
use std::mem;

const JIT_MEMORY_SIZE: u32 = 16 * 1024 * 1024;
#[cfg(target_os = "linux")]
const JIT_PAGE_SIZE: u32 = 4096;
#[cfg(target_os = "vita")]
const JIT_PAGE_SIZE: u32 = 16;

#[derive(Copy, Clone, Default)]
pub struct JitInstInfo {
    pub jit_addr: u32,
    pub cycle_count: u8,          // Cycle count of this instruction
    pub pre_cycle_count_sum: u16, // Sum of cycle counts of block up until this instruction
}

impl JitInstInfo {
    fn new(jit_addr: u32, cycle_count: u8, pre_cycle_count_sum: u16) -> Self {
        JitInstInfo {
            jit_addr,
            cycle_count,
            pre_cycle_count_sum,
        }
    }
}

pub struct JitInsertArgs {
    guest_start_pc: u32,
    guest_pc_to_jit_addr_offset: Vec<u16>,
    guest_insts_cycle_counts: Vec<u16>,
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
    itcm_info: HeapMem<JitInstInfo, { regions::INSTRUCTION_TCM_SIZE as usize }>,
    main_info: Mmap,
    wram_info:
        HeapMem<JitInstInfo, { (regions::SHARED_WRAM_SIZE + regions::ARM7_WRAM_SIZE) as usize }>,
    arm9_bios_info: HeapMem<JitInstInfo, { regions::ARM9_BIOS_SIZE as usize }>,
    arm7_bios_info: HeapMem<JitInstInfo, { regions::ARM7_BIOS_SIZE as usize }>,
}

impl JitMemory {
    pub fn new() -> Self {
        JitMemory {
            mem: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            mem_common_offset: 0,
            mem_offset: 0,
            itcm_info: HeapMem::new(),
            main_info: Mmap::rw(
                "main_jit_info",
                regions::MAIN_MEMORY_SIZE * mem::size_of::<JitInstInfo>() as u32,
            )
            .unwrap(),
            wram_info: HeapMem::new(),
            arm9_bios_info: HeapMem::new(),
            arm7_bios_info: HeapMem::new(),
        }
    }

    fn get_main_info_slice(&self) -> &[JitInstInfo] {
        unsafe {
            slice::from_raw_parts(
                self.main_info.as_ptr() as _,
                regions::MAIN_MEMORY_SIZE as usize,
            )
        }
    }

    fn get_main_info_slice_mut(&mut self) -> &mut [JitInstInfo] {
        unsafe {
            slice::from_raw_parts_mut(
                self.main_info.as_mut_ptr() as _,
                regions::MAIN_MEMORY_SIZE as usize,
            )
        }
    }

    fn allocate_block(&mut self, required_size: u32) -> u32 {
        let offset = if self.mem_offset + required_size >= JIT_MEMORY_SIZE {
            self.mem_offset = self.mem_common_offset;
            self.itcm_info.fill(JitInstInfo::default());
            self.main_info.fill(0);
            self.wram_info.fill(JitInstInfo::default());
            self.arm9_bios_info.fill(JitInstInfo::default());
            self.arm7_bios_info.fill(JitInstInfo::default());
            self.mem_offset
        } else {
            let addr = self.mem_offset;
            self.mem_offset += required_size;
            addr
        };
        offset + self.mem.as_ptr() as u32
    }

    pub fn insert_block<const CPU: CpuType, const THUMB: bool>(
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
                let insert_block = |mem: &mut [JitInstInfo]| {
                    let start_addr = args.guest_start_pc as usize % mem.len();
                    mem[start_addr] =
                        JitInstInfo::new(new_addr, args.guest_insts_cycle_counts[0] as u8, 0);
                    for i in 0..args.guest_pc_to_jit_addr_offset.len() {
                        mem[(start_addr + ((i + 1) << if THUMB { 1 } else { 2 })) % mem.len()] =
                            JitInstInfo::new(
                                new_addr + args.guest_pc_to_jit_addr_offset[i] as u32,
                                (args.guest_insts_cycle_counts[i + 1]
                                    - args.guest_insts_cycle_counts[i])
                                    as u8,
                                args.guest_insts_cycle_counts[i],
                            );
                    }
                };
                match CPU {
                    CpuType::ARM9 => match args.guest_start_pc & 0xFF000000 {
                        regions::INSTRUCTION_TCM_OFFSET
                        | regions::INSTRUCTION_TCM_MIRROR_OFFSET => {
                            insert_block(self.itcm_info.as_mut_slice())
                        }
                        regions::MAIN_MEMORY_OFFSET => insert_block(self.get_main_info_slice_mut()),
                        0xFF000000 => {
                            if args.guest_start_pc & regions::ARM9_BIOS_OFFSET
                                == regions::ARM9_BIOS_OFFSET
                            {
                                insert_block(self.arm9_bios_info.as_mut_slice())
                            } else {
                                todo!("{:x}", args.guest_start_pc)
                            }
                        }
                        _ => {
                            todo!("{:x}", args.guest_start_pc)
                        }
                    },
                    CpuType::ARM7 => match args.guest_start_pc & 0xFF000000 {
                        regions::MAIN_MEMORY_OFFSET => insert_block(self.get_main_info_slice_mut()),
                        regions::SHARED_WRAM_OFFSET => insert_block(self.wram_info.as_mut_slice()),
                        regions::ARM7_BIOS_OFFSET => {
                            insert_block(self.arm7_bios_info.as_mut_slice())
                        }
                        _ => {
                            todo!("{:x}", args.guest_start_pc)
                        }
                    },
                }

                if DEBUG_LOG {
                    let per = (self.mem_offset * 100) as f32 / JIT_MEMORY_SIZE as f32;
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
    pub fn get_jit_start_addr<const CPU: CpuType>(&self, guest_pc: u32) -> Option<&JitInstInfo> {
        macro_rules! get_info {
            ($blocks:expr) => {{
                let block = &$blocks[guest_pc as usize % $blocks.len()];
                if block.jit_addr != 0 {
                    Some(block)
                } else {
                    None
                }
            }};
        }
        match CPU {
            CpuType::ARM9 => match guest_pc & 0xFF000000 {
                regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => {
                    get_info!(self.itcm_info)
                }
                regions::MAIN_MEMORY_OFFSET => get_info!(self.get_main_info_slice()),
                0xFF000000 => {
                    if guest_pc & regions::ARM9_BIOS_OFFSET == regions::ARM9_BIOS_OFFSET {
                        get_info!(self.arm9_bios_info)
                    } else {
                        todo!("{:x}", guest_pc)
                    }
                }
                _ => {
                    todo!("{:x}", guest_pc)
                }
            },
            CpuType::ARM7 => match guest_pc & 0xFF000000 {
                regions::MAIN_MEMORY_OFFSET => get_info!(self.get_main_info_slice()),
                regions::SHARED_WRAM_OFFSET => get_info!(self.wram_info),
                regions::ARM7_BIOS_OFFSET => get_info!(self.arm7_bios_info),
                _ => {
                    todo!("{:x}", guest_pc)
                }
            },
        }
    }

    #[cfg(target_os = "linux")]
    pub fn open(&self) {}

    #[cfg(target_os = "linux")]
    pub fn close(&self) {}

    #[cfg(target_os = "linux")]
    fn flush_cache(&self, _: u32, _: u32) {}

    #[cfg(target_os = "linux")]
    fn set_rw(&self, start_addr: u32, size: u32) {
        unsafe {
            libc::mprotect(
                (self.mem.as_ptr() as u32 + start_addr) as _,
                size as _,
                libc::PROT_READ | libc::PROT_WRITE,
            )
        };
    }

    #[cfg(target_os = "linux")]
    fn set_rx(&self, start_addr: u32, size: u32) {
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
    fn flush_cache(&self, start_addr: u32, size: u32) {
        unsafe {
            vitasdk_sys::sceKernelSyncVMDomain(
                self.mem.block_uid,
                (self.mem.as_ptr() as u32 + start_addr) as _,
                size,
            )
        };
    }

    #[cfg(target_os = "vita")]
    fn set_rw(&self, _: u32, _: u32) {}

    #[cfg(target_os = "vita")]
    fn set_rx(&self, _r: u32, _: u32) {}
}
