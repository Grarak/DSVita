// Per-backend assemblers live in their own directory; everything below the module
// declarations is backend-neutral (guest-file shapes, block metadata). Other host arches
// run the interpreter until they grow a backend of their own (aarch64 lands in stage 5).
#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(target_arch = "arm")]
pub mod arm32;
// Compatibility re-exports: call sites keep addressing crate::jit::assembler::{arm, ...}.
#[cfg(target_arch = "arm")]
pub use arm32::{arm, block_asm, reg_alloc, thumb, vixl};

/// The host register type each backend allocates from. The arm32 backend's host registers
/// are modelled by the same vixl aarch32 `Reg` as the guest file (host r4-r11 hold the
/// guest pool); a 64-bit backend defines its own.
#[cfg(target_arch = "arm")]
pub type HostReg = crate::jit::reg::Reg;

use crate::jit::op::Op;
use crate::jit::reg::{Reg, RegReserve};
use crate::jit::inst_info::Operands;
use std::ptr;

// Host-register pool shape shared by all backends: 8 pool registers, guest regs id'd by the
// 16-entry guest file. The arm32 backend maps the pool to r4-r11.
pub const GUEST_REGS_LENGTH: usize = 16;
pub const GUEST_REG_POOL_SIZE: usize = 8;

#[derive(Copy, Clone)]
pub struct GuestInstMetadataFastMem {
    pub start_offset: u16,
    pub size: u16,
    pub op: Op,
    pub operands: Operands,
    pub op0: Reg,
    pub opcode_offset: usize,
    pub is_os_irq_handler: bool,
}

impl GuestInstMetadataFastMem {
    fn new(start_offset: u16, size: u16, op: Op, operands: Operands, op0: Reg, opcode_offset: usize, is_os_irq_handler: bool) -> Self {
        GuestInstMetadataFastMem {
            start_offset,
            size,
            op,
            operands,
            op0,
            opcode_offset,
            is_os_irq_handler,
        }
    }
}

#[derive(Copy, Clone)]
pub struct GuestInstMetadataSlowMem {
    pub initial_patch_addr: u32,
    pub io_func: *const (),
}

#[derive(Copy, Clone)]
pub union GuestInstMetadataShared {
    pub fast: GuestInstMetadataFastMem,
    pub slow: GuestInstMetadataSlowMem,
}

impl GuestInstMetadataShared {
    fn new(fast: GuestInstMetadataFastMem) -> Self {
        GuestInstMetadataShared { fast }
    }
}

#[derive(Clone)]
pub struct GuestInstMetadata {
    pub s: GuestInstMetadataShared,
    pub pc: u32,
    pub total_cycle_count: u16,
    pub dirty_guest_regs: RegReserve,
    pub mapped_guest_regs: [Reg; GUEST_REGS_LENGTH],
}

impl GuestInstMetadata {
    pub fn new(
        fast_mem_start_offset: u16,
        fast_mem_size: u16,
        opcode_offset: usize,
        is_os_irq_handler: bool,
        pc: u32,
        total_cycle_count: u16,
        op: Op,
        operands: Operands,
        op0: Reg,
        dirty_guest_regs: RegReserve,
        mapped_guest_regs: [Reg; GUEST_REGS_LENGTH],
    ) -> Self {
        GuestInstMetadata {
            s: GuestInstMetadataShared::new(GuestInstMetadataFastMem::new(fast_mem_start_offset, fast_mem_size, op, operands, op0, opcode_offset, is_os_irq_handler)),
            pc,
            total_cycle_count,
            dirty_guest_regs,
            mapped_guest_regs,
        }
    }
}

#[repr(C)]
pub struct GuestInstOffset {
    pub offset: u16,
    pub pre_cycle_count_sum: u16,
    pub mapping: [*const u32; GUEST_REG_POOL_SIZE],
    pub pc: u32,
}

impl GuestInstOffset {
    fn new(offset: u16, pre_cycle_count_sum: u16, pc: u32) -> Self {
        GuestInstOffset {
            offset,
            mapping: [ptr::null(); GUEST_REG_POOL_SIZE],
            pre_cycle_count_sum,
            pc,
        }
    }
}
