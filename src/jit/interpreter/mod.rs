// Cold-block interpreter.
//
// Every uncompiled guest address dispatches to `emit_code_block`. Compiling a block is expensive
// and wasteful for code that only runs a handful of times. This interpreter executes cold blocks
// directly and only lets `emit_code_block` compile once an address has been seen enough times
// (see INTERP_THRESHOLD).
//
// Dispatch is a flat `[ArmInterpFn; 4096]` lookup table (arm_table.rs, generated from the
// disassembler's own lookup table layout) indexed by `((op>>16)&0xFF0)|((op>>4)&0xF)`, exactly
// like the disassembler and NooDS' `armInstrs`. Every entry points at a fully specialized handler
// (`and_lli`, `ldr_ofip`, `strh_ptrm`, `mul`, ...) generated with const generics + a `paste` macro
// (mirroring the disassembler's delegations.rs), so there is no operation/addressing/shift matching
// on the execution path. Thumb dispatches the same way through a flat `[ThumbInterpFn; 1024]`
// table (thumb_table.rs, generated from lookup_table_thumb.rs) indexed by `op>>6`. Semantics are a
// Rust port of NooDS/src/interpreter_*.cpp.
//
// Handback model: "one block then call next entry". We interpret straight-line instructions until
// a branch (or a PC write), then hand control to the next guest address' jit entry via
// `call_jit_fun`, exactly like a compiled block would. Anything not (yet) interpreted maps to
// `inst_fallback`, which compiles the block, so correctness holds regardless of coverage.

mod alu;
mod branch;
mod memory;
mod thumb_alu;
mod thumb_branch;
mod thumb_memory;

use crate::core::memory::mmu::MMU_PAGE_SIZE;
use crate::core::memory::regions;
use crate::core::thread_regs::ThreadRegs;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::inst_branch_handler::{branch_lr, branch_reg, breakout_imm, call_jit_fun, check_scheduler, check_stack_depth};
use crate::jit::jit_asm::JitAsm;
use crate::jit::reg::Reg;
use crate::settings::Arm7Emu;
use crate::{utils, DEBUG_LOG, IS_DEBUG};
use std::hint::assert_unchecked;
use std::intrinsics::{likely, unlikely};

// Bring every specialized handler into scope so the flat tables can reference them by bare name.
use alu::*;
use branch::*;
use memory::*;
use thumb_alu::*;
use thumb_branch::*;
use thumb_memory::*;

// Threshold of executions of a cold address before it gets compiled to a jit block.
// 255 = always interpret (counter saturates), 0 = always compile. Useful for testing.
#[cfg(not(target_arch = "arm"))]
pub mod fallback;

pub const INTERP_THRESHOLD: u8 = 100;

// CPSR flag bits.
pub(super) const N_BIT: u32 = 1 << 31;
pub(super) const Z_BIT: u32 = 1 << 30;
pub(super) const C_BIT: u32 = 1 << 29;
pub(super) const V_BIT: u32 = 1 << 28;
pub(super) const T_BIT: u32 = 1 << 5;

macro_rules! mem_read {
    ($asm:expr, $ty:ty, $addr:expr) => {
        match $asm.cpu {
            ARM9 => $asm.emu.mem_read::<{ ARM9 }, $ty>($addr),
            ARM7 => $asm.emu.mem_read::<{ ARM7 }, $ty>($addr),
        }
    };
}

macro_rules! mem_write {
    ($asm:expr, $ty:ty, $addr:expr, $value:expr) => {
        match $asm.cpu {
            ARM9 => $asm.emu.mem_write::<{ ARM9 }, $ty>($addr, $value),
            ARM7 => $asm.emu.mem_write::<{ ARM7 }, $ty>($addr, $value),
        }
    };
}

// Block-transfer variants: one region/mmu resolve for the whole register list, like the jit's
// handle_multiple_request slow path.
macro_rules! mem_read_multiple {
    ($asm:expr, $addr:expr, $slice:expr) => {
        match $asm.cpu {
            ARM9 => $asm.emu.mem_read_multiple_slice::<{ ARM9 }, true, true, u32>($addr, $slice),
            ARM7 => $asm.emu.mem_read_multiple_slice::<{ ARM7 }, true, true, u32>($addr, $slice),
        }
    };
}

