use crate::hle::memory::regions;
use crate::hle::CpuType;
use crate::logging::debug_println;
use crate::mmap::Mmap;
use crate::utils::HeapMem;
use crate::{utils, DEBUG_LOG};
use bilge::prelude::*;
use core::slice;
use std::intrinsics::likely;
use std::mem;

const JIT_MEMORY_SIZE: u32 = 16 * 1024 * 1024;
#[cfg(target_os = "linux")]
const JIT_PAGE_SIZE: u32 = 4096;
#[cfg(target_os = "vita")]
const JIT_PAGE_SIZE: u32 = 16;

#[bitsize(32)]
#[derive(FromBits)]
struct AddrCycleInfo {
    jit_add_offset: u24,
    cycle_count: u8,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Default)]
pub struct JitInstInfo {
    inst_info: u32,
    pre_cycle_count_sum: u16, // Sum of cycle counts of block up until this instruction
}

impl JitInstInfo {
    fn new(jit_addr_offset: u32, cycle_count: u8, pre_cycle_count_sum: u16) -> Self {
        JitInstInfo {
            inst_info: AddrCycleInfo::new(u24::new(jit_addr_offset), cycle_count).into(),
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
        if self.mem_offset + required_size >= JIT_MEMORY_SIZE {
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
        }
    }

    pub fn insert_block<const CPU: CpuType, const THUMB: bool>(
        &mut self,
        opcodes: &[u32],
        args: Option<JitInsertArgs>,
    ) -> u32 {
        let aligned_size = utils::align_up((opcodes.len() << 2) as u32, JIT_PAGE_SIZE);
        let allocated_offset_addr = self.allocate_block(aligned_size);

        self.set_rw(allocated_offset_addr, aligned_size);
        utils::write_to_mem_slice(&mut self.mem, allocated_offset_addr, opcodes);
        self.flush_cache(allocated_offset_addr, aligned_size);
        self.set_rx(allocated_offset_addr, aligned_size);

        match args {
            Some(args) => {
                let insert_block = |mem: &mut [JitInstInfo]| {
                    let start_addr = args.guest_start_pc as usize % mem.len();
                    mem[start_addr] = JitInstInfo::new(
                        allocated_offset_addr,
                        args.guest_insts_cycle_counts[0] as u8,
                        0,
                    );
                    for i in 0..args.guest_pc_to_jit_addr_offset.len() {
                        mem[(start_addr + ((i + 1) << if THUMB { 1 } else { 2 })) % mem.len()] =
                            JitInstInfo::new(
                                allocated_offset_addr + args.guest_pc_to_jit_addr_offset[i] as u32,
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
                        0xFF000000 => insert_block(self.arm9_bios_info.as_mut_slice()),
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
                    self.mem.as_ptr() as u32,
                    aligned_size,
                    allocated_offset_addr,
                    per,
                    args.guest_start_pc
                );
                }
            }
            None => {
                // Inserts without args is common code, happens on initialization, adjust offset
                self.mem_common_offset = allocated_offset_addr + aligned_size;
                debug_println!(
                    "Insert new jit ({:x}) block with size {} at {:x}",
                    self.mem.as_ptr() as u32,
                    aligned_size,
                    allocated_offset_addr,
                );
            }
        }
        allocated_offset_addr + self.mem.as_ptr() as u32
    }

    #[inline]
    pub fn get_jit_start_addr<const CPU: CpuType>(&self, guest_pc: u32) -> Option<(u32, u8, u16)> {
        macro_rules! get_info {
            ($blocks:expr) => {{
                let block = &$blocks[guest_pc as usize % $blocks.len()];
                // Just check if inst info is not 0
                // Addr offset can't be at 0 since we always have common functions which allocate the first couple blocks in mem
                if likely(block.inst_info != 0) {
                    let info = AddrCycleInfo::from(block.inst_info);
                    Some((u32::from(info.jit_add_offset()) + self.mem.as_ptr() as u32, info.cycle_count(), block.pre_cycle_count_sum))
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
                    get_info!(self.arm9_bios_info)
                }
                _ => todo!("{:x}", guest_pc),
            },
            CpuType::ARM7 => match guest_pc & 0xFF000000 {
                regions::MAIN_MEMORY_OFFSET => get_info!(self.get_main_info_slice()),
                regions::SHARED_WRAM_OFFSET => get_info!(self.wram_info),
                regions::ARM7_BIOS_OFFSET => get_info!(self.arm7_bios_info),
                _ => todo!("{:x}", guest_pc),
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
