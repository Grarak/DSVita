// The aarch64 data-processing lowering, stage-5 slice 2: ALU ops with and without flag
// updates, comparisons, and the A32 operand-2 shifter.
//
// Flag strategy: guest NZCV lives in the stored cpsr word, not in host flags — every
// S-op computes its new bits and merges them immediately, every conditional instruction
// loads the cpsr into host NZCV for one b.cond. Call-safe and allocator-free; keeping
// flags resident in host NZCV across instructions is a later optimization slice.
// The A32 shifter carry-out is computed explicitly (the plan's top-ranked risk): statically
// for rotated immediates (rotation recovered from the raw opcode — Operand::Imm loses it),
// and as a pre-shift bit extract for immediate-amount register shifts. Shift-by-register
// forms stay refused for S-ops.

use super::{SCRATCH3, SCRATCH4};
use crate::jit::assembler::aarch64::{A64BlockAsm, SCRATCH0, SCRATCH1, SCRATCH2};
use crate::jit::inst_info::{InstInfo, Operand, Shift, ShiftValue};
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use vixl::{A64Reg, A64ShiftKind};

const N_BIT: u32 = 1 << 31;
const Z_BIT: u32 = 1 << 30;
const C_BIT: u32 = 1 << 29;

/// What the instruction does to the guest flags.
#[derive(Copy, Clone, PartialEq)]
pub(super) enum FlagMode {
    None,
    /// A64 W-form S-op produces exactly the A32 NZCV (add/sub families).
    Arith,
    /// N/Z from the result, C from the shifter (V untouched).
    Logical,
}

pub(super) fn op_class(op: Op) -> Option<(Op, FlagMode, bool)> {
    // (base op used for emission, flag mode, wants carry-in)
    Some(match op {
        Op::Mov | Op::Mvn | Op::And | Op::Orr | Op::Eor | Op::Add | Op::Sub | Op::Rsb | Op::Bic => (op, FlagMode::None, false),
        Op::Adc | Op::Sbc | Op::Rsc => (op, FlagMode::None, true),
        Op::Movs => (Op::Mov, FlagMode::Logical, false),
        Op::Mvns => (Op::Mvn, FlagMode::Logical, false),
        Op::Ands => (Op::And, FlagMode::Logical, false),
        Op::Orrs => (Op::Orr, FlagMode::Logical, false),
        Op::Eors => (Op::Eor, FlagMode::Logical, false),
        Op::Bics => (Op::Bic, FlagMode::Logical, false),
        Op::Tst => (Op::And, FlagMode::Logical, false),
        Op::Teq => (Op::Eor, FlagMode::Logical, false),
        Op::Adds => (Op::Add, FlagMode::Arith, false),
        Op::Subs => (Op::Sub, FlagMode::Arith, false),
        Op::Rsbs => (Op::Rsb, FlagMode::Arith, false),
        Op::Cmp => (Op::Sub, FlagMode::Arith, false),
        Op::Cmn => (Op::Add, FlagMode::Arith, false),
        Op::Adcs => (Op::Adc, FlagMode::Arith, true),
        Op::Sbcs => (Op::Sbc, FlagMode::Arith, true),
        Op::Rscs => (Op::Rsc, FlagMode::Arith, true),
        _ => return None,
    })
}

fn has_dst(op: Op) -> bool {
    !matches!(op, Op::Tst | Op::Teq | Op::Cmp | Op::Cmn)
}

fn source_reg(block_asm: &mut A64BlockAsm, dst: A64Reg, guest: Reg, pc: u32) -> A64Reg {
    if guest == Reg::PC {
        // ARM pipeline: PC reads as the instruction address + 8.
        block_asm.mov_imm(dst, pc + 8);
    } else {
        block_asm.load_guest(dst, guest);
    }
    dst
}

/// Shifter carry-out of operand 2, when the flags want it.
enum CarryOut {
    Unchanged,
    Static(bool),
    /// 0/1 in SCRATCH4.
    Dynamic,
}

