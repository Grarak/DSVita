// Data processing (ALU), multiply, status-register transfers and CLZ.
//
// One specialized handler per (operation, shifter form) — the shifter form is a const generic
// (0=lli 1=llr 2=lri 3=lrr 4=ari 5=arr 6=rri 7=rrr 8=imm) and whether the shifter updates the carry
// flag is a const generic too, so there is no matching on the execution path. Mirrors the
// disassembler's alu delegations. Semantics ported from NooDS/src/interpreter_alu.cpp.

use super::{Ctx, InstResult, C_BIT, N_BIT, T_BIT, V_BIT, Z_BIT};
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::reg::Reg;
use paste::paste;

// ---------------------------------------------------------------------------------------------
// Flag helpers
// ---------------------------------------------------------------------------------------------

#[inline]
pub(super) fn set_nz(ctx: &mut Ctx, result: u32) {
    let mut cpsr = ctx.cpsr() & !(N_BIT | Z_BIT);
    cpsr |= result & N_BIT;
    if result == 0 {
        cpsr |= Z_BIT;
    }
    ctx.set_cpsr(cpsr);
}

#[inline]
fn set_nz_64(ctx: &mut Ctx, result: u64) {
    let mut cpsr = ctx.cpsr() & !(N_BIT | Z_BIT);
    cpsr |= (result >> 32) as u32 & N_BIT;
    if result == 0 {
        cpsr |= Z_BIT;
    }
    ctx.set_cpsr(cpsr);
}

#[inline]
pub(super) fn add_flags(ctx: &mut Ctx, a: u32, b: u32, result: u32) {
    let carry = (a as u64 + b as u64) > 0xFFFFFFFF;
    let overflow = !(a ^ b) & (a ^ result) & N_BIT != 0;
    let mut cpsr = ctx.cpsr() & !(N_BIT | Z_BIT | C_BIT | V_BIT);
    cpsr |= result & N_BIT;
    if result == 0 {
        cpsr |= Z_BIT;
    }
    cpsr |= (carry as u32) << 29;
    cpsr |= (overflow as u32) << 28;
    ctx.set_cpsr(cpsr);
}

#[inline]
fn adc_flags(ctx: &mut Ctx, a: u32, b: u32, carry_in: u64, result: u32) {
    let carry = (a as u64 + b as u64 + carry_in) > 0xFFFFFFFF;
    let overflow = !(a ^ b) & (a ^ result) & N_BIT != 0;
    let mut cpsr = ctx.cpsr() & !(N_BIT | Z_BIT | C_BIT | V_BIT);
    cpsr |= result & N_BIT;
    if result == 0 {
        cpsr |= Z_BIT;
    }
    cpsr |= (carry as u32) << 29;
    cpsr |= (overflow as u32) << 28;
    ctx.set_cpsr(cpsr);
}

#[inline]
pub(super) fn sub_flags(ctx: &mut Ctx, a: u32, b: u32, result: u32) {
    let carry = a >= b;
    let overflow = (a ^ b) & (a ^ result) & N_BIT != 0;
    let mut cpsr = ctx.cpsr() & !(N_BIT | Z_BIT | C_BIT | V_BIT);
    cpsr |= result & N_BIT;
    if result == 0 {
        cpsr |= Z_BIT;
    }
    cpsr |= (carry as u32) << 29;
    cpsr |= (overflow as u32) << 28;
    ctx.set_cpsr(cpsr);
}

#[inline]
fn sbc_flags(ctx: &mut Ctx, a: u32, b: u32, carry_in: u64, result: u32) {
    let carry = (a as u64) >= (b as u64 + (1 - carry_in));
    let overflow = (a ^ b) & (a ^ result) & N_BIT != 0;
    let mut cpsr = ctx.cpsr() & !(N_BIT | Z_BIT | C_BIT | V_BIT);
    cpsr |= result & N_BIT;
    if result == 0 {
        cpsr |= Z_BIT;
    }
    cpsr |= (carry as u32) << 29;
    cpsr |= (overflow as u32) << 28;
    ctx.set_cpsr(cpsr);
}

// ---------------------------------------------------------------------------------------------
// Shifter operand + writeback
// ---------------------------------------------------------------------------------------------

#[inline]
fn alu_rn(ctx: &Ctx, opcode: u32, by_reg: bool) -> u32 {
    let rn_idx = (opcode >> 16) & 0xF;
    ctx.reg(rn_idx) + if by_reg && rn_idx == 15 { 4 } else { 0 }
}

