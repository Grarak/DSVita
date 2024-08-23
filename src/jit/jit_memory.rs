use crate::core::emu::{get_contiguous_mem, get_jit, get_jit_mut, get_mem_mut, Emu};
use crate::core::memory::mem::Memory;
use crate::core::memory::{regions, vram};
use crate::core::CpuType;
use crate::logging::debug_println;
use crate::mmap::Mmap;
use crate::utils::HeapMem;
use crate::{utils, DEBUG_LOG};
use paste::paste;
use std::intrinsics::likely;
use std::{mem, ptr};
use CpuType::{ARM7, ARM9};

const JIT_MEMORY_SIZE: u32 = 16 * 1024 * 1024;
const JIT_BLOCK_SIZE_SHIFT: u32 = 8;
pub const JIT_BLOCK_SIZE: u32 = 1 << JIT_BLOCK_SIZE_SHIFT;

#[derive(Default)]
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
            cycles: HeapMem::new(),
        }
    }
}

pub struct JitInsertArgs {
    guest_start_pc: u32,
    guest_pc_to_jit_addr_offset: Vec<u16>,
    guest_insts_cycle_counts: Vec<u16>,
}

impl JitInsertArgs {
    pub fn new(guest_start_pc: u32, guest_pc_to_jit_addr_offset: Vec<u16>, guest_insts_cycle_counts: Vec<u16>) -> Self {
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
    [arm9_bios, regions::ARM9_BIOS_SIZE],
    [main_arm7, regions::MAIN_MEMORY_SIZE],
    [wram, regions::SHARED_WRAM_SIZE + regions::ARM7_WRAM_SIZE],
    [vram_arm7, vram::ARM7_SIZE],
    [arm7_bios, regions::ARM7_BIOS_SIZE]
);

#[cfg(target_os = "linux")]
extern "C" {
    fn built_in_clear_cache(start: *const u8, end: *const u8);
}

static mut NEXT_SEGV_HANDLER: *mut libc::sigaction = ptr::null_mut();
pub static mut EMU_PTR: *mut Emu = ptr::null_mut();

unsafe extern "C" fn sigsegv_handler(sig: i32, si: *mut libc::siginfo_t, segfault_ctx: *mut libc::c_void) {
    let addr = (*si).si_addr();

    let emu = EMU_PTR.as_mut().unwrap_unchecked();
    let host_addr_aligned = utils::align_down(addr as _, JIT_BLOCK_SIZE);
    let guest_addr = get_contiguous_mem!(emu).host_to_guest(addr as u32);
    let mem = get_mem_mut!(emu);
    let prot_lifted = match mem.current_cpu {
        ARM9 => get_jit_mut!(emu).invalidate_block::<{ ARM9 }>(addr as _, guest_addr),
        ARM7 => get_jit_mut!(emu).invalidate_block::<{ ARM7 }>(addr as _, guest_addr),
    };

    // println!("sigsegv {:x} {:x} {guest_addr:x}", addr as u32, utils::align_down(addr as _, get_jit!(emu).page_size));

    mem.breakout_imm = host_addr_aligned == utils::align_down(mem.current_host_block_addr, JIT_BLOCK_SIZE);
    if !prot_lifted {
        let host_page_addr_aligned = utils::align_down(addr as _, get_jit!(emu).page_size);
        libc::mprotect(host_page_addr_aligned as _, get_jit!(emu).page_size as _, libc::PROT_READ | libc::PROT_WRITE);
        mem.host_addrs_to_protect.push(host_page_addr_aligned);
    }

    // if !NEXT_SEGV_HANDLER.is_null() {
    //     let next = (*NEXT_SEGV_HANDLER).sa_sigaction as *const extern "C" fn(sig: i32, si: *mut libc::siginfo_t, segfault_ctx: *mut libc::c_void);
    //     (*next)(sig, si, segfault_ctx);
    // } else {
    //     std::process::exit(1);
    // }
}

pub struct JitMemory {
    mem: Mmap,
    mem_common_offset: u32,
    mem_offset: u32,
    jit_blocks: Box<JitBlocks>,
    pub page_size: u32,
}

impl JitMemory {
    pub fn new() -> Self {
        unsafe {
            let mut sa = mem::zeroed::<libc::sigaction>();
            sa.sa_flags = libc::SA_SIGINFO;
            libc::sigemptyset(&mut sa.sa_mask);
            sa.sa_sigaction = sigsegv_handler as *const () as _;
            if libc::sigaction(libc::SIGSEGV, &sa, NEXT_SEGV_HANDLER) == -1 {
                panic!()
            }
        }

        let mut jit_blocks = unsafe { Box::<JitBlocks>::new_zeroed().assume_init() };
        jit_blocks.init();
        JitMemory {
            mem: Mmap::executable("code", JIT_MEMORY_SIZE).unwrap(),
            mem_common_offset: 0,
            mem_offset: 0,
            jit_blocks,
            page_size: {
                #[cfg(target_os = "linux")]
                {
                    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as _ }
                }
                #[cfg(target_os = "vita")]
                {
                    16
                }
            },
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
        let aligned_size = utils::align_up((opcodes.len() << 2) as u32, self.page_size);
        let allocated_offset_addr = self.allocate_block(aligned_size);

        utils::write_to_mem_slice(&mut self.mem, allocated_offset_addr, opcodes);
        self.flush_cache(allocated_offset_addr, aligned_size);

        (allocated_offset_addr, aligned_size)
    }

