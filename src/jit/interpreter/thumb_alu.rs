// Thumb ALU: shifts, add/sub, immediate ops, register-to-register data processing and the hi
// register operations. Thumb encodes the operand registers in fixed positions per opcode class, so
// every handler is already fully specialized without const generics. Flag semantics are shared with
// the ARM handlers (alu.rs). Semantics ported from NooDS/src/interpreter_alu.cpp (the *T functions).

use super::alu::{add_flags, do_adc, do_sbc, set_nz, sub_flags};
use super::{Ctx, InstResult, C_BIT, N_BIT, Z_BIT};

#[inline]
fn set_nzc(ctx: &mut Ctx, result: u32, carry: u32) {
    let mut cpsr = ctx.cpsr() & !(N_BIT | Z_BIT | C_BIT);
    cpsr |= result & N_BIT;
    if result == 0 {
        cpsr |= Z_BIT;
    }
    cpsr |= carry << 29;
    ctx.set_cpsr(cpsr);
}

// ---------------------------------------------------------------------------------------------
// Shift by immediate (LSL/LSR/ASR Rd, Rs, #i). LSR/ASR treat an amount of 0 as 32.
// ---------------------------------------------------------------------------------------------

pub(super) fn lsl_imm_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (opcode & 0x7) as u32;
    let op1 = ctx.reg(((opcode >> 3) & 0x7) as u32);
    let amount = ((opcode >> 6) & 0x1F) as u32;
    let result = op1 << amount;
    ctx.set_reg(rd, result);
    if amount == 0 {
        // LSL #0 leaves the carry untouched.
        set_nz(ctx, result);
    } else {
        set_nzc(ctx, result, (op1 >> (32 - amount)) & 1);
    }
    InstResult::Continue(1)
}

pub(super) fn lsr_imm_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (opcode & 0x7) as u32;
    let op1 = ctx.reg(((opcode >> 3) & 0x7) as u32);
    let amount = ((opcode >> 6) & 0x1F) as u32;
    let result = if amount == 0 { 0 } else { op1 >> amount };
    let carry = (op1 >> if amount == 0 { 31 } else { amount - 1 }) & 1;
    ctx.set_reg(rd, result);
    set_nzc(ctx, result, carry);
    InstResult::Continue(1)
}

pub(super) fn asr_imm_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (opcode & 0x7) as u32;
    let op1 = ctx.reg(((opcode >> 3) & 0x7) as u32);
    let amount = ((opcode >> 6) & 0x1F) as u32;
    let result = if amount == 0 { ((op1 as i32) >> 31) as u32 } else { ((op1 as i32) >> amount) as u32 };
    let carry = (op1 >> if amount == 0 { 31 } else { amount - 1 }) & 1;
    ctx.set_reg(rd, result);
    set_nzc(ctx, result, carry);
    InstResult::Continue(1)
}

// ---------------------------------------------------------------------------------------------
// Three-register / three-bit-immediate add and sub (ADD/SUB Rd, Rs, Rn|#i).
// ---------------------------------------------------------------------------------------------

macro_rules! def_add_sub {
    ($name:ident, $op2:expr, $sub:expr) => {
        pub(super) fn $name(ctx: &mut Ctx, opcode: u16) -> InstResult {
            let rd = (opcode & 0x7) as u32;
            let op1 = ctx.reg(((opcode >> 3) & 0x7) as u32);
            let op2 = $op2(ctx, opcode);
            let result = if $sub { op1.wrapping_sub(op2) } else { op1.wrapping_add(op2) };
            ctx.set_reg(rd, result);
            if $sub {
                sub_flags(ctx, op1, op2, result);
            } else {
                add_flags(ctx, op1, op2, result);
            }
            InstResult::Continue(1)
        }
    };
}

fn reg_op2(ctx: &Ctx, opcode: u16) -> u32 {
    ctx.reg(((opcode >> 6) & 0x7) as u32)
}
fn imm3_op2(_ctx: &Ctx, opcode: u16) -> u32 {
    ((opcode >> 6) & 0x7) as u32
}

def_add_sub!(add_reg_t, reg_op2, false);
def_add_sub!(sub_reg_t, reg_op2, true);
def_add_sub!(add_imm3_t, imm3_op2, false);
def_add_sub!(sub_imm3_t, imm3_op2, true);

// ---------------------------------------------------------------------------------------------
// Eight-bit immediate ops (MOV/CMP/ADD/SUB Rd, #i), Rd in bits 8-10.
// ---------------------------------------------------------------------------------------------

pub(super) fn mov_imm8_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let result = (opcode & 0xFF) as u32;
    ctx.set_reg(((opcode >> 8) & 0x7) as u32, result);
    set_nz(ctx, result);
    InstResult::Continue(1)
}

