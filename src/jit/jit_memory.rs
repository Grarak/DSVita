use crate::emu::memory::regions;
use crate::emu::CpuType;
use crate::logging::debug_println;
use crate::mmap::Mmap;
use crate::utils::HeapMem;
use crate::{utils, DEBUG_LOG};
use paste::paste;
use std::intrinsics::likely;
use std::{mem, ptr};

const JIT_MEMORY_SIZE: u32 = 16 * 1024 * 1024;
#[cfg(target_os = "linux")]
const JIT_PAGE_SIZE: u32 = 4096;
#[cfg(target_os = "vita")]
const JIT_PAGE_SIZE: u32 = 16;

const JIT_BLOCK_SIZE_SHIFT: u32 = 9;
pub const JIT_BLOCK_SIZE: u32 = 1 << JIT_BLOCK_SIZE_SHIFT;

#[derive(Copy, Clone, Default)]
struct JitCycle {
    pre_cycle_sum: u16,
    inst_cycle_count: u8,
}

struct JitBlock<const SIZE: usize> {
    jit_addr: u32,
    offsets: [u16; SIZE],
    cycles: HeapMem<JitCycle, SIZE>,
}

impl<const SIZE: usize> Default for JitBlock<SIZE> {
    fn default() -> Self {
        JitBlock {
            jit_addr: 0,
            offsets: unsafe { mem::zeroed() },
            cycles: HeapMem::default(),
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

macro_rules! create_jit_blocks {
    ($([$block_name:ident, $size:expr]),+) => {
        paste! {
            struct JitBlocks {
                $(
                    $block_name: [JitBlock<{ JIT_BLOCK_SIZE as usize / 4 }>; $size as usize / JIT_BLOCK_SIZE as usize],
                    [<$block_name _ thumb>]: [JitBlock<{ JIT_BLOCK_SIZE as usize / 2 }>; $size as usize / JIT_BLOCK_SIZE as usize],
                )*
            }

            impl JitBlocks {
                fn init(&mut self) {
                    $(
                        self.$block_name.fill_with(|| { JitBlock::default() });
                        self.[<$block_name _ thumb>].fill_with(|| { JitBlock::default() });
                    )*
                }

                fn reset(&mut self) {
                    $(
                        for block in &mut self.$block_name {
                            block.jit_addr = 0;
                        }
                        for block in &mut self.[<$block_name _ thumb>] {
                            block.jit_addr = 0;
                        }
                    )*
                }
            }
        }
    };
}

create_jit_blocks!(
    [itcm, regions::INSTRUCTION_TCM_SIZE],
    [main_arm9, regions::MAIN_MEMORY_SIZE],
    [wram, (regions::SHARED_WRAM_SIZE + regions::ARM7_WRAM_SIZE)],
    [main_arm7, regions::MAIN_MEMORY_SIZE],
    [arm9_bios, regions::ARM9_BIOS_SIZE],
    [arm7_bios, regions::ARM7_BIOS_SIZE]
);

pub struct JitMemory {
    mem: Mmap,
    mem_common_offset: u32,
    mem_offset: u32,
    jit_blocks: Box<JitBlocks>,
}

impl JitMemory {
    pub fn new() -> Self {
        let mut jit_blocks = unsafe { Box::<JitBlocks>::new_zeroed().assume_init() };
        jit_blocks.init();
        JitMemory {
            mem: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            mem_common_offset: 0,
            mem_offset: 0,
            jit_blocks,
        }
    }

    fn allocate_block(&mut self, required_size: u32) -> u32 {
        if self.mem_offset + required_size >= JIT_MEMORY_SIZE {
            self.mem_offset = self.mem_common_offset;
            self.jit_blocks.reset();
            let addr = self.mem_offset;
            self.mem_offset += required_size;
            addr
        } else {
            let addr = self.mem_offset;
            self.mem_offset += required_size;
            addr
        }
    }

    fn insert(&mut self, opcodes: &[u32]) -> (u32, u32) {
        let aligned_size = utils::align_up((opcodes.len() << 2) as u32, JIT_PAGE_SIZE);
        let allocated_offset_addr = self.allocate_block(aligned_size);

        self.set_rw(allocated_offset_addr, aligned_size);
        utils::write_to_mem_slice(&mut self.mem, allocated_offset_addr, opcodes);
        self.flush_cache(allocated_offset_addr, aligned_size);
        self.set_rx(allocated_offset_addr, aligned_size);

        (allocated_offset_addr, aligned_size)
    }

    pub fn insert_common(&mut self, opcodes: &[u32]) -> u32 {
        let (allocated_offset_addr, aligned_size) = self.insert(opcodes);

        self.mem_common_offset = allocated_offset_addr + aligned_size;
        debug_println!(
            "Insert new jit ({:x}) block with size {} at {:x}",
            self.mem.as_ptr() as u32,
            aligned_size,
            allocated_offset_addr,
        );

        allocated_offset_addr + self.mem.as_ptr() as u32
    }

    pub fn insert_block<const CPU: CpuType, const THUMB: bool>(
        &mut self,
        opcodes: &[u32],
        insert_args: JitInsertArgs,
    ) {
        assert_eq!(
            insert_args.guest_insts_cycle_counts.len() as u32,
            JIT_BLOCK_SIZE / if THUMB { 2 } else { 4 }
        );
        let (allocated_offset_addr, aligned_size) = self.insert(opcodes);

        let addr_entry = (insert_args.guest_start_pc >> JIT_BLOCK_SIZE_SHIFT) as usize;
        macro_rules! insert_to_block {
            ($blocks:expr, $cycles:expr) => {{
                let addr_entry = addr_entry % $blocks.len();
                let block = &mut $blocks[addr_entry];
                block.jit_addr = allocated_offset_addr + self.mem.as_ptr() as u32;
                block.offsets[0] = insert_args.guest_pc_to_jit_addr_offset[0];
                block.cycles[0].pre_cycle_sum = 0;
                block.cycles[0].inst_cycle_count = insert_args.guest_insts_cycle_counts[0] as u8;
                for i in 1..insert_args.guest_pc_to_jit_addr_offset.len() {
                    block.offsets[i] = insert_args.guest_pc_to_jit_addr_offset[i];
                    let cycles = &mut block.cycles[i];
                    cycles.inst_cycle_count = (insert_args.guest_insts_cycle_counts[i]
                        - insert_args.guest_insts_cycle_counts[i - 1])
                        as u8;
                    cycles.pre_cycle_sum =
                        insert_args.guest_insts_cycle_counts[i] - cycles.inst_cycle_count as u16;
                }
            }};
        }
        macro_rules! insert {
            ($block_name:ident) => {
                paste! {
                    if THUMB {
                        insert_to_block!(self.jit_blocks.[<$block_name _ thumb>], self.jit_cycles.[<$block_name _ thumb>])
                    } else {
                        insert_to_block!(self.jit_blocks.$block_name, self.jit_cycles.$block_name)
                    }
                }
            };
        }
        match CPU {
            CpuType::ARM9 => match insert_args.guest_start_pc & 0xFF000000 {
                regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => {
                    insert!(itcm)
                }
                regions::MAIN_MEMORY_OFFSET => insert!(main_arm9),
                0xFF000000 => insert!(arm9_bios),
                _ => todo!("{:x}", insert_args.guest_start_pc),
            },
            CpuType::ARM7 => match insert_args.guest_start_pc & 0xFF000000 {
                regions::ARM7_BIOS_OFFSET => insert!(arm7_bios),
                regions::MAIN_MEMORY_OFFSET => insert!(main_arm7),
                regions::SHARED_WRAM_OFFSET => insert!(wram),
                _ => todo!("{:x}", insert_args.guest_start_pc),
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
                    insert_args.guest_start_pc
                );
        }
    }

    #[inline(always)]
    pub fn get_jit_start_addr<const CPU: CpuType, const THUMB: bool>(
        &self,
        guest_pc: u32,
    ) -> Option<(u32, u32)> {
        macro_rules! get_addr_block {
            ($blocks:expr) => {{
                let addr_entry = (guest_pc >> JIT_BLOCK_SIZE_SHIFT) as usize;
                let addr_entry = addr_entry % $blocks.len();
                let block = &$blocks[addr_entry];
                if likely(block.jit_addr != 0) {
                    let inst_offset =
                        (guest_pc & (JIT_BLOCK_SIZE - 1)) >> if THUMB { 1 } else { 2 };
                    Some((
                        block.jit_addr + block.offsets[inst_offset as usize] as u32,
                        block as *const _ as u32,
                    ))
                } else {
                    None
                }
            }};
        }
        macro_rules! get_addr {
            ($block_name:ident) => {
                paste! {
                    if THUMB {
                        get_addr_block!(self.jit_blocks.[<$block_name _ thumb>])
                    } else {
                        get_addr_block!(self.jit_blocks.$block_name)
                    }
                }
            };
        }
        match CPU {
            CpuType::ARM9 => match guest_pc & 0xFF000000 {
                regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => {
                    get_addr!(itcm)
                }
                regions::MAIN_MEMORY_OFFSET => {
                    get_addr!(main_arm9)
                }
                0xFF000000 => get_addr!(arm9_bios),
                _ => todo!("{:x} {:x}", guest_pc, guest_pc & 0xFF000000),
            },
            CpuType::ARM7 => match guest_pc & 0xFF000000 {
                regions::ARM7_BIOS_OFFSET => get_addr!(arm7_bios),
                regions::MAIN_MEMORY_OFFSET => get_addr!(main_arm7),
                regions::SHARED_WRAM_OFFSET => get_addr!(wram),
                _ => todo!("{:x} {:x}", guest_pc, guest_pc & 0xFF000000),
            },
        }
    }

    pub fn get_cycle_counts_unchecked<const THUMB: bool>(
        &self,
        guest_pc: u32,
        jit_block_addr: u32,
    ) -> (u16, u8) {
        if THUMB {
            let block = unsafe {
                (jit_block_addr as *const JitBlock<{ JIT_BLOCK_SIZE as usize / 2 }>)
                    .as_ref()
                    .unwrap_unchecked()
            };
            let inst_offset = (guest_pc & (JIT_BLOCK_SIZE - 1)) >> 1;
            let cycle = &block.cycles[inst_offset as usize];
            (cycle.pre_cycle_sum, cycle.inst_cycle_count)
        } else {
            let block = unsafe {
                (jit_block_addr as *const JitBlock<{ JIT_BLOCK_SIZE as usize / 4 }>)
                    .as_ref()
                    .unwrap_unchecked()
            };
            let inst_offset = (guest_pc & (JIT_BLOCK_SIZE - 1)) >> 2;
            let cycle = &block.cycles[inst_offset as usize];
            (cycle.pre_cycle_sum, cycle.inst_cycle_count)
        }
    }

    pub fn invalidate_block<const CPU: CpuType>(&mut self, addr: u32, size: u32) -> (u32, u32) {
        macro_rules! invalidate {
            ($blocks:expr, $blocks_thumb:expr) => {{
                let addr_entry = (addr >> JIT_BLOCK_SIZE_SHIFT) as usize % $blocks.len();
                let addr_entry_end =
                    ((addr + size - 1) >> JIT_BLOCK_SIZE_SHIFT) as usize % $blocks.len();
                $blocks[addr_entry].jit_addr = 0;
                $blocks[addr_entry_end].jit_addr = 0;
                $blocks_thumb[addr_entry].jit_addr = 0;
                $blocks_thumb[addr_entry_end].jit_addr = 0;
                (
                    ptr::addr_of!($blocks[addr_entry]) as u32,
                    ptr::addr_of!($blocks_thumb[addr_entry]) as u32,
                )
            }};
        }
        match CPU {
            CpuType::ARM9 => match addr & 0xFF000000 {
                regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => {
                    invalidate!(self.jit_blocks.itcm, self.jit_blocks.itcm_thumb)
                }
                regions::MAIN_MEMORY_OFFSET => {
                    invalidate!(self.jit_blocks.main_arm7, self.jit_blocks.main_arm7_thumb);
                    invalidate!(self.jit_blocks.main_arm9, self.jit_blocks.main_arm9_thumb)
                }
                _ => (0, 0),
            },
            CpuType::ARM7 => match addr & 0xFF000000 {
                regions::MAIN_MEMORY_OFFSET => {
                    invalidate!(self.jit_blocks.main_arm9, self.jit_blocks.main_arm9_thumb);
                    invalidate!(self.jit_blocks.main_arm7, self.jit_blocks.main_arm7_thumb)
                }
                regions::SHARED_WRAM_OFFSET => {
                    invalidate!(self.jit_blocks.wram, self.jit_blocks.wram_thumb)
                }
                _ => (0, 0),
            },
        }
    }

    #[cfg(target_os = "linux")]
    pub fn open(&mut self) {}

    #[cfg(target_os = "linux")]
    pub fn close(&mut self) {}

    #[cfg(target_os = "linux")]
    fn flush_cache(&mut self, _: u32, _: u32) {}

    #[cfg(target_os = "linux")]
    fn set_rw(&mut self, start_addr: u32, size: u32) {
        unsafe {
            libc::mprotect(
                (self.mem.as_ptr() as u32 + start_addr) as _,
                size as _,
                libc::PROT_READ | libc::PROT_WRITE,
            )
        };
    }

    #[cfg(target_os = "linux")]
    fn set_rx(&mut self, start_addr: u32, size: u32) {
        unsafe {
            libc::mprotect(
                (self.mem.as_ptr() as u32 + start_addr) as _,
                size as _,
                libc::PROT_READ | libc::PROT_EXEC,
            )
        };
    }

    #[cfg(target_os = "vita")]
    pub fn open(&mut self) {
        unsafe { vitasdk_sys::sceKernelOpenVMDomain() };
    }

    #[cfg(target_os = "vita")]
    pub fn close(&mut self) {
        unsafe { vitasdk_sys::sceKernelCloseVMDomain() };
    }

    #[cfg(target_os = "vita")]
    fn flush_cache(&mut self, start_addr: u32, size: u32) {
        unsafe {
            vitasdk_sys::sceKernelSyncVMDomain(
                self.mem.block_uid,
                (self.mem.as_ptr() as u32 + start_addr) as _,
                size,
            )
        };
    }

    #[cfg(target_os = "vita")]
    fn set_rw(&mut self, _: u32, _: u32) {}

    #[cfg(target_os = "vita")]
    fn set_rx(&mut self, _r: u32, _: u32) {}
}
