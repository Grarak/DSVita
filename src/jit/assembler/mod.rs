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

use crate::jit::inst_info::Operands;
use crate::jit::op::Op;
use crate::jit::reg::{Reg, RegReserve};
use std::ptr;

// Host-register pool shape shared by all backends: 8 pool registers, guest regs id'd by the
// 16-entry guest file. The arm32 backend maps the pool to r4-r11, the a64 one to x19-x26.
pub const GUEST_REGS_LENGTH: usize = 16;
#[cfg(target_arch = "arm")]
pub const GUEST_REG_POOL_SIZE: usize = 8;
// x19-x26 + x28: every callee-saved register except the pinned ThreadRegs base (x27)
// and fp/lr — the spill/restore fabric addresses guest slots through x27 on every
// access, so it stays out of the pool.
#[cfg(not(target_arch = "arm"))]
pub const GUEST_REG_POOL_SIZE: usize = 9;

/// The host register type each backend allocates from — also the element type of the
/// shared metadata's mapped_guest_regs (slow-mem patching routes values between pool
/// registers and the handlers through it). arm32 models host registers with the same
/// vixl aarch32 `Reg` as the guest file; the a64 backend uses A64Reg.
#[cfg(target_arch = "arm")]
pub type HostReg = Reg;
#[cfg(not(target_arch = "arm"))]
pub type HostReg = vixl::A64Reg;
#[cfg(target_arch = "arm")]
pub const HOST_REG_NONE: HostReg = Reg::None;
#[cfg(not(target_arch = "arm"))]
pub const HOST_REG_NONE: HostReg = vixl::A64Reg::ZR;

/// Index of a mapped host register within the pool dump the breakout/writeback paths
/// capture (arm32: r4-r11 pushed in order; a64: x19-x26).
#[cfg(target_arch = "arm")]
pub fn host_pool_index(reg: HostReg) -> usize {
    reg as usize - 4
}
#[cfg(not(target_arch = "arm"))]
pub fn host_pool_index(reg: HostReg) -> usize {
    if reg == vixl::A64Reg::X28 {
        8
    } else {
        reg as usize - vixl::A64Reg::X19 as usize
    }
}

#[derive(Copy, Clone)]
pub struct GuestInstMetadataFastMem {
    pub start_offset: u16,
    pub size: u16,
    pub op: Op,
    pub operands: Operands,
    pub op0: HostReg,
    pub opcode_offset: usize,
    pub is_os_irq_handler: bool,
}

impl GuestInstMetadataFastMem {
    fn new(start_offset: u16, size: u16, op: Op, operands: Operands, op0: HostReg, opcode_offset: usize, is_os_irq_handler: bool) -> Self {
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
    pub mapped_guest_regs: [HostReg; GUEST_REGS_LENGTH],
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
        op0: HostReg,
        dirty_guest_regs: RegReserve,
        mapped_guest_regs: [HostReg; GUEST_REGS_LENGTH],
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