macro_rules! mem_write_multiple {
    ($asm:expr, $addr:expr, $slice:expr) => {
        match $asm.cpu {
            ARM9 => $asm.emu.mem_write_multiple_slice::<{ ARM9 }, true, u32>($addr, $slice),
            ARM7 => $asm.emu.mem_write_multiple_slice::<{ ARM7 }, true, u32>($addr, $slice),
        }
    };
}

pub(super) use mem_read;
pub(super) use mem_read_multiple;
pub(super) use mem_write;
pub(super) use mem_write_multiple;

/// Per instruction outcome.
pub(super) enum InstResult {
    /// Instruction executed, continue with the following instruction. Carries cycle count.
    Continue(u16),
    /// Like `Continue`, but the instruction stored to memory: the write may have requested an
    /// immediate breakout (gx fifo full / renderer sync), so the dispatch loop checks
    /// `emu.breakout_imm` — only for stores, mirroring the jit's write handlers.
    ContinueStore(u16),
    /// A branch / PC write happened, hand control to `target` (guest pc with thumb bit in bit0).
    Branch(u16, u32),
    /// A linking branch (bl/blx): push the return stack and native-call the target via the jit's
    /// `branch_reg`, then resume interpreting at the return address `lr` (the value written to the
    /// guest LR, mode-tagged in thumb) — exactly like a compiled block's call site.
    BranchLink(u16, u32, u32),
    /// A returning branch (`bx lr`, `mov pc, lr`, `pop {.., pc}`, `ldr pc, [sp..]` — the shapes the
    /// jit routes to `branch_lr`): pop the return stack; on a match control goes back natively to
    /// whoever called this block (a compiled call site or an interpreter BranchLink frame).
    BranchReturn(u16, u32),
    /// Not (yet) interpreted, compile the block instead.
    Fallback,
}

/// Execution context for the instruction currently being interpreted.
pub(super) struct Ctx<'a, 'b> {
    pub(super) asm: &'a mut JitAsm<'b>,
    pub(super) cpu: CpuType,
    /// The cpu's ThreadRegs, resolved once per block: register accesses index it directly
    /// instead of re-selecting the per-cpu base pointer on every read/write.
    pub(super) regs: *mut ThreadRegs,
    /// Address of the instruction currently executing (no thumb bit).
    pub(super) inst_addr: u32,
}

impl<'a, 'b> Ctx<'a, 'b> {
    /// `gp_regs` through `spsr` are one contiguous `repr(C)` block of u32s, indexed exactly like
    /// the `Reg` enum (R0..R12, SP=13, LR=14, PC=15, CPSR=16, SPSR=17) — the same layout trick
    /// `thread_get_reg` relies on.
    #[inline]
    fn reg_slot(&self, index: u32) -> *mut u32 {
        debug_assert!(index <= Reg::SPSR as u32);
        unsafe { std::ptr::addr_of_mut!((*self.regs).gp_regs[0]).add(index as usize) }
    }

    #[inline]
    pub(super) fn reg(&self, index: u32) -> u32 {
        if index == 15 {
            // ARM pipeline: reading PC yields the address of the current instruction + 8.
            self.inst_addr + 8
        } else {
            unsafe { *self.reg_slot(index) }
        }
    }

    #[inline]
    pub(super) fn set_reg(&mut self, index: u32, value: u32) {
        unsafe { *self.reg_slot(index) = value };
    }

    #[inline]
    pub(super) fn cpsr(&self) -> u32 {
        unsafe { (*self.regs).cpsr }
    }

    /// Overwrite only the flag bits (NZCVQ); never changes mode, so no register banking needed.
    #[inline]
    pub(super) fn set_cpsr(&mut self, value: u32) {
        unsafe { (*self.regs).cpsr = value };
    }