/// IS_MOV marks the plain `mov` operation: `mov pc, <ops with lr>` without flag setting is a
/// function return, which the jit routes through the return stack (handle_indirect_branch's
/// `is_mov && src_regs has LR && !out_regs has CPSR` check) — mirror that exactly.
#[inline]
fn alu_finish<const SHIFT: u8, const IS_MOV: bool>(ctx: &mut Ctx, opcode: u32, result: u32, set_flags: bool) -> InstResult {
    let rd = (opcode >> 12) & 0xF;
    if rd == 15 {
        if set_flags {
            // e.g. `movs pc, lr`: restore CPSR from SPSR (may switch mode/thumb).
            ctx.asm.emu.thread_restore_spsr(ctx.cpu);
        }
        let thumb = ctx.cpsr() & T_BIT != 0;
        let target = if thumb { (result & !1) | 1 } else { result & !3 };
        if IS_MOV && !set_flags && SHIFT != 8 {
            let by_reg = matches!(SHIFT, 1 | 3 | 5 | 7);
            let src_lr = opcode & 0xF == 14 || (by_reg && (opcode >> 8) & 0xF == 14);
            if src_lr {
                return InstResult::BranchReturn(1, target);
            }
        }
        InstResult::Branch(1, target)
    } else {
        ctx.set_reg(rd, result);
        InstResult::Continue(1)
    }
}

/// Compute the ARM shifter operand for the given const shifter form, optionally updating carry.
fn alu_shift<const SHIFT: u8, const SET_CARRY: bool>(ctx: &mut Ctx, opcode: u32) -> u32 {
    if SHIFT == 8 {
        // Rotated immediate.
        let value = opcode & 0xFF;
        let rot = (opcode >> 7) & 0x1E;
        if rot == 0 {
            return value;
        }
        let result = value.rotate_right(rot);
        if SET_CARRY {
            ctx.set_cpsr((ctx.cpsr() & !C_BIT) | (((result >> 31) & 1) << 29));
        }
        return result;
    }

    let by_reg = matches!(SHIFT, 1 | 3 | 5 | 7);
    let rm_idx = opcode & 0xF;
    let value = ctx.reg(rm_idx) + if by_reg && rm_idx == 15 { 4 } else { 0 };
    let amount = if by_reg { ctx.reg((opcode >> 8) & 0xF) & 0xFF } else { (opcode >> 7) & 0x1F };
    let old_carry = ctx.carry();
    let mut carry_out = old_carry;

    let result = match SHIFT {
        0 | 1 => {
            // LSL
            if amount == 0 {
                value
            } else if amount < 32 {
                carry_out = (value >> (32 - amount)) & 1;
                value << amount
            } else if amount == 32 {
                carry_out = value & 1;
                0
            } else {
                carry_out = 0;
                0
            }
        }
        2 | 3 => {
            // LSR (immediate form: 0 means 32)
            let amount = if !by_reg && amount == 0 { 32 } else { amount };
            if amount == 0 {
                value
            } else if amount < 32 {
                carry_out = (value >> (amount - 1)) & 1;
                value >> amount
            } else if amount == 32 {
                carry_out = (value >> 31) & 1;
                0
            } else {
                carry_out = 0;
                0
            }
        }
        4 | 5 => {
            // ASR (immediate form: 0 means 32)
            let amount = if !by_reg && amount == 0 { 32 } else { amount };
            if amount == 0 {
                value
            } else if amount < 32 {
                carry_out = (value >> (amount - 1)) & 1;
                ((value as i32) >> amount) as u32
            } else {
                carry_out = (value >> 31) & 1;
                ((value as i32) >> 31) as u32
            }
        }
        _ => {
            // ROR (immediate form: 0 means RRX)
            if !by_reg && amount == 0 {
                carry_out = value & 1;
                (old_carry << 31) | (value >> 1)
            } else if amount == 0 {
                value
            } else if amount & 0x1F == 0 {
                carry_out = (value >> 31) & 1;
                value
            } else {
                let rot = amount & 0x1F;
                carry_out = (value >> (rot - 1)) & 1;
                value.rotate_right(rot)
            }
        }
    };

    if SET_CARRY {
        ctx.set_cpsr((ctx.cpsr() & !C_BIT) | (carry_out << 29));
    }
    result
}

// ---------------------------------------------------------------------------------------------
// Operation handlers (generic over the shifter form)
// ---------------------------------------------------------------------------------------------

