// Data transfers: single (LDR/STR/LDRB/STRB), halfword/signed (LDRH/STRH/LDRSB/LDRSH) and block
// (LDM/STM). One specialized handler per (load, size, addressing mode, up/down, offset form); the
// whole shape is encoded in const generics so there is no matching on the execution path. Mirrors
// the disassembler's transfer delegations. Semantics ported from NooDS/src/interpreter_transfer.cpp.
//
// Computed access addresses use plain `+`/`-`: an overflow there means the guest did something very
// wrong, and overflow checks will surface it.

use super::{mem_read, mem_read_multiple, mem_write, mem_write_multiple, Ctx, InstResult};
use crate::core::CpuType::{ARM7, ARM9};
use paste::paste;
use std::hint::assert_unchecked;

// ---------------------------------------------------------------------------------------------
// Single data transfer (LDR/STR/LDRB/STRB). ADDR: 0=offset 1=preindex 2=postindex.
// OFFK: 0=immediate, 1=reg LSL, 2=reg LSR, 3=reg ASR, 4=reg ROR.
// ---------------------------------------------------------------------------------------------

#[inline]
fn transfer_reg_offset<const OFFK: u8>(ctx: &Ctx, opcode: u32) -> u32 {
    let v = ctx.reg(opcode & 0xF);
    let amount = (opcode >> 7) & 0x1F;
    match OFFK {
        1 => v << amount,
        2 => {
            if amount == 0 {
                0
            } else {
                v >> amount
            }
        }
        3 => {
            if amount == 0 {
                ((v as i32) >> 31) as u32
            } else {
                ((v as i32) >> amount) as u32
            }
        }
        _ => {
            if amount == 0 {
                (ctx.carry() << 31) | (v >> 1)
            } else {
                v.rotate_right(amount)
            }
        }
    }
}

fn ldr_str<const LOAD: bool, const BYTE: bool, const ADDR: u8, const UP: bool, const OFFK: u8>(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let rn = (opcode >> 16) & 0xF;
    let rd = (opcode >> 12) & 0xF;
    let base = ctx.reg(rn);
    let offset = if OFFK == 0 { opcode & 0xFFF } else { transfer_reg_offset::<OFFK>(ctx, opcode) };
    let offset_addr = if UP { base.wrapping_add(offset) } else { base.wrapping_sub(offset) };
    let addr = if ADDR == 2 { base } else { offset_addr };

    if LOAD {
        let value = if BYTE {
            mem_read!(ctx.asm, u8, addr) as u32
        } else {
            let raw = mem_read!(ctx.asm, u32, addr);
            raw.rotate_right((addr & 3) * 8)
        };
        if ADDR != 0 {
            ctx.set_reg(rn, offset_addr);
        }
        if rd == 15 {
            // `ldr pc, [sp..]` (SP among the source regs) is a return; the jit routes it through
            // the return stack (branch_lr).
            if rn == 13 || (OFFK != 0 && opcode & 0xF == 13) {
                InstResult::BranchReturn(3, value)
            } else {
                InstResult::Branch(3, value)
            }
        } else {
            ctx.set_reg(rd, value);
            InstResult::Continue(3)
        }
    } else {
        let value = if rd == 15 { ctx.inst_addr + 12 } else { ctx.reg(rd) };
        if BYTE {
            mem_write!(ctx.asm, u8, addr, value as u8);
        } else {
            mem_write!(ctx.asm, u32, addr, value);
        }
        if ADDR != 0 {
            ctx.set_reg(rn, offset_addr);
        }
        InstResult::ContinueStore(2)
    }
}