    #[inline]
    pub(super) fn carry(&self) -> u32 {
        (self.cpsr() & C_BIT != 0) as u32
    }

    /// Thumb register read: the pipeline makes PC read as the current instruction + 4.
    #[inline]
    pub(super) fn reg_t(&self, index: u32) -> u32 {
        if index == 15 {
            self.inst_addr + 4
        } else {
            unsafe { *self.reg_slot(index) }
        }
    }
}

type ArmInterpFn = fn(&mut Ctx<'_, '_>, u32) -> InstResult;
type ThumbInterpFn = fn(&mut Ctx<'_, '_>, u16) -> InstResult;

#[inline]
fn arm_index(opcode: u32) -> usize {
    (((opcode >> 16) & 0xFF0) | ((opcode >> 4) & 0xF)) as usize
}

include!("arm_table.rs");
include!("thumb_table.rs");

/// Condition lookup, copied from NooDS (interpreter_lookup.cpp). Indexed by
/// `((opcode >> 24) & 0xF0) | (cpsr >> 28)` = cond << 4 | NZCV. 0 = skip, 1 = execute,
/// 2 = reserved (cond 0xF, the unconditional extension space).
#[rustfmt::skip]
static CONDITION: [u8; 0x100] = [
    0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, // EQ
    1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, // NE
    0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, // CS
    1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, // CC
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, // MI
    1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, // PL
    0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, // VS
    1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, // VC
    0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, // HI
    1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, // LS
    1, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0, 1, // GE
    0, 1, 0, 1, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0, // LT
    1, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, // GT
    0, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 1, 1, 1, 1, // LE
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // AL
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, // Reserved
];

// Last instruction each interpreter dispatched, for the panic hook: when a guest bug (or an
// interpreter bug) trips an assert deep inside a memory handler, this pins down the faulting
// guest instruction. Only maintained when IS_DEBUG (compiles out of release builds).
pub static mut LAST_INTERPRETED: [(u32, u32); 2] = [(0, 0); 2];

pub fn print_last_interpreted() {
    let last = unsafe { &*(&raw const LAST_INTERPRETED) };
    for (cpu, (addr, opcode)) in [ARM9, ARM7].into_iter().zip(last) {
        if *addr != 0 {
            let regs = cpu.thread_regs();
            println!("{cpu:?} last interpreted inst at {:x} opcode {opcode:x} (thumb: {})", addr & !1, addr & 1);
            println!("{cpu:?} gp regs {:x?} sp {:x} lr {:x} pc {:x} cpsr {:x}", regs.gp_regs, regs.sp, regs.lr, regs.pc, regs.cpsr);
        }
    }
}

pub(super) fn inst_fallback(_ctx: &mut Ctx, _opcode: u32) -> InstResult {
    InstResult::Fallback
}

pub(super) fn inst_fallback_t(_ctx: &mut Ctx, _opcode: u16) -> InstResult {
    InstResult::Fallback
}

/// Entry point from `emit_code_block_internal`. Interprets the cold block starting at `guest_pc`
/// (aligned, no thumb bit) and hands control to the next entry. Returns false only when the very
/// first instruction can't be interpreted, in which case the caller compiles the block instead.
pub fn interpret_block(asm: &mut JitAsm, guest_pc: u32, thumb: bool) -> bool {
    if thumb {
        interpret_block_inner::<true>(asm, guest_pc)
    } else {
        interpret_block_inner::<false>(asm, guest_pc)
    }
}