macro_rules! def_logical {
    ($name:ident, $s:expr, $is_mov:expr, |$a:ident, $b:ident| $body:expr) => {
        fn $name<const SHIFT: u8>(ctx: &mut Ctx, opcode: u32) -> InstResult {
            let by_reg = matches!(SHIFT, 1 | 3 | 5 | 7);
            let $a = alu_rn(ctx, opcode, by_reg);
            let $b = alu_shift::<SHIFT, $s>(ctx, opcode);
            let result = $body;
            if $s {
                set_nz(ctx, result);
            }
            alu_finish::<SHIFT, $is_mov>(ctx, opcode, result, $s)
        }
    };
}

def_logical!(and, false, false, |a, b| a & b);
def_logical!(ands, true, false, |a, b| a & b);
def_logical!(eor, false, false, |a, b| a ^ b);
def_logical!(eors, true, false, |a, b| a ^ b);
def_logical!(orr, false, false, |a, b| a | b);
def_logical!(orrs, true, false, |a, b| a | b);
def_logical!(bic, false, false, |a, b| a & !b);
def_logical!(bics, true, false, |a, b| a & !b);
def_logical!(mov, false, true, |_a, b| b);
def_logical!(movs, true, true, |_a, b| b);
def_logical!(mvn, false, false, |_a, b| !b);
def_logical!(mvns, true, false, |_a, b| !b);

fn do_add(ctx: &mut Ctx, a: u32, b: u32, s: bool) -> u32 {
    let r = a.wrapping_add(b);
    if s {
        add_flags(ctx, a, b, r);
    }
    r
}
fn do_sub(ctx: &mut Ctx, a: u32, b: u32, s: bool) -> u32 {
    let r = a.wrapping_sub(b);
    if s {
        sub_flags(ctx, a, b, r);
    }
    r
}
fn do_rsb(ctx: &mut Ctx, a: u32, b: u32, s: bool) -> u32 {
    let r = b.wrapping_sub(a);
    if s {
        sub_flags(ctx, b, a, r);
    }
    r
}
pub(super) fn do_adc(ctx: &mut Ctx, a: u32, b: u32, s: bool) -> u32 {
    let c = ctx.carry();
    let r = a.wrapping_add(b).wrapping_add(c);
    if s {
        adc_flags(ctx, a, b, c as u64, r);
    }
    r
}
pub(super) fn do_sbc(ctx: &mut Ctx, a: u32, b: u32, s: bool) -> u32 {
    let c = ctx.carry();
    let r = a.wrapping_sub(b).wrapping_sub(1 - c);
    if s {
        sbc_flags(ctx, a, b, c as u64, r);
    }
    r
}
fn do_rsc(ctx: &mut Ctx, a: u32, b: u32, s: bool) -> u32 {
    let c = ctx.carry();
    let r = b.wrapping_sub(a).wrapping_sub(1 - c);
    if s {
        sbc_flags(ctx, b, a, c as u64, r);
    }
    r
}

macro_rules! def_arith {
    ($name:ident, $s:expr, $compute:expr) => {
        fn $name<const SHIFT: u8>(ctx: &mut Ctx, opcode: u32) -> InstResult {
            let by_reg = matches!(SHIFT, 1 | 3 | 5 | 7);
            let a = alu_rn(ctx, opcode, by_reg);
            let b = alu_shift::<SHIFT, false>(ctx, opcode);
            let result = $compute(ctx, a, b, $s);
            alu_finish::<SHIFT, false>(ctx, opcode, result, $s)
        }
    };
}

def_arith!(add, false, do_add);
def_arith!(adds, true, do_add);
def_arith!(sub, false, do_sub);
def_arith!(subs, true, do_sub);
def_arith!(rsb, false, do_rsb);
def_arith!(rsbs, true, do_rsb);
def_arith!(adc, false, do_adc);
def_arith!(adcs, true, do_adc);
def_arith!(sbc, false, do_sbc);
def_arith!(sbcs, true, do_sbc);
def_arith!(rsc, false, do_rsc);
def_arith!(rscs, true, do_rsc);