/// Generate the 10 offset variants (`<op>_<a>ip`, `<op>_<a>rpll`, ...) for one addressing mode.
macro_rules! ldr_str_addr {
    ($name:ident, $load:expr, $byte:expr, $addr:expr, $a:ident) => {
        paste! {
            pub(super) fn [<$name _ $a ip>](c: &mut Ctx, o: u32) -> InstResult { ldr_str::<$load, $byte, $addr, true, 0>(c, o) }
            pub(super) fn [<$name _ $a im>](c: &mut Ctx, o: u32) -> InstResult { ldr_str::<$load, $byte, $addr, false, 0>(c, o) }
            pub(super) fn [<$name _ $a rpll>](c: &mut Ctx, o: u32) -> InstResult { ldr_str::<$load, $byte, $addr, true, 1>(c, o) }
            pub(super) fn [<$name _ $a rplr>](c: &mut Ctx, o: u32) -> InstResult { ldr_str::<$load, $byte, $addr, true, 2>(c, o) }
            pub(super) fn [<$name _ $a rpar>](c: &mut Ctx, o: u32) -> InstResult { ldr_str::<$load, $byte, $addr, true, 3>(c, o) }
            pub(super) fn [<$name _ $a rprr>](c: &mut Ctx, o: u32) -> InstResult { ldr_str::<$load, $byte, $addr, true, 4>(c, o) }
            pub(super) fn [<$name _ $a rmll>](c: &mut Ctx, o: u32) -> InstResult { ldr_str::<$load, $byte, $addr, false, 1>(c, o) }
            pub(super) fn [<$name _ $a rmlr>](c: &mut Ctx, o: u32) -> InstResult { ldr_str::<$load, $byte, $addr, false, 2>(c, o) }
            pub(super) fn [<$name _ $a rmar>](c: &mut Ctx, o: u32) -> InstResult { ldr_str::<$load, $byte, $addr, false, 3>(c, o) }
            pub(super) fn [<$name _ $a rmrr>](c: &mut Ctx, o: u32) -> InstResult { ldr_str::<$load, $byte, $addr, false, 4>(c, o) }
        }
    };
}

macro_rules! ldr_str_op {
    ($name:ident, $load:expr, $byte:expr) => {
        ldr_str_addr!($name, $load, $byte, 0, of);
        ldr_str_addr!($name, $load, $byte, 1, pr);
        ldr_str_addr!($name, $load, $byte, 2, pt);
    };
}

ldr_str_op!(ldr, true, false);
ldr_str_op!(str, false, false);
ldr_str_op!(ldrb, true, true);
ldr_str_op!(strb, false, true);

// ---------------------------------------------------------------------------------------------
// Halfword / signed transfers (LDRH/STRH/LDRSB/LDRSH). SIZE: 0=byte 1=half. REG: register offset
// (no shift) vs immediate.
// ---------------------------------------------------------------------------------------------

fn ldrh_strh<const LOAD: bool, const SIZE: u8, const SIGNED: bool, const ADDR: u8, const UP: bool, const REG: bool>(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let rn = (opcode >> 16) & 0xF;
    let rd = (opcode >> 12) & 0xF;
    let base = ctx.reg(rn);
    let offset = if REG { ctx.reg(opcode & 0xF) } else { ((opcode >> 4) & 0xF0) | (opcode & 0xF) };
    let offset_addr = if UP { base.wrapping_add(offset) } else { base.wrapping_sub(offset) };
    let addr = if ADDR == 2 { base } else { offset_addr };

    if LOAD {
        let value = if SIZE == 0 {
            mem_read!(ctx.asm, u8, addr) as i8 as i32 as u32
        } else if SIGNED {
            mem_read!(ctx.asm, u16, addr) as i16 as i32 as u32
        } else {
            mem_read!(ctx.asm, u16, addr) as u32
        };
        if ADDR != 0 {
            ctx.set_reg(rn, offset_addr);
        }
        if rd == 15 {
            // Same return-stack routing rule as the word loads (SP among the source regs).
            if rn == 13 || (REG && opcode & 0xF == 13) {
                InstResult::BranchReturn(3, value)
            } else {
                InstResult::Branch(3, value)
            }
        } else {
            ctx.set_reg(rd, value);
            InstResult::Continue(3)
        }
    } else {
        let value = if rd == 15 { ctx.inst_addr + 12 } else { ctx.reg(rd) };
        mem_write!(ctx.asm, u16, addr, value as u16);
        if ADDR != 0 {
            ctx.set_reg(rn, offset_addr);
        }
        InstResult::ContinueStore(2)
    }
}