fn interpret_block_inner<const THUMB: bool>(asm: &mut JitAsm, guest_pc: u32) -> bool {
    let cpu = asm.cpu;
    let arm7_hle = asm.emu.settings.arm7_emu() == Arm7Emu::Hle;
    let regs: *mut ThreadRegs = cpu.thread_regs();
    let mut addr = guest_pc;
    let mut executed_any = false;

    let step: u32 = if THUMB { 2 } else { 4 };
    // Sequential opcode fetch: resolve the shm offset once and bump it alongside `addr`, instead
    // of paying the full generic mem_read (per-cpu select + mmu walk) on every instruction. Only
    // re-resolved on an mmu-page crossing or a non-sequential jump. Stores stay coherent (they
    // write the same shm backing), and an mmu remap mid-run matches the jit, whose compiled
    // blocks don't re-fetch either. `code_shm_offset == 0` = unmapped, fetch through mem_read.
    let mut code_shm_offset: u32 = 0;
    let mut page_left: u32 = 0;

    loop {
        if unlikely(page_left == 0) {
            let masked = addr & 0x0FFFFFFF;
            code_shm_offset = match cpu {
                ARM9 => asm.emu.get_shm_offset::<{ ARM9 }, true, false>(masked),
                ARM7 => asm.emu.get_shm_offset::<{ ARM7 }, true, false>(masked),
            } as u32;
            page_left = if code_shm_offset == 0 { step } else { MMU_PAGE_SIZE as u32 - (addr & (MMU_PAGE_SIZE as u32 - 1)) };
        }

        let (result, opcode) = if THUMB {
            // Thumb has no condition field; conditional branches check the flags themselves.
            let opcode = if likely(code_shm_offset != 0) {
                utils::read_from_mem::<u16>(&asm.emu.mem.shm, code_shm_offset)
            } else {
                mem_read!(asm, u16, addr)
            };
            if IS_DEBUG {
                unsafe { LAST_INTERPRETED[cpu as usize] = (addr | 1, opcode as u32) };
            }
            let mut ctx = Ctx {
                cpu,
                regs,
                inst_addr: addr,
                asm: &mut *asm,
            };
            (THUMB_TABLE[(opcode >> 6) as usize](&mut ctx, opcode), opcode as u32)
        } else {
            let opcode = if likely(code_shm_offset != 0) {
                utils::read_from_mem::<u32>(&asm.emu.mem.shm, code_shm_offset)
            } else {
                mem_read!(asm, u32, addr)
            };
            if IS_DEBUG {
                unsafe { LAST_INTERPRETED[cpu as usize] = (addr, opcode) };
            }

            let result = if opcode >= 0xE000_0000 {
                if likely(opcode < 0xF000_0000) {
                    // AL — the dominant case: skip the cpsr load and the condition table.
                    let mut ctx = Ctx {
                        cpu,
                        regs,
                        inst_addr: addr,
                        asm: &mut *asm,
                    };
                    ARM_TABLE[arm_index(opcode)](&mut ctx, opcode)
                } else {
                    // Reserved (cond 0xF): unconditional extension space (BLX imm, PLD, ...), let
                    // the jit handle it (the table index ignores the condition bits, so it can't
                    // classify these).
                    InstResult::Fallback
                }
            } else {
                let cpsr = unsafe { (*regs).cpsr };
                match CONDITION[(((opcode >> 24) & 0xF0) | (cpsr >> 28)) as usize] {
                    // Condition failed, skip but still spend a cycle.
                    0 => InstResult::Continue(1),
                    _ => {
                        let mut ctx = Ctx {
                            cpu,
                            regs,
                            inst_addr: addr,
                            asm: &mut *asm,
                        };
                        ARM_TABLE[arm_index(opcode)](&mut ctx, opcode)
                    }
                }
            };
            (result, opcode)
        };

        // Log the executed instruction's post-state, like the jit's debug_after_exec_op. Fallback
        // instructions aren't executed here (the jit logs them when it compiles the block), and
        // taken branches aren't logged because the jit's emitted code transfers control before its
        // debug hook runs — logging them here would break trace diffing against a jit run.
        if DEBUG_LOG && matches!(result, InstResult::Continue(_) | InstResult::ContinueStore(_)) {
            crate::debug_inst_log::log(asm.emu, cpu, addr, opcode);
        }

        match result {
            InstResult::Continue(cycles) => {
                asm.runtime_data.accumulated_cycles += cycles;
                executed_any = true;
                addr += step;
                code_shm_offset += step;
                page_left -= step;
            }
            InstResult::ContinueStore(cycles) => {
                asm.runtime_data.accumulated_cycles += cycles;
                executed_any = true;
                addr += step;
                code_shm_offset += step;
                page_left -= step;

                // The store may have requested an immediate breakout (gx fifo full / renderer
                // sync): run the scheduler right away, like the jit's write handlers do. Only
                // stores can set the flag, so non-store instructions skip the check entirely.
                // No scheduler check otherwise: the jit only checks at taken branches, and
                // checking mid-straight-line here would skew irq timing between the engines.
                if unlikely(asm.emu.breakout_imm) {
                    run_breakout_imm::<THUMB>(asm, addr);
                }
            }
            InstResult::Branch(cycles, target) => {
                // +2 is the jit's per-block branch epilogue cost (emit_count_cycles); the
                // interpreted straight-line run ends here, so it's charged the same way.
                asm.runtime_data.accumulated_cycles += cycles + 2;
                // Set the resume point before the scheduler runs, so a pending interrupt returns
                // to the branch target (thumb bit carried in bit0), like a compiled branch.
                unsafe { (*regs).pc = target };
                run_scheduler(asm, addr, arm7_hle);

                // Stay in this flat loop when the target is another cold same-mode block: a
                // handback would recurse a host frame per block (compiled branches tail-call
                // instead) and pay the whole dispatch chain per iteration — pathological for
                // interpreted loops. The hotness counter still ticks so the threshold flip to
                // the jit is unchanged, and the two always-compile pcs (os irq handler,
                // FSi_ClearOverlayImage) still hand off; the ARM7 os handler address is only
                // trusted once it points into shared wram (before that the gate re-reads it
                // from 0x380FFFC, so everything must go through the gate). A null counter =
                // target outside the executable regions (the hle bios trampolines) — those must
                // reach their special jit entries.
                // Alignment matches call_jit_fun's align_guest_pc: &!1 in thumb, &!3 in ARM.
                let aligned = target & !(step - 1);
                if target & 1 == THUMB as u32 && (cpu == ARM9 || asm.os_irq_handler_addr & 0xFF000000 == regions::SHARED_WRAM_OFFSET) {
                    let count_ptr = asm.emu.jit.jit_memory_map.get_exec_count(aligned);
                    unsafe { assert_unchecked(!count_ptr.is_null()) };
                    let count = unsafe { (*count_ptr).saturating_add(1) };
                    unsafe { *count_ptr = count };
                    // The TWL microcode window (0x1FF8xxx) must also hand off: its HLE
                    // substitution only exists in the jit (see emit_code_block_internal).
                    if count <= INTERP_THRESHOLD
                        && aligned != asm.os_irq_handler_addr
                        && aligned != asm.emu.fs_clear_overlay_image_addr
                        && !(aligned & 0xFFFF000 == 0x1FF8000 && cpu == ARM9 && arm7_hle && asm.emu.nitro_sdk_version.is_twl_sdk())
                    {
                        addr = aligned;
                        page_left = 0;
                        continue;
                    }
                }
                hand_to_next_entry(asm, target);
                return true;
            }
            InstResult::BranchLink(cycles, target, lr) => {
                // The jit's branch_reg adds the +2 branch epilogue cost in its flush, checks the
                // scheduler, pushes `lr` on the return stack and native-calls the target's entry.
                // It returns when the callee came back through a matched return-stack pop; resume
                // interpreting the tail behind the call, like a compiled block would.
                asm.runtime_data.accumulated_cycles += cycles;
                unsafe { (*regs).pc = target };
                unsafe {
                    match (cpu, arm7_hle) {
                        (ARM9, true) => branch_reg::<{ ARM9 }, true, true>(0, target, lr, addr),
                        (ARM9, false) => branch_reg::<{ ARM9 }, true, false>(0, target, lr, addr),
                        (ARM7, true) => branch_reg::<{ ARM7 }, true, true>(0, target, lr, addr),
                        (ARM7, false) => branch_reg::<{ ARM7 }, true, false>(0, target, lr, addr),
                    }
                }
                executed_any = true;
                addr = lr & !1;
                page_left = 0;
            }
            InstResult::BranchReturn(cycles, target) => {
                // The jit's branch_lr adds the +2 in its flush, checks the scheduler and pops the
                // return stack. On a mismatch it never returns (exit_guest_context safety net); on
                // a match, control belongs to our native caller — a compiled call site or the
                // interpreter BranchLink frame that pushed the entry.
                asm.runtime_data.accumulated_cycles += cycles;
                unsafe { (*regs).pc = target };
                unsafe {
                    match (cpu, arm7_hle) {
                        (ARM9, true) => branch_lr::<{ ARM9 }, true>(0, target, addr),
                        (ARM9, false) => branch_lr::<{ ARM9 }, false>(0, target, addr),
                        (ARM7, true) => branch_lr::<{ ARM7 }, true>(0, target, addr),
                        (ARM7, false) => branch_lr::<{ ARM7 }, false>(0, target, addr),
                    }
                }
                return true;
            }
            InstResult::Fallback => {
                if !executed_any {
                    // Nothing ran yet, let the caller compile this exact block.
                    return false;
                }
                // State is consistent at `addr`, compile and run from there.
                let target = addr | THUMB as u32;
                unsafe { (*regs).pc = target };
                hand_to_next_entry(asm, target);
                return true;
            }
        }
    }
}