// Tests (logical, carry from shifter) and compares (arithmetic), always set flags, no write back.
macro_rules! def_test {
    ($name:ident, logical, |$a:ident, $b:ident| $body:expr) => {
        fn $name<const SHIFT: u8>(ctx: &mut Ctx, opcode: u32) -> InstResult {
            let by_reg = matches!(SHIFT, 1 | 3 | 5 | 7);
            let $a = alu_rn(ctx, opcode, by_reg);
            let $b = alu_shift::<SHIFT, true>(ctx, opcode);
            set_nz(ctx, $body);
            InstResult::Continue(1)
        }
    };
    ($name:ident, arith, |$ctx:ident, $a:ident, $b:ident| $body:expr) => {
        fn $name<const SHIFT: u8>(ctx: &mut Ctx, opcode: u32) -> InstResult {
            let by_reg = matches!(SHIFT, 1 | 3 | 5 | 7);
            let $a = alu_rn(ctx, opcode, by_reg);
            let $b = alu_shift::<SHIFT, false>(ctx, opcode);
            let $ctx = ctx;
            $body;
            InstResult::Continue(1)
        }
    };
}

def_test!(tst, logical, |a, b| a & b);
def_test!(teq, logical, |a, b| a ^ b);
def_test!(cmp, arith, |ctx, a, b| {
    let r = a.wrapping_sub(b);
    sub_flags(ctx, a, b, r);
});
def_test!(cmn, arith, |ctx, a, b| {
    let r = a.wrapping_add(b);
    add_flags(ctx, a, b, r);
});

/// Generate the `<op>_lli`/`<op>_llr`/.../`<op>_imm` table wrappers for each ALU operation.
macro_rules! alu_wrappers {
    ($($name:ident),* $(,)?) => { paste! { $(
        pub(super) fn [<$name _lli>](c: &mut Ctx, o: u32) -> InstResult { $name::<0>(c, o) }
        pub(super) fn [<$name _llr>](c: &mut Ctx, o: u32) -> InstResult { $name::<1>(c, o) }
        pub(super) fn [<$name _lri>](c: &mut Ctx, o: u32) -> InstResult { $name::<2>(c, o) }
        pub(super) fn [<$name _lrr>](c: &mut Ctx, o: u32) -> InstResult { $name::<3>(c, o) }
        pub(super) fn [<$name _ari>](c: &mut Ctx, o: u32) -> InstResult { $name::<4>(c, o) }
        pub(super) fn [<$name _arr>](c: &mut Ctx, o: u32) -> InstResult { $name::<5>(c, o) }
        pub(super) fn [<$name _rri>](c: &mut Ctx, o: u32) -> InstResult { $name::<6>(c, o) }
        pub(super) fn [<$name _rrr>](c: &mut Ctx, o: u32) -> InstResult { $name::<7>(c, o) }
        pub(super) fn [<$name _imm>](c: &mut Ctx, o: u32) -> InstResult { $name::<8>(c, o) }
    )* } };
}

alu_wrappers!(and, ands, eor, eors, sub, subs, rsb, rsbs, add, adds, adc, adcs, sbc, sbcs, rsc, rscs, tst, teq, cmp, cmn, orr, orrs, mov, movs, bic, bics, mvn, mvns,);

// ---------------------------------------------------------------------------------------------
// Multiply
// ---------------------------------------------------------------------------------------------

macro_rules! mul32 {
    ($name:ident, $s:expr, $acc:expr) => {
        pub(super) fn $name(ctx: &mut Ctx, opcode: u32) -> InstResult {
            let rd = (opcode >> 16) & 0xF;
            let rn = (opcode >> 12) & 0xF;
            let rm = ctx.reg(opcode & 0xF);
            let rs = ctx.reg((opcode >> 8) & 0xF);
            let mut r = rm.wrapping_mul(rs);
            if $acc {
                r = r.wrapping_add(ctx.reg(rn));
            }
            ctx.set_reg(rd, r);
            if $s {
                set_nz(ctx, r);
            }
            InstResult::Continue(if $acc { 6 } else { 5 })
        }
    };
}

mul32!(mul, false, false);
mul32!(muls, true, false);
mul32!(mla, false, true);
mul32!(mlas, true, true);

macro_rules! mul64 {
    ($name:ident, $s:expr, $acc:expr, $signed:expr) => {
        pub(super) fn $name(ctx: &mut Ctx, opcode: u32) -> InstResult {
            let rdhi = (opcode >> 16) & 0xF;
            let rdlo = (opcode >> 12) & 0xF;
            let rm = ctx.reg(opcode & 0xF);
            let rs = ctx.reg((opcode >> 8) & 0xF);
            let mut r = if $signed {
                ((rm as i32 as i64).wrapping_mul(rs as i32 as i64)) as u64
            } else {
                (rm as u64).wrapping_mul(rs as u64)
            };
            if $acc {
                let lo = ctx.reg(rdlo) as u64;
                let hi = ctx.reg(rdhi) as u64;
                r = r.wrapping_add((hi << 32) | lo);
            }
            ctx.set_reg(rdlo, r as u32);
            ctx.set_reg(rdhi, (r >> 32) as u32);
            if $s {
                set_nz_64(ctx, r);
            }
            InstResult::Continue(if $acc { 7 } else { 6 })
        }
    };
}

