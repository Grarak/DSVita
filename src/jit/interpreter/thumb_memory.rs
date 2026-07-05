// Thumb data transfers: register/immediate offset loads and stores, SP/PC-relative accesses and
// the block transfers (LDMIA/STMIA/PUSH/POP). Word loads rotate misaligned reads exactly like the
// jit's memory handler; halfword loads don't (the jit doesn't model the ARM7 rotate quirk either).
// Semantics ported from NooDS/src/interpreter_transfer.cpp (the *T functions).
//
// Access addresses wrap like hardware: register offsets are routinely negative (stored as large
// unsigned values), so plain `+`/`-` would false-positive under overflow checks (GTA: Chinatown
// Wars indexes arrays with -4 in the offset register).

use super::{mem_read, mem_read_multiple, mem_write, mem_write_multiple, Ctx, InstResult};
use crate::core::CpuType::{ARM7, ARM9};
use std::hint::assert_unchecked;

/// Gather/scatter buffer for the block transfers: 8 low registers + LR/PC. One memory request per
/// transfer, like the jit's handle_multiple_request.
#[inline]
fn transfer_buf() -> [u32; 9] {
    [0; 9]
}

#[inline]
fn rotate_word_read(raw: u32, addr: u32) -> u32 {
    raw.rotate_right((addr & 3) * 8)
}

// ---------------------------------------------------------------------------------------------
// Register offset (format 7/8): Rd in bits 0-2, base Rb in bits 3-5, offset Ro in bits 6-8.
// ---------------------------------------------------------------------------------------------

#[inline]
fn reg_offset_addr(ctx: &Ctx, opcode: u16) -> u32 {
    ctx.reg(((opcode >> 3) & 0x7) as u32).wrapping_add(ctx.reg(((opcode >> 6) & 0x7) as u32))
}

pub(super) fn ldr_reg_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = reg_offset_addr(ctx, opcode);
    let value = rotate_word_read(mem_read!(ctx.asm, u32, addr), addr);
    ctx.set_reg((opcode & 0x7) as u32, value);
    InstResult::Continue(3)
}

pub(super) fn ldrb_reg_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = reg_offset_addr(ctx, opcode);
    let value = mem_read!(ctx.asm, u8, addr) as u32;
    ctx.set_reg((opcode & 0x7) as u32, value);
    InstResult::Continue(3)
}

pub(super) fn ldrh_reg_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = reg_offset_addr(ctx, opcode);
    let value = mem_read!(ctx.asm, u16, addr) as u32;
    ctx.set_reg((opcode & 0x7) as u32, value);
    InstResult::Continue(3)
}

pub(super) fn ldrsb_reg_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = reg_offset_addr(ctx, opcode);
    let value = mem_read!(ctx.asm, u8, addr) as i8 as i32 as u32;
    ctx.set_reg((opcode & 0x7) as u32, value);
    InstResult::Continue(3)
}

pub(super) fn ldrsh_reg_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = reg_offset_addr(ctx, opcode);
    let value = mem_read!(ctx.asm, u16, addr) as i16 as i32 as u32;
    ctx.set_reg((opcode & 0x7) as u32, value);
    InstResult::Continue(3)
}

pub(super) fn str_reg_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = reg_offset_addr(ctx, opcode);
    let value = ctx.reg((opcode & 0x7) as u32);
    mem_write!(ctx.asm, u32, addr, value);
    InstResult::ContinueStore(2)
}

pub(super) fn strb_reg_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = reg_offset_addr(ctx, opcode);
    let value = ctx.reg((opcode & 0x7) as u32);
    mem_write!(ctx.asm, u8, addr, value as u8);
    InstResult::ContinueStore(2)
}

pub(super) fn strh_reg_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = reg_offset_addr(ctx, opcode);
    let value = ctx.reg((opcode & 0x7) as u32);
    mem_write!(ctx.asm, u16, addr, value as u16);
    InstResult::ContinueStore(2)
}

// ---------------------------------------------------------------------------------------------
// Immediate offset (format 9/10); the 5-bit immediate is scaled by the access size.
// ---------------------------------------------------------------------------------------------

pub(super) fn ldr_imm5_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = ctx.reg(((opcode >> 3) & 0x7) as u32).wrapping_add(((opcode >> 4) & 0x7C) as u32);
    let value = rotate_word_read(mem_read!(ctx.asm, u32, addr), addr);
    ctx.set_reg((opcode & 0x7) as u32, value);
    InstResult::Continue(3)
}