macro_rules! half_addr {
    ($name:ident, $load:expr, $size:expr, $signed:expr, $addr:expr, $a:ident) => {
        paste! {
            pub(super) fn [<$name _ $a ip>](c: &mut Ctx, o: u32) -> InstResult { ldrh_strh::<$load, $size, $signed, $addr, true, false>(c, o) }
            pub(super) fn [<$name _ $a im>](c: &mut Ctx, o: u32) -> InstResult { ldrh_strh::<$load, $size, $signed, $addr, false, false>(c, o) }
            pub(super) fn [<$name _ $a rp>](c: &mut Ctx, o: u32) -> InstResult { ldrh_strh::<$load, $size, $signed, $addr, true, true>(c, o) }
            pub(super) fn [<$name _ $a rm>](c: &mut Ctx, o: u32) -> InstResult { ldrh_strh::<$load, $size, $signed, $addr, false, true>(c, o) }
        }
    };
}

macro_rules! half_op {
    ($name:ident, $load:expr, $size:expr, $signed:expr) => {
        half_addr!($name, $load, $size, $signed, 0, of);
        half_addr!($name, $load, $size, $signed, 1, pr);
        half_addr!($name, $load, $size, $signed, 2, pt);
    };
}

half_op!(ldrh, true, 1, false);
half_op!(strh, false, 1, false);
half_op!(ldrsb, true, 0, true);
half_op!(ldrsh, true, 1, true);

// ---------------------------------------------------------------------------------------------
// Block data transfer (LDM/STM). PRE/UP select IA/IB/DA/DB, USER is the S bit, WB is writeback.
// ---------------------------------------------------------------------------------------------