mul64!(umull, false, false, false);
mul64!(umulls, true, false, false);
mul64!(umlal, false, true, false);
mul64!(umlals, true, true, false);
mul64!(smull, false, false, true);
mul64!(smulls, true, false, true);
mul64!(smlal, false, true, true);
mul64!(smlals, true, true, true);

// ---------------------------------------------------------------------------------------------
// Status register transfers + CLZ
// ---------------------------------------------------------------------------------------------

pub(super) fn mrs_rc(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let value = ctx.cpsr();
    ctx.set_reg((opcode >> 12) & 0xF, value);
    InstResult::Continue(1)
}

pub(super) fn mrs_rs(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let value = *ctx.asm.emu.thread_get_reg(ctx.cpu, Reg::SPSR);
    ctx.set_reg((opcode >> 12) & 0xF, value);
    InstResult::Continue(1)
}

#[inline]
fn msr_fields(opcode: u32) -> u8 {
    let mut flags = 0u8;
    let mut i = 0;
    while i < 4 {
        if opcode & (1 << (16 + i)) != 0 {
            flags |= 1 << i;
        }
        i += 1;
    }
    flags
}

#[inline]
fn msr_apply(ctx: &mut Ctx, opcode: u32, value: u32, spsr: bool) -> InstResult {
    let flags = msr_fields(opcode);
    if spsr {
        ctx.asm.emu.thread_set_spsr_with_flags(ctx.cpu, value, flags);
    } else {
        match ctx.cpu {
            ARM9 => ctx.asm.emu.thread_set_cpsr_with_flags(ARM9, value, flags),
            ARM7 => ctx.asm.emu.thread_set_cpsr_with_flags(ARM7, value, flags),
        }
    }
    InstResult::Continue(1)
}

pub(super) fn msr_rc(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let value = ctx.reg(opcode & 0xF);
    msr_apply(ctx, opcode, value, false)
}
pub(super) fn msr_rs(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let value = ctx.reg(opcode & 0xF);
    msr_apply(ctx, opcode, value, true)
}
pub(super) fn msr_ic(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let value = (opcode & 0xFF).rotate_right((opcode >> 7) & 0x1E);
    msr_apply(ctx, opcode, value, false)
}
pub(super) fn msr_is(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let value = (opcode & 0xFF).rotate_right((opcode >> 7) & 0x1E);
    msr_apply(ctx, opcode, value, true)
}

pub(super) fn clz(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let rm = ctx.reg(opcode & 0xF);
    ctx.set_reg((opcode >> 12) & 0xF, rm.leading_zeros());
    InstResult::Continue(1)
}

// ---------------------------------------------------------------------------------------------
// ARMv5 DSP extensions (arm9 only): saturating add/sub and signed halfword multiplies.
// Semantics from NooDS interpreter_alu.cpp; Q (bit 27) is sticky.
// ---------------------------------------------------------------------------------------------

const Q_BIT: u32 = 1 << 27;

#[inline]
fn clamp_q(ctx: &mut Ctx, value: i64) -> u32 {
    if value > 0x7FFFFFFF {
        ctx.set_cpsr(ctx.cpsr() | Q_BIT);
        0x7FFFFFFF
    } else if value < -0x80000000 {
        ctx.set_cpsr(ctx.cpsr() | Q_BIT);
        0x80000000
    } else {
        value as u32
    }
}

macro_rules! qalu {
    ($name:ident, |$ctx:ident, $a:ident, $b:ident| $body:expr) => {
        pub(super) fn $name($ctx: &mut Ctx, opcode: u32) -> InstResult {
            if $ctx.cpu == ARM7 {
                return InstResult::Continue(1);
            }
            let $a = $ctx.reg(opcode & 0xF) as i32 as i64;
            let $b = $ctx.reg((opcode >> 16) & 0xF) as i32 as i64;
            let value = $body;
            let rd = (opcode >> 12) & 0xF;
            let result = clamp_q($ctx, value);
            if rd != 15 {
                $ctx.set_reg(rd, result);
            }
            InstResult::Continue(1)
        }
    };
}