pub(super) fn ldrb_imm5_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = ctx.reg(((opcode >> 3) & 0x7) as u32).wrapping_add(((opcode >> 6) & 0x1F) as u32);
    let value = mem_read!(ctx.asm, u8, addr) as u32;
    ctx.set_reg((opcode & 0x7) as u32, value);
    InstResult::Continue(3)
}

pub(super) fn ldrh_imm5_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = ctx.reg(((opcode >> 3) & 0x7) as u32).wrapping_add(((opcode >> 5) & 0x3E) as u32);
    let value = mem_read!(ctx.asm, u16, addr) as u32;
    ctx.set_reg((opcode & 0x7) as u32, value);
    InstResult::Continue(3)
}

pub(super) fn str_imm5_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = ctx.reg(((opcode >> 3) & 0x7) as u32).wrapping_add(((opcode >> 4) & 0x7C) as u32);
    let value = ctx.reg((opcode & 0x7) as u32);
    mem_write!(ctx.asm, u32, addr, value);
    InstResult::ContinueStore(2)
}

pub(super) fn strb_imm5_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = ctx.reg(((opcode >> 3) & 0x7) as u32).wrapping_add(((opcode >> 6) & 0x1F) as u32);
    let value = ctx.reg((opcode & 0x7) as u32);
    mem_write!(ctx.asm, u8, addr, value as u8);
    InstResult::ContinueStore(2)
}

pub(super) fn strh_imm5_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = ctx.reg(((opcode >> 3) & 0x7) as u32).wrapping_add(((opcode >> 5) & 0x3E) as u32);
    let value = ctx.reg((opcode & 0x7) as u32);
    mem_write!(ctx.asm, u16, addr, value as u16);
    InstResult::ContinueStore(2)
}

// ---------------------------------------------------------------------------------------------
// PC/SP-relative accesses (format 6/11), Rd in bits 8-10.
// ---------------------------------------------------------------------------------------------

pub(super) fn ldr_pc_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = ((ctx.inst_addr + 4) & !3) + (((opcode & 0xFF) as u32) << 2);
    let value = rotate_word_read(mem_read!(ctx.asm, u32, addr), addr);
    ctx.set_reg(((opcode >> 8) & 0x7) as u32, value);
    InstResult::Continue(3)
}

pub(super) fn ldr_sp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = ctx.reg(13).wrapping_add(((opcode & 0xFF) as u32) << 2);
    let value = rotate_word_read(mem_read!(ctx.asm, u32, addr), addr);
    ctx.set_reg(((opcode >> 8) & 0x7) as u32, value);
    InstResult::Continue(3)
}

pub(super) fn str_sp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let addr = ctx.reg(13).wrapping_add(((opcode & 0xFF) as u32) << 2);
    let value = ctx.reg(((opcode >> 8) & 0x7) as u32);
    mem_write!(ctx.asm, u32, addr, value);
    InstResult::ContinueStore(2)
}

// ---------------------------------------------------------------------------------------------
// Block transfers (formats 14/15). Register lists only cover R0-R7 (+ LR/PC variants).
// ---------------------------------------------------------------------------------------------

pub(super) fn ldmia_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rb = ((opcode >> 8) & 0x7) as u32;
    let rlist = (opcode & 0xFF) as u32;
    if rlist == 0 {
        // Empty register list: unpredictable, let the jit decide.
        return InstResult::Fallback;
    }
    let count = rlist.count_ones() as usize;
    let addr = ctx.reg(rb);

    let mut values = transfer_buf();
    unsafe { assert_unchecked(count <= values.len()) };
    mem_read_multiple!(ctx.asm, addr, &mut values[..count]);

    // Write the base back first; a load of the base register overrides it.
    ctx.set_reg(rb, addr.wrapping_add(count as u32 * 4));
    let mut slot = 0;
    for i in 0..8 {
        if rlist & (1 << i) == 0 {
            continue;
        }
        ctx.set_reg(i, unsafe { *values.get_unchecked(slot) });
        slot += 1;
    }
    InstResult::Continue(count as u16 + 2)
}