fn ldm_stm<const LOAD: bool, const PRE: bool, const UP: bool, const USER: bool, const WB: bool>(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let rn = (opcode >> 16) & 0xF;
    let reg_list = opcode & 0xFFFF;

    // User-mode banked transfers (S bit without PC in the list) go through the user bank.
    // Empty register lists transfer nothing and write back an unchanged base, like NooDS.
    let user_banked = USER && (reg_list & (1 << 15) == 0) && !ctx.asm.emu.thread_is_user_mode(ctx.cpu);
    let fiq_mode = user_banked && (ctx.cpsr() & 0x1F) == 0x11;
    let count = reg_list.count_ones();

    let base = ctx.reg(rn);
    let final_base = if UP { base.wrapping_add(count * 4) } else { base.wrapping_sub(count * 4) };
    // Registers transfer lowest-numbered to lowest-address; compute the lowest accessed address.
    // ARM address arithmetic wraps mod 2^32 (negative-index / high-base accesses).
    let addr = match (UP, PRE) {
        (true, false) => base,                                        // IA
        (true, true) => base.wrapping_add(4),                         // IB
        (false, true) => base.wrapping_sub(count * 4),                // DB
        (false, false) => base.wrapping_sub(count * 4).wrapping_add(4), // DA
    };

    // The disassembler's cycle value: rlist.len() + 2 for ldm, + 1 for stm.
    let cycles = count as u16 + if LOAD { 2 } else { 1 };

    // Gather/scatter through a stack buffer with a single memory request for the whole list,
    // like the jit's handle_multiple_request; per-word requests would re-resolve the region on
    // every transferred register.
    let mut values = [0u32; 16];
    let count = count as usize;
    unsafe { assert_unchecked(count <= values.len()) };

    if LOAD {
        mem_read_multiple!(ctx.asm, addr, &mut values[..count]);

        let mut branch_target: Option<u32> = None;
        let mut slot = 0;
        for i in 0..16u32 {
            if reg_list & (1 << i) == 0 {
                continue;
            }
            let value = unsafe { *values.get_unchecked(slot) };
            slot += 1;
            if i == 15 {
                branch_target = Some(value);
            } else if user_banked && (i == 13 || i == 14 || fiq_mode) {
                // Only sp/lr are banked outside fiq; r8-r12 share the active file (the user
                // store is a stale copy synced at mode switches) — same rule as the jit's
                // get_reg_usr_mut.
                let cpu = ctx.cpu;
                *ctx.asm.emu.thread_get_reg_usr_mut(cpu, crate::jit::reg::Reg::from(i as u8)) = value;
            } else {
                ctx.set_reg(i, value);
            }
        }

        // Base in the list: the loaded value wins on the ARM7; the ARM9 re-applies the
        // writeback unless the base is the last listed register (an only-reg base writes
        // back too) — armwrestler's LDM base-in-list cases, NooDS parity.
        if WB {
            let base_in_list = reg_list & (1 << rn) != 0;
            if !base_in_list || (ctx.cpu == ARM9 && ((reg_list >> (rn + 1)) != 0 || reg_list == (1 << rn))) {
                ctx.set_reg(rn, final_base);
            }
        }

        // The spsr->cpsr restore of `ldm {..,pc}^` is architecturally the LAST step: the base
        // writeback above must land in the executing mode's sp, not the restored mode's (the
        // mode switch swaps the banked sp/lr). Restoring inside the loop shifted the irq
        // handler's sp writeback into the interrupted mode (libnds cothread stack corruption).
        if USER && branch_target.is_some() {
            ctx.asm.emu.thread_restore_spsr(ctx.cpu);
        }

        match branch_target {
            // `ldm sp.., {.., pc}` (pop) is a return; the jit routes it through the return stack.
            Some(t) if rn == 13 => InstResult::BranchReturn(cycles, t),
            Some(t) => InstResult::Branch(cycles, t),
            None => InstResult::Continue(cycles),
        }
    } else {
        let mut slot = 0;
        for i in 0..16u32 {
            if reg_list & (1 << i) == 0 {
                continue;
            }
            let value = if i == 15 {
                ctx.inst_addr + 12
            } else if user_banked && (i == 13 || i == 14 || fiq_mode) {
                *ctx.asm.emu.thread_get_reg_usr(ctx.cpu, crate::jit::reg::Reg::from(i as u8))
            } else if WB && ctx.cpu == ARM7 && i == rn && (reg_list & ((1 << rn) - 1)) != 0 {
                // ARM7 STM with the base in the list stores the written-back base unless
                // it is the first listed register (NooDS parity; the ARM9 stores the old).
                final_base
            } else {
                ctx.reg(i)
            };
            unsafe { *values.get_unchecked_mut(slot) = value };
            slot += 1;
        }
        mem_write_multiple!(ctx.asm, addr, &values[..count]);

        if WB {
            ctx.set_reg(rn, final_base);
        }
        InstResult::ContinueStore(cycles)
    }
}

macro_rules! ldm_stm_modes {
    ($name:ident, $load:expr, $m:ident, $pre:expr, $up:expr) => {
        paste! {
            pub(super) fn [<$name $m>](c: &mut Ctx, o: u32) -> InstResult { ldm_stm::<$load, $pre, $up, false, false>(c, o) }
            pub(super) fn [<$name $m _w>](c: &mut Ctx, o: u32) -> InstResult { ldm_stm::<$load, $pre, $up, false, true>(c, o) }
            pub(super) fn [<$name $m _u>](c: &mut Ctx, o: u32) -> InstResult { ldm_stm::<$load, $pre, $up, true, false>(c, o) }
            pub(super) fn [<$name $m _u_w>](c: &mut Ctx, o: u32) -> InstResult { ldm_stm::<$load, $pre, $up, true, true>(c, o) }
        }
    };
}

macro_rules! ldm_stm_op {
    ($name:ident, $load:expr) => {
        ldm_stm_modes!($name, $load, ia, false, true);
        ldm_stm_modes!($name, $load, ib, true, true);
        ldm_stm_modes!($name, $load, da, false, false);
        ldm_stm_modes!($name, $load, db, true, false);
    };
}

ldm_stm_op!(ldm, true);
ldm_stm_op!(stm, false);