    pub fn insert_common(&mut self, opcodes: &[u32]) -> u32 {
        let (allocated_offset_addr, aligned_size) = self.insert(opcodes);

        self.mem_common_offset = allocated_offset_addr + aligned_size;
        debug_println!("Insert new jit ({:x}) block with size {} at {:x}", self.mem.as_ptr() as u32, aligned_size, allocated_offset_addr,);

        allocated_offset_addr + self.mem.as_ptr() as u32
    }

    pub fn insert_block<const CPU: CpuType, const THUMB: bool>(&mut self, opcodes: &[u32], insert_args: JitInsertArgs, mem: &Memory) {
        assert_eq!(insert_args.guest_insts_cycle_counts.len() as u32, JIT_BLOCK_SIZE / if THUMB { 2 } else { 4 });
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
                    cycles.inst_cycle_count = (insert_args.guest_insts_cycle_counts[i] - insert_args.guest_insts_cycle_counts[i - 1]) as u8;
                    cycles.pre_cycle_sum = insert_args.guest_insts_cycle_counts[i] - cycles.inst_cycle_count as u16;
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

        let protect_region = |host_ptr: *const u8| unsafe { libc::mprotect(utils::align_down(host_ptr as _, self.page_size) as _, JIT_BLOCK_SIZE as _, libc::PROT_READ) };

        match CPU {
            ARM9 => match insert_args.guest_start_pc & 0xFF000000 {
                regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => {
                    insert!(itcm);
                    protect_region(mem.tcm.get_itcm_ptr(insert_args.guest_start_pc));
                }
                regions::MAIN_MEMORY_OFFSET => {
                    insert!(main_arm9);
                    protect_region(mem.main.get_ptr(insert_args.guest_start_pc));
                }
                0xFF000000 => insert!(arm9_bios),
                _ => todo!("{:x}", insert_args.guest_start_pc),
            },
            ARM7 => match insert_args.guest_start_pc & 0xFF000000 {
                regions::ARM7_BIOS_OFFSET => insert!(arm7_bios),
                regions::MAIN_MEMORY_OFFSET => {
                    insert!(main_arm7);
                    protect_region(mem.main.get_ptr(insert_args.guest_start_pc));
                }
                regions::SHARED_WRAM_OFFSET => {
                    insert!(wram);
                    protect_region(mem.wram.get_ptr::<{ ARM7 }>(insert_args.guest_start_pc));
                }
                regions::VRAM_OFFSET => {
                    insert!(vram_arm7);
                    protect_region(mem.vram.get_ptr::<{ ARM7 }>(insert_args.guest_start_pc));
                }
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
    pub fn get_jit_start_addr<const CPU: CpuType, const THUMB: bool>(&self, guest_pc: u32, mem: &Memory) -> Option<(u32, u32, u32)> {
        macro_rules! get_addr_block {
            ($blocks:expr, $host_addr:expr) => {{
                let addr_entry = (guest_pc >> JIT_BLOCK_SIZE_SHIFT) as usize;
                let addr_entry = addr_entry % $blocks.len();
                let block = &$blocks[addr_entry];
                if likely(block.jit_addr != 0) {
                    let inst_offset = (guest_pc & (JIT_BLOCK_SIZE - 1)) >> if THUMB { 1 } else { 2 };
                    Some((block.jit_addr + block.offsets[inst_offset as usize] as u32, block as *const _ as u32, $host_addr as u32))
                } else {
                    None
                }
            }};
        }
        macro_rules! get_addr {
            ($block_name:ident, $host_addr:expr) => {
                paste! {
                    if THUMB {
                        get_addr_block!(self.jit_blocks.[<$block_name _ thumb>], $host_addr)
                    } else {
                        get_addr_block!(self.jit_blocks.$block_name, $host_addr)
                    }
                }
            };
        }
        match CPU {
            ARM9 => match guest_pc & 0xFF000000 {
                regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => {
                    get_addr!(itcm, mem.tcm.get_itcm_ptr(guest_pc))
                }
                regions::MAIN_MEMORY_OFFSET => {
                    get_addr!(main_arm9, mem.main.get_ptr(guest_pc))
                }
                0xFF000000 => get_addr!(arm9_bios, 0),
                _ => todo!("{:x} {:x}", guest_pc, guest_pc & 0xFF000000),
            },
            ARM7 => match guest_pc & 0xFF000000 {
                regions::ARM7_BIOS_OFFSET => get_addr!(arm7_bios, 0),
                regions::MAIN_MEMORY_OFFSET => get_addr!(main_arm7, mem.main.get_ptr(guest_pc)),
                regions::SHARED_WRAM_OFFSET => get_addr!(wram, mem.wram.get_ptr::<{ ARM7 }>(guest_pc)),
                regions::VRAM_OFFSET => get_addr!(vram_arm7, mem.vram.get_ptr::<{ ARM7 }>(guest_pc)),
                _ => todo!("{:x} {:x}", guest_pc, guest_pc & 0xFF000000),
            },
        }
    }

    pub fn get_cycle_counts_unchecked<const THUMB: bool>(&self, guest_pc: u32, jit_block_addr: u32) -> (u16, u8) {
        if THUMB {
            let block = unsafe { (jit_block_addr as *const JitBlock<{ JIT_BLOCK_SIZE as usize / 2 }>).as_ref().unwrap_unchecked() };
            let inst_offset = (guest_pc & (JIT_BLOCK_SIZE - 1)) >> 1;
            let cycle = &block.cycles[inst_offset as usize];
            (cycle.pre_cycle_sum, cycle.inst_cycle_count)
        } else {
            let block = unsafe { (jit_block_addr as *const JitBlock<{ JIT_BLOCK_SIZE as usize / 4 }>).as_ref().unwrap_unchecked() };
            let inst_offset = (guest_pc & (JIT_BLOCK_SIZE - 1)) >> 2;
            let cycle = &block.cycles[inst_offset as usize];
            (cycle.pre_cycle_sum, cycle.inst_cycle_count)
        }
    }

    pub fn invalidate_block<const CPU: CpuType>(&mut self, host_addr: u32, guest_addr: u32) -> bool {
        macro_rules! invalidate {
            ($blocks:expr, $blocks_thumb:expr) => {{
                let addr_entry = (guest_addr >> JIT_BLOCK_SIZE_SHIFT) as usize % $blocks.len();
                $blocks[addr_entry].jit_addr = 0;
                $blocks_thumb[addr_entry].jit_addr = 0;

                let block_addr_start = utils::align_down(guest_addr, self.page_size);
                let start_addr_entry = (block_addr_start >> JIT_BLOCK_SIZE_SHIFT) as usize % $blocks.len();
                let mut addr_sum = 0;
                for i in 0..(self.page_size >> JIT_BLOCK_SIZE_SHIFT) {
                    addr_sum |= $blocks[start_addr_entry + i as usize].jit_addr;
                    addr_sum |= $blocks_thumb[start_addr_entry + i as usize].jit_addr;
                }

                if addr_sum == 0 {
                    unsafe { libc::mprotect(utils::align_down(host_addr, self.page_size) as _, self.page_size as _, libc::PROT_READ | libc::PROT_WRITE) };
                    true
                } else {
                    false
                }
            }};
        }

        match CPU {
            ARM9 => match guest_addr & 0xFF000000 {
                regions::INSTRUCTION_TCM_OFFSET | regions::INSTRUCTION_TCM_MIRROR_OFFSET => invalidate!(self.jit_blocks.itcm, self.jit_blocks.itcm_thumb),
                regions::MAIN_MEMORY_OFFSET => {
                    invalidate!(self.jit_blocks.main_arm7, self.jit_blocks.main_arm7_thumb);
                    invalidate!(self.jit_blocks.main_arm9, self.jit_blocks.main_arm9_thumb)
                }
                _ => true,
            },
            ARM7 => match guest_addr & 0xFF000000 {
                regions::MAIN_MEMORY_OFFSET => {
                    invalidate!(self.jit_blocks.main_arm9, self.jit_blocks.main_arm9_thumb);
                    invalidate!(self.jit_blocks.main_arm7, self.jit_blocks.main_arm7_thumb)
                }
                regions::SHARED_WRAM_OFFSET => invalidate!(self.jit_blocks.wram, self.jit_blocks.wram_thumb),
                regions::VRAM_OFFSET => invalidate!(self.jit_blocks.vram_arm7, self.jit_blocks.vram_arm7_thumb),
                _ => true,
            },
        }
    }

    #[cfg(target_os = "linux")]
    pub fn open(&mut self) {}

    #[cfg(target_os = "linux")]
    pub fn close(&mut self) {}

    #[cfg(target_os = "linux")]
    fn flush_cache(&mut self, start_addr: u32, size: u32) {
        unsafe {
            built_in_clear_cache((self.mem.as_ptr() as u32 + start_addr) as _, (self.mem.as_ptr() as u32 + start_addr + size) as _);
        }
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
        unsafe { vitasdk_sys::sceKernelSyncVMDomain(self.mem.block_uid, (self.mem.as_ptr() as u32 + start_addr) as _, size) };
    }
}