qalu!(qadd, |ctx, a, b| a + b);
qalu!(qsub, |ctx, a, b| a - b);
qalu!(qdadd, |ctx, a, b| {
    let doubled = clamp_q(ctx, b * 2) as i32 as i64;
    a + doubled
});
qalu!(qdsub, |ctx, a, b| {
    let doubled = clamp_q(ctx, b * 2) as i32 as i64;
    a - doubled
});

#[inline]
fn half<const TOP: bool>(value: u32) -> i64 {
    if TOP {
        (value >> 16) as i16 as i64
    } else {
        value as i16 as i64
    }
}

macro_rules! smul_xy {
    ($name:ident, $x:expr, $y:expr) => {
        pub(super) fn $name(ctx: &mut Ctx, opcode: u32) -> InstResult {
            if ctx.cpu == ARM7 {
                return InstResult::Continue(1);
            }
            let op1 = half::<$x>(ctx.reg(opcode & 0xF));
            let op2 = half::<$y>(ctx.reg((opcode >> 8) & 0xF));
            let rd = (opcode >> 16) & 0xF;
            ctx.set_reg(rd, (op1 * op2) as u32);
            InstResult::Continue(1)
        }
    };
}

smul_xy!(smulbb, false, false);
smul_xy!(smulbt, false, true);
smul_xy!(smultb, true, false);
smul_xy!(smultt, true, true);

macro_rules! smla_xy {
    ($name:ident, $x:expr, $y:expr) => {
        pub(super) fn $name(ctx: &mut Ctx, opcode: u32) -> InstResult {
            if ctx.cpu == ARM7 {
                return InstResult::Continue(1);
            }
            let op1 = half::<$x>(ctx.reg(opcode & 0xF));
            let op2 = half::<$y>(ctx.reg((opcode >> 8) & 0xF));
            let acc = ctx.reg((opcode >> 12) & 0xF) as i32 as i64;
            let res = op1 * op2 + acc;
            let rd = (opcode >> 16) & 0xF;
            let truncated = res as i32;
            if res != truncated as i64 {
                ctx.set_cpsr(ctx.cpsr() | Q_BIT);
            }
            ctx.set_reg(rd, truncated as u32);
            InstResult::Continue(1)
        }
    };
}

smla_xy!(smlabb, false, false);
smla_xy!(smlabt, false, true);
smla_xy!(smlatb, true, false);
smla_xy!(smlatt, true, true);

macro_rules! smulw_y {
    ($name:ident, $y:expr, $acc:expr) => {
        pub(super) fn $name(ctx: &mut Ctx, opcode: u32) -> InstResult {
            if ctx.cpu == ARM7 {
                return InstResult::Continue(1);
            }
            let op1 = ctx.reg(opcode & 0xF) as i32 as i64;
            let op2 = half::<$y>(ctx.reg((opcode >> 8) & 0xF));
            let mut res = (op1 * op2) >> 16;
            if $acc {
                let acc = ctx.reg((opcode >> 12) & 0xF) as i32 as i64;
                res += acc;
                let truncated = res as i32;
                if res != truncated as i64 {
                    ctx.set_cpsr(ctx.cpsr() | Q_BIT);
                }
            }
            let rd = (opcode >> 16) & 0xF;
            ctx.set_reg(rd, res as u32);
            InstResult::Continue(1)
        }
    };
}

smulw_y!(smulwb, false, false);
smulw_y!(smulwt, true, false);
smulw_y!(smlawb, false, true);
smulw_y!(smlawt, true, true);

macro_rules! smlal_xy {
    ($name:ident, $x:expr, $y:expr) => {
        pub(super) fn $name(ctx: &mut Ctx, opcode: u32) -> InstResult {
            if ctx.cpu == ARM7 {
                return InstResult::Continue(1);
            }
            let rd_lo = (opcode >> 12) & 0xF;
            let rd_hi = (opcode >> 16) & 0xF;
            let op2 = half::<$x>(ctx.reg(opcode & 0xF));
            let op3 = half::<$y>(ctx.reg((opcode >> 8) & 0xF));
            let mut res = ((ctx.reg(rd_hi) as u64) << 32 | ctx.reg(rd_lo) as u64) as i64;
            res = res.wrapping_add(op2 * op3);
            ctx.set_reg(rd_lo, res as u32);
            ctx.set_reg(rd_hi, (res >> 32) as u32);
            InstResult::Continue(2)
        }
    };
}

smlal_xy!(smlalbb, false, false);
smlal_xy!(smlalbt, false, true);
smlal_xy!(smlaltb, true, false);
smlal_xy!(smlaltt, true, true);