/// The last interpreted instruction's write requested an immediate breakout: run the jit's
/// `breakout_imm` handler as if the store had been executed by a compiled block. The interpreter
/// has already flushed this instruction's cycles, so the flush inside is neutralized by passing
/// the current pre cycle count sum. Returns only when execution should continue interpreting at
/// `next_addr` (otherwise it exits the guest context, like the jit).
#[inline(never)]
fn run_breakout_imm<const THUMB: bool>(asm: &mut JitAsm, next_addr: u32) {
    // breakout_imm re-derives the resume point from the faulting instruction's (tagged) pc.
    let current_pc = (next_addr - if THUMB { 2 } else { 4 }) | THUMB as u32;
    unsafe {
        match asm.cpu {
            ARM9 => breakout_imm::<{ ARM9 }>(asm, asm.runtime_data.pre_cycle_count_sum, current_pc),
            ARM7 => breakout_imm::<{ ARM7 }>(asm, asm.runtime_data.pre_cycle_count_sum, current_pc),
        }
    }
}

/// Hand control to the jit entry for `target` (guest pc with thumb bit in bit0), exactly like a
/// compiled block's external branch.
#[inline]
fn hand_to_next_entry(asm: &mut JitAsm, target: u32) {
    // Compiled non-link branches tail-call the next block, so jit loops never grow the host
    // stack; the interpreter adds a Rust frame per handback instead. Bound the recursion with
    // the jit's ARM9 stack-depth guard (exit_guest_context unwinds everything and execution
    // resumes from regs.pc). ARM7 is bounded by its scheduler-quantum exit.
    if asm.cpu == ARM9 {
        check_stack_depth(asm, target);
    }
    asm.runtime_data.pre_cycle_count_sum = 0;
    unsafe {
        match asm.cpu {
            ARM9 => call_jit_fun::<{ ARM9 }>(asm, target),
            ARM7 => call_jit_fun::<{ ARM7 }>(asm, target),
        }
    }
}

#[inline]
fn run_scheduler(asm: &mut JitAsm, current_pc: u32, arm7_hle: bool) {
    match (asm.cpu, arm7_hle) {
        (ARM9, true) => check_scheduler::<{ ARM9 }, true>(asm, current_pc),
        (ARM9, false) => check_scheduler::<{ ARM9 }, false>(asm, current_pc),
        (ARM7, true) => check_scheduler::<{ ARM7 }, true>(asm, current_pc),
        (ARM7, false) => check_scheduler::<{ ARM7 }, false>(asm, current_pc),
    }
}

// The execution counters live in jit_memory (JitExecCounts, one shared set laid out per region
// like the jit entries) and are indexed through jit_memory_map::get_exec_count, which collapses
// memory mirrors exactly like the jit entry lookup.