// ---------------------------------------------------------------------------------------------
// LDRD/STRD (arm9 only; the disassembler rejects odd/pc rd encodings as UnkArm) and SWP/SWPB.
// Semantics from NooDS interpreter_transfer.cpp.
// ---------------------------------------------------------------------------------------------

fn ldrd_strd<const STORE: bool, const ADDR: u8, const UP: bool, const REG: bool>(ctx: &mut Ctx, opcode: u32) -> InstResult {
    if ctx.cpu == ARM7 {
        return InstResult::Continue(1);
    }
    let rn = (opcode >> 16) & 0xF;
    let rd = (opcode >> 12) & 0xF;
    let base = ctx.reg(rn);
    let offset = if REG { ctx.reg(opcode & 0xF) } else { ((opcode >> 4) & 0xF0) | (opcode & 0xF) };
    let offset_addr = if UP { base.wrapping_add(offset) } else { base.wrapping_sub(offset) };
    let addr = if ADDR == 2 { base } else { offset_addr };

    if STORE {
        let v0 = ctx.reg(rd);
        let v1 = ctx.reg(rd + 1);
        mem_write!(ctx.asm, u32, addr, v0);
        mem_write!(ctx.asm, u32, addr.wrapping_add(4), v1);
        if ADDR != 0 {
            ctx.set_reg(rn, offset_addr);
        }
        InstResult::ContinueStore(2)
    } else {
        let v0 = mem_read!(ctx.asm, u32, addr);
        let v1 = mem_read!(ctx.asm, u32, addr.wrapping_add(4));
        if ADDR != 0 {
            ctx.set_reg(rn, offset_addr);
        }
        ctx.set_reg(rd, v0);
        ctx.set_reg(rd + 1, v1);
        InstResult::Continue(2)
    }
}

macro_rules! dword_addr {
    ($name:ident, $store:expr, $addr:expr, $a:ident) => {
        paste! {
            pub(super) fn [<$name _ $a ip>](c: &mut Ctx, o: u32) -> InstResult { ldrd_strd::<$store, $addr, true, false>(c, o) }
            pub(super) fn [<$name _ $a im>](c: &mut Ctx, o: u32) -> InstResult { ldrd_strd::<$store, $addr, false, false>(c, o) }
            pub(super) fn [<$name _ $a rp>](c: &mut Ctx, o: u32) -> InstResult { ldrd_strd::<$store, $addr, true, true>(c, o) }
            pub(super) fn [<$name _ $a rm>](c: &mut Ctx, o: u32) -> InstResult { ldrd_strd::<$store, $addr, false, true>(c, o) }
        }
    };
}

macro_rules! dword_op {
    ($name:ident, $store:expr) => {
        dword_addr!($name, $store, 0, of);
        dword_addr!($name, $store, 1, pr);
        dword_addr!($name, $store, 2, pt);
    };
}

dword_op!(ldrd, false);
dword_op!(strd, true);

fn swp_common<const BYTE: bool>(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let rd = (opcode >> 12) & 0xF;
    let rm = opcode & 0xF;
    let rn = (opcode >> 16) & 0xF;
    let addr = ctx.reg(rn);
    let store_value = ctx.reg(rm);
    let loaded = if BYTE {
        let v = mem_read!(ctx.asm, u8, addr) as u32;
        mem_write!(ctx.asm, u8, addr, store_value as u8);
        v
    } else {
        let v = mem_read!(ctx.asm, u32, addr & !3).rotate_right((addr & 3) << 3);
        mem_write!(ctx.asm, u32, addr & !3, store_value);
        v
    };
    if rd != 15 {
        ctx.set_reg(rd, loaded);
    }
    // NooDS: (arm7 << 1) + 2.
    InstResult::ContinueStore(if ctx.cpu == ARM7 { 4 } else { 2 })
}

pub(super) fn swp(c: &mut Ctx, o: u32) -> InstResult {
    swp_common::<false>(c, o)
}

pub(super) fn swpb(c: &mut Ctx, o: u32) -> InstResult {
    swp_common::<true>(c, o)
}