pub(super) fn cmp_imm8_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let op1 = ctx.reg(((opcode >> 8) & 0x7) as u32);
    let op2 = (opcode & 0xFF) as u32;
    let result = op1.wrapping_sub(op2);
    sub_flags(ctx, op1, op2, result);
    InstResult::Continue(1)
}

pub(super) fn add_imm8_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = ((opcode >> 8) & 0x7) as u32;
    let op1 = ctx.reg(rd);
    let op2 = (opcode & 0xFF) as u32;
    let result = op1.wrapping_add(op2);
    ctx.set_reg(rd, result);
    add_flags(ctx, op1, op2, result);
    InstResult::Continue(1)
}

pub(super) fn sub_imm8_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = ((opcode >> 8) & 0x7) as u32;
    let op1 = ctx.reg(rd);
    let op2 = (opcode & 0xFF) as u32;
    let result = op1.wrapping_sub(op2);
    ctx.set_reg(rd, result);
    sub_flags(ctx, op1, op2, result);
    InstResult::Continue(1)
}

// ---------------------------------------------------------------------------------------------
// Register-to-register data processing (format 4), Rd in bits 0-2, Rs in bits 3-5.
// ---------------------------------------------------------------------------------------------

macro_rules! def_dp_logical {
    ($name:ident, |$a:ident, $b:ident| $body:expr) => {
        pub(super) fn $name(ctx: &mut Ctx, opcode: u16) -> InstResult {
            let rd = (opcode & 0x7) as u32;
            let $a = ctx.reg(rd);
            let $b = ctx.reg(((opcode >> 3) & 0x7) as u32);
            let result = $body;
            ctx.set_reg(rd, result);
            set_nz(ctx, result);
            InstResult::Continue(1)
        }
    };
}

def_dp_logical!(and_dp_t, |a, b| a & b);
def_dp_logical!(eor_dp_t, |a, b| a ^ b);
def_dp_logical!(orr_dp_t, |a, b| a | b);
def_dp_logical!(bic_dp_t, |a, b| a & !b);
def_dp_logical!(mvn_dp_t, |_a, b| !b);

pub(super) fn adc_dp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (opcode & 0x7) as u32;
    let a = ctx.reg(rd);
    let b = ctx.reg(((opcode >> 3) & 0x7) as u32);
    let result = do_adc(ctx, a, b, true);
    ctx.set_reg(rd, result);
    InstResult::Continue(1)
}

pub(super) fn sbc_dp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (opcode & 0x7) as u32;
    let a = ctx.reg(rd);
    let b = ctx.reg(((opcode >> 3) & 0x7) as u32);
    let result = do_sbc(ctx, a, b, true);
    ctx.set_reg(rd, result);
    InstResult::Continue(1)
}

pub(super) fn tst_dp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let a = ctx.reg((opcode & 0x7) as u32);
    let b = ctx.reg(((opcode >> 3) & 0x7) as u32);
    set_nz(ctx, a & b);
    InstResult::Continue(1)
}

pub(super) fn cmp_dp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let a = ctx.reg((opcode & 0x7) as u32);
    let b = ctx.reg(((opcode >> 3) & 0x7) as u32);
    let result = a.wrapping_sub(b);
    sub_flags(ctx, a, b, result);
    InstResult::Continue(1)
}

pub(super) fn cmn_dp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let a = ctx.reg((opcode & 0x7) as u32);
    let b = ctx.reg(((opcode >> 3) & 0x7) as u32);
    let result = a.wrapping_add(b);
    add_flags(ctx, a, b, result);
    InstResult::Continue(1)
}

pub(super) fn neg_dp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (opcode & 0x7) as u32;
    let b = ctx.reg(((opcode >> 3) & 0x7) as u32);
    let result = 0u32.wrapping_sub(b);
    ctx.set_reg(rd, result);
    sub_flags(ctx, 0, b, result);
    InstResult::Continue(1)
}

pub(super) fn mul_dp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (opcode & 0x7) as u32;
    let a = ctx.reg(rd);
    let b = ctx.reg(((opcode >> 3) & 0x7) as u32);
    let result = a.wrapping_mul(b);
    ctx.set_reg(rd, result);
    set_nz(ctx, result);
    InstResult::Continue(4)
}

// Shifts by register: the amount is the low byte of Rs; a zero amount leaves the flags untouched.