/// Materialize operand 2 into a register; when `want_carry`, also produce the shifter
/// carry-out per the A32 rules.
fn op2(block_asm: &mut A64BlockAsm, operand: &Operand, opcode: u32, pc: u32, want_carry: bool) -> (A64Reg, CarryOut) {
    match operand {
        Operand::Imm(imm) => {
            block_asm.mov_imm(SCRATCH1, *imm);
            let carry = if !want_carry {
                CarryOut::Unchanged
            } else {
                // Rotated immediate: C = bit31 of the value when the encoded rotation is
                // non-zero, otherwise untouched. The rotation only survives in the opcode.
                let rot = (opcode >> 8) & 0xF;
                if rot == 0 {
                    CarryOut::Unchanged
                } else {
                    CarryOut::Static(imm & N_BIT != 0)
                }
            };
            (SCRATCH1, carry)
        }
        Operand::Reg { reg, shift: None } => (source_reg(block_asm, SCRATCH1, *reg, pc), CarryOut::Unchanged),
        Operand::Reg { reg, shift: Some(shift) } => {
            let src = source_reg(block_asm, SCRATCH1, *reg, pc);
            let (kind, amount, carry_bit) = match shift {
                Shift::Lsl(ShiftValue::Imm(0)) => {
                    return (src, CarryOut::Unchanged);
                }
                Shift::Lsl(ShiftValue::Imm(imm)) => (A64ShiftKind::LSL, *imm as u32, 32 - *imm as u32),
                // Encoded amount 0 means a full 32-bit shift for LSR/ASR.
                Shift::Lsr(ShiftValue::Imm(imm)) => {
                    let n = if *imm == 0 { 32 } else { *imm as u32 };
                    (A64ShiftKind::LSR, n, n - 1)
                }
                Shift::Asr(ShiftValue::Imm(imm)) => {
                    let n = if *imm == 0 { 32 } else { *imm as u32 };
                    (A64ShiftKind::ASR, n, n - 1)
                }
                Shift::Ror(ShiftValue::Imm(imm)) => (A64ShiftKind::ROR, *imm as u32, *imm as u32 - 1),
                _ => unreachable!(),
            };
            let carry = if want_carry {
                block_asm.masm.ubfx(SCRATCH4, src, carry_bit.min(31), 1, false);
                CarryOut::Dynamic
            } else {
                CarryOut::Unchanged
            };
            if amount == 32 {
                // A64 32-bit shifts take amounts 0-31: split the full shift in two.
                block_asm.shift_reg_imm(SCRATCH1, src, kind, 31);
                block_asm.shift_reg_imm(SCRATCH1, SCRATCH1, kind, 1);
            } else {
                block_asm.shift_reg_imm(SCRATCH1, src, kind, amount);
            }
            (SCRATCH1, carry)
        }
        _ => unreachable!(),
    }
}

/// Seed the host carry flag from the guest cpsr (for adc/sbc/rsc).
fn seed_carry(block_asm: &mut A64BlockAsm) {
    block_asm.load_cpsr(SCRATCH3);
    block_asm.masm.msr_nzcv(SCRATCH3);
}

/// Merge fresh arithmetic NZCV (currently in host flags) into the guest cpsr.
fn writeback_arith_flags(block_asm: &mut A64BlockAsm) {
    block_asm.masm.mrs_nzcv(SCRATCH3);
    block_asm.load_cpsr(SCRATCH4);
    block_asm.masm.and_imm(SCRATCH4, SCRATCH4, 0x0FFFFFFF, false);
    block_asm.masm.and_imm(SCRATCH3, SCRATCH3, 0xF0000000, false);
    block_asm.masm.orr_reg(SCRATCH4, SCRATCH4, SCRATCH3, A64ShiftKind::LSL, 0, false);
    block_asm.store_cpsr(SCRATCH4);
}

/// Merge logical-op flags: N/Z from `result`, C per the shifter carry, V untouched.
fn writeback_logical_flags(block_asm: &mut A64BlockAsm, result: A64Reg, carry: CarryOut) {
    // N/Z via a host tst, captured from NZCV.
    block_asm.masm.tst_reg(result, result, A64ShiftKind::LSL, 0, false);
    block_asm.masm.mrs_nzcv(SCRATCH3);
    block_asm.load_cpsr(SCRATCH0);
    let clear: u32 = match carry {
        CarryOut::Unchanged => !(N_BIT | Z_BIT),
        _ => !(N_BIT | Z_BIT | C_BIT),
    };
    block_asm.masm.and_imm(SCRATCH0, SCRATCH0, clear as u64, false);
    block_asm.masm.and_imm(SCRATCH3, SCRATCH3, (N_BIT | Z_BIT) as u64, false);
    block_asm.masm.orr_reg(SCRATCH0, SCRATCH0, SCRATCH3, A64ShiftKind::LSL, 0, false);
    match carry {
        CarryOut::Unchanged => {}
        CarryOut::Static(true) => block_asm.masm.orr_imm(SCRATCH0, SCRATCH0, C_BIT as u64, false),
        CarryOut::Static(false) => {}
        CarryOut::Dynamic => {
            // C_BIT | (carry01 << 29)
            block_asm.masm.orr_reg(SCRATCH0, SCRATCH0, SCRATCH4, A64ShiftKind::LSL, 29, false);
        }
    }
    block_asm.store_cpsr(SCRATCH0);
}

