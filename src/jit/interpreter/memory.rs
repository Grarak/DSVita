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
    let offset_addr = if UP { base + offset } else { base - offset };
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
    let offset_addr = if UP { base + offset } else { base - offset };
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

    // User-mode banked transfers (S bit, without PC in the list) need banked register access.
    if USER && (reg_list & (1 << 15) == 0) {
        return InstResult::Fallback;
    }

    let count = reg_list.count_ones();
    if count == 0 {
        return InstResult::Fallback;
    }

    let base = ctx.reg(rn);
    let final_base = if UP { base + count * 4 } else { base - count * 4 };
    // Registers transfer lowest-numbered to lowest-address; compute the lowest accessed address.
    let addr = match (UP, PRE) {
        (true, false) => base,                  // IA
        (true, true) => base + 4,               // IB
        (false, true) => base - count * 4,      // DB
        (false, false) => base - count * 4 + 4, // DA
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
                if USER {
                    ctx.asm.emu.thread_restore_spsr(ctx.cpu);
                }
                branch_target = Some(value);
            } else {
                ctx.set_reg(i, value);
            }
        }

        if WB && (reg_list & (1 << rn) == 0) {
            ctx.set_reg(rn, final_base);
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
            let value = if i == 15 { ctx.inst_addr + 12 } else { ctx.reg(i) };
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