pub(super) fn lsl_dp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (opcode & 0x7) as u32;
    let op1 = ctx.reg(rd);
    let amount = ctx.reg(((opcode >> 3) & 0x7) as u32) & 0xFF;
    let result = if amount < 32 { op1 << amount } else { 0 };
    ctx.set_reg(rd, result);
    if amount == 0 {
        set_nz(ctx, result);
    } else {
        let carry = if amount <= 32 { (op1 >> (32 - amount)) & 1 } else { 0 };
        set_nzc(ctx, result, carry);
    }
    InstResult::Continue(1)
}

pub(super) fn lsr_dp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (opcode & 0x7) as u32;
    let op1 = ctx.reg(rd);
    let amount = ctx.reg(((opcode >> 3) & 0x7) as u32) & 0xFF;
    let result = if amount < 32 { op1 >> amount } else { 0 };
    ctx.set_reg(rd, result);
    if amount == 0 {
        set_nz(ctx, result);
    } else {
        let carry = if amount <= 32 { (op1 >> (amount - 1)) & 1 } else { 0 };
        set_nzc(ctx, result, carry);
    }
    InstResult::Continue(1)
}

pub(super) fn asr_dp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (opcode & 0x7) as u32;
    let op1 = ctx.reg(rd);
    let amount = ctx.reg(((opcode >> 3) & 0x7) as u32) & 0xFF;
    let result = if amount < 32 { ((op1 as i32) >> amount) as u32 } else { ((op1 as i32) >> 31) as u32 };
    ctx.set_reg(rd, result);
    if amount == 0 {
        set_nz(ctx, result);
    } else {
        let carry = (op1 >> if amount <= 32 { amount - 1 } else { 31 }) & 1;
        set_nzc(ctx, result, carry);
    }
    InstResult::Continue(1)
}

pub(super) fn ror_dp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (opcode & 0x7) as u32;
    let op1 = ctx.reg(rd);
    let amount = ctx.reg(((opcode >> 3) & 0x7) as u32) & 0xFF;
    let result = op1.rotate_right(amount & 0x1F);
    ctx.set_reg(rd, result);
    if amount == 0 {
        set_nz(ctx, result);
    } else {
        let carry = (op1 >> ((amount - 1) & 0x1F)) & 1;
        set_nzc(ctx, result, carry);
    }
    InstResult::Continue(1)
}

// ---------------------------------------------------------------------------------------------
// Hi register operations (format 5, no flags) and SP/PC-relative adds.
// ---------------------------------------------------------------------------------------------

pub(super) fn add_h_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (((opcode >> 4) & 0x8) | (opcode & 0x7)) as u32;
    let result = ctx.reg_t(rd).wrapping_add(ctx.reg_t(((opcode >> 3) & 0xF) as u32));
    if rd == 15 {
        InstResult::Branch(1, (result & !1) | 1)
    } else {
        ctx.set_reg(rd, result);
        InstResult::Continue(1)
    }
}

pub(super) fn cmp_h_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let op1 = ctx.reg_t((((opcode >> 4) & 0x8) | (opcode & 0x7)) as u32);
    let op2 = ctx.reg_t(((opcode >> 3) & 0xF) as u32);
    let result = op1.wrapping_sub(op2);
    sub_flags(ctx, op1, op2, result);
    InstResult::Continue(1)
}

pub(super) fn mov_h_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let rd = (((opcode >> 4) & 0x8) | (opcode & 0x7)) as u32;
    let rs = ((opcode >> 3) & 0xF) as u32;
    let result = ctx.reg_t(rs);
    if rd == 15 {
        // MOV to PC does not interwork on thumb, the mode stays. `mov pc, lr` is a return; the
        // jit routes it through the return stack (branch_lr).
        if rs == 14 {
            InstResult::BranchReturn(1, (result & !1) | 1)
        } else {
            InstResult::Branch(1, (result & !1) | 1)
        }
    } else {
        ctx.set_reg(rd, result);
        InstResult::Continue(1)
    }
}

pub(super) fn add_pc_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let base = (ctx.inst_addr + 4) & !3;
    let result = base.wrapping_add(((opcode & 0xFF) as u32) << 2);
    ctx.set_reg(((opcode >> 8) & 0x7) as u32, result);
    InstResult::Continue(1)
}

pub(super) fn add_sp_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let result = ctx.reg(13).wrapping_add(((opcode & 0xFF) as u32) << 2);
    ctx.set_reg(((opcode >> 8) & 0x7) as u32, result);
    InstResult::Continue(1)
}

pub(super) fn add_sp_imm_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let imm = ((opcode & 0x7F) as u32) << 2;
    let sp = ctx.reg(13);
    let result = if opcode & (1 << 7) != 0 { sp.wrapping_sub(imm) } else { sp.wrapping_add(imm) };
    ctx.set_reg(13, result);
    InstResult::Continue(1)
}