pub(super) fn stmia_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rb = ((opcode >> 8) & 0x7) as u32;
    let rlist = (opcode & 0xFF) as u32;
    if rlist == 0 {
        return InstResult::Fallback;
    }
    let count = rlist.count_ones() as usize;
    let addr = ctx.reg(rb);
    let final_base = addr.wrapping_add(count as u32 * 4);
    // ARM7 stores the written-back base when it's in the list but not the lowest entry; the jit
    // memory handler models the same quirk.
    if ctx.cpu == ARM7 && (rlist & ((1 << (rb + 1)) - 1)) > (1 << rb) {
        ctx.set_reg(rb, final_base);
    }

    let mut values = transfer_buf();
    unsafe { assert_unchecked(count <= values.len()) };
    let mut slot = 0;
    for i in 0..8 {
        if rlist & (1 << i) == 0 {
            continue;
        }
        unsafe { *values.get_unchecked_mut(slot) = ctx.reg(i) };
        slot += 1;
    }
    mem_write_multiple!(ctx.asm, addr, &values[..count]);

    ctx.set_reg(rb, final_base);
    InstResult::ContinueStore(1 + count as u16)
}

pub(super) fn pop_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rlist = (opcode & 0xFF) as u32;
    let count = rlist.count_ones() as usize;
    let sp = ctx.reg(13);

    let mut values = transfer_buf();
    unsafe { assert_unchecked(count <= values.len()) };
    mem_read_multiple!(ctx.asm, sp, &mut values[..count]);

    let mut slot = 0;
    for i in 0..8 {
        if rlist & (1 << i) == 0 {
            continue;
        }
        ctx.set_reg(i, unsafe { *values.get_unchecked(slot) });
        slot += 1;
    }
    ctx.set_reg(13, sp.wrapping_add(count as u32 * 4));
    InstResult::Continue(count as u16 + 2)
}

pub(super) fn pop_pc_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rlist = (opcode & 0xFF) as u32;
    let count = (rlist.count_ones() + 1) as usize;
    let sp = ctx.reg(13);

    let mut values = transfer_buf();
    unsafe { assert_unchecked(count <= values.len()) };
    mem_read_multiple!(ctx.asm, sp, &mut values[..count]);

    let mut slot = 0;
    for i in 0..8 {
        if rlist & (1 << i) == 0 {
            continue;
        }
        ctx.set_reg(i, unsafe { *values.get_unchecked(slot) });
        slot += 1;
    }
    let value = unsafe { *values.get_unchecked(count - 1) };
    ctx.set_reg(13, sp.wrapping_add(count as u32 * 4));
    // ARMv5 interworks on a pc pop (bit 0 selects the mode), ARMv4 stays in thumb.
    let target = match ctx.cpu {
        ARM9 => value,
        ARM7 => (value & !1) | 1,
    };
    // `pop {.., pc}` is a return; the jit routes it through the return stack (branch_lr).
    InstResult::BranchReturn(count as u16 + 2, target)
}

pub(super) fn push_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rlist = (opcode & 0xFF) as u32;
    let count = rlist.count_ones() as usize;
    let addr = ctx.reg(13).wrapping_sub(count as u32 * 4);
    ctx.set_reg(13, addr);

    let mut values = transfer_buf();
    unsafe { assert_unchecked(count <= values.len()) };
    let mut slot = 0;
    for i in 0..8 {
        if rlist & (1 << i) == 0 {
            continue;
        }
        unsafe { *values.get_unchecked_mut(slot) = ctx.reg(i) };
        slot += 1;
    }
    mem_write_multiple!(ctx.asm, addr, &values[..count]);
    InstResult::ContinueStore(1 + count as u16)
}

pub(super) fn push_lr_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rlist = (opcode & 0xFF) as u32;
    let count = (rlist.count_ones() + 1) as usize;
    let addr = ctx.reg(13).wrapping_sub(count as u32 * 4);
    ctx.set_reg(13, addr);

    let mut values = transfer_buf();
    unsafe { assert_unchecked(count <= values.len()) };
    let mut slot = 0;
    for i in 0..8 {
        if rlist & (1 << i) == 0 {
            continue;
        }
        unsafe { *values.get_unchecked_mut(slot) = ctx.reg(i) };
        slot += 1;
    }
    unsafe { *values.get_unchecked_mut(count - 1) = ctx.reg(14) };
    mem_write_multiple!(ctx.asm, addr, &values[..count]);
    InstResult::ContinueStore(1 + count as u16)
}