pub(super) fn emit_data_processing(block_asm: &mut A64BlockAsm, inst: &InstInfo, pc: u32) {
    let (base_op, flag_mode, carry_in) = op_class(inst.op).unwrap();
    let operands = inst.operands();
    let want_carry = flag_mode == FlagMode::Logical;

    // Operand layout: [rd, op2] for mov/mvn, [rn, op2] for tst/teq/cmp/cmn,
    // [rd, rn, op2] for the rest.
    let (dst, lhs_reg, op2_operand) = if matches!(base_op, Op::Mov | Op::Mvn) {
        (operands[0].as_reg_no_shift(), None, &operands[1])
    } else if has_dst(inst.op) {
        (operands[0].as_reg_no_shift(), operands[1].as_reg_no_shift(), &operands[2])
    } else {
        (None, operands[0].as_reg_no_shift(), &operands[1])
    };

    let (rhs, carry) = op2(block_asm, op2_operand, inst.opcode, pc, want_carry);
    let lhs = lhs_reg.map(|reg| source_reg(block_asm, SCRATCH0, reg, pc));

    if carry_in {
        seed_carry(block_asm);
    }

    let set_host_flags = flag_mode == FlagMode::Arith;
    let result = SCRATCH2;
    match (base_op, set_host_flags) {
        (Op::Mov, _) => block_asm.masm.orr_reg(result, A64Reg::ZR, rhs, A64ShiftKind::LSL, 0, false),
        (Op::Mvn, _) => {
            block_asm.masm.orr_reg(result, A64Reg::ZR, rhs, A64ShiftKind::LSL, 0, false);
            block_asm.masm.eor_imm(result, result, u32::MAX as u64, false);
        }
        (Op::And, _) => block_asm.masm.and_reg(result, lhs.unwrap(), rhs, A64ShiftKind::LSL, 0, false),
        (Op::Orr, _) => block_asm.masm.orr_reg(result, lhs.unwrap(), rhs, A64ShiftKind::LSL, 0, false),
        (Op::Eor, _) => block_asm.masm.eor_reg(result, lhs.unwrap(), rhs, A64ShiftKind::LSL, 0, false),
        (Op::Bic, _) => block_asm.masm.bic_reg(result, lhs.unwrap(), rhs, A64ShiftKind::LSL, 0, false),
        (Op::Add, false) => block_asm.masm.add_reg(result, lhs.unwrap(), rhs, A64ShiftKind::LSL, 0, false),
        (Op::Add, true) => block_asm.masm.adds_reg(result, lhs.unwrap(), rhs, A64ShiftKind::LSL, 0, false),
        (Op::Sub, false) => block_asm.masm.sub_reg(result, lhs.unwrap(), rhs, A64ShiftKind::LSL, 0, false),
        (Op::Sub, true) => block_asm.masm.subs_reg(result, lhs.unwrap(), rhs, A64ShiftKind::LSL, 0, false),
        (Op::Rsb, false) => block_asm.masm.sub_reg(result, rhs, lhs.unwrap(), A64ShiftKind::LSL, 0, false),
        (Op::Rsb, true) => block_asm.masm.subs_reg(result, rhs, lhs.unwrap(), A64ShiftKind::LSL, 0, false),
        (Op::Adc, false) => block_asm.masm.adc(result, lhs.unwrap(), rhs, false),
        (Op::Adc, true) => block_asm.masm.adcs(result, lhs.unwrap(), rhs, false),
        (Op::Sbc, false) => block_asm.masm.sbc(result, lhs.unwrap(), rhs, false),
        (Op::Sbc, true) => block_asm.masm.sbcs(result, lhs.unwrap(), rhs, false),
        (Op::Rsc, false) => block_asm.masm.sbc(result, rhs, lhs.unwrap(), false),
        (Op::Rsc, true) => block_asm.masm.sbcs(result, rhs, lhs.unwrap(), false),
        _ => unreachable!(),
    }

    match flag_mode {
        FlagMode::None => {}
        FlagMode::Arith => writeback_arith_flags(block_asm),
        FlagMode::Logical => writeback_logical_flags(block_asm, result, carry),
    }

    if let Some(dst) = dst {
        if has_dst(inst.op) {
            block_asm.store_guest(result, dst);
        }
    }
}
