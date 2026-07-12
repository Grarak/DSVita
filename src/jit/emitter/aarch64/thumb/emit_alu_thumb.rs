// The aarch64 thumb data-processing lowering: the flag-setting
// low-register ALU maps onto the shared W-form S-ops and cpsr writeback helpers from
// emit_alu; thumb has no shifted operand-2, so the dedicated shift instructions carry
// the ARM immediate-shift carry rules themselves.

use super::super::emit_alu::{seed_carry, writeback_arith_flags, writeback_logical_flags, CarryOut};
use crate::jit::assembler::aarch64::{BlockAsm, SCRATCH0, SCRATCH1, SCRATCH2};
use crate::jit::inst_info::{InstInfo, Operand};
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use vixl::{A64Reg, A64ShiftKind};

use super::super::SCRATCH4;

/// Thumb source read: the pipeline exposes PC as the instruction address + 4.
fn thumb_source_reg(block_asm: &mut BlockAsm, scratch: A64Reg, guest: Reg, pc: u32) -> A64Reg {
    if guest == Reg::PC {
        block_asm.mov_imm(scratch, pc + 4);
        scratch
    } else {
        block_asm.guest_map(guest)
    }
}

/// Materialize a thumb operand (plain register or immediate — thumb has no shifted
/// operand-2; the dedicated shift instructions are lowered by the caller).
fn thumb_operand(block_asm: &mut BlockAsm, operand: &Operand, pc: u32) -> A64Reg {
    match operand {
        Operand::Imm(imm) => {
            block_asm.mov_imm(SCRATCH1, *imm);
            SCRATCH1
        }
        Operand::Reg { reg, shift: None } => thumb_source_reg(block_asm, SCRATCH1, *reg, pc),
        _ => unreachable!(),
    }
}

/// Lower one thumb data-processing instruction (the branch kinds are handled by
/// emit_branch). Everything is flag-setting unless noted; the writeback helpers are the
/// same ones the ARM path uses, so the cpsr merge rules stay identical.
pub(in super::super) fn emit_thumb_data_processing(block_asm: &mut BlockAsm, inst: &InstInfo, pc: u32) {
    let operands = inst.operands();
    let result = SCRATCH2;
    match inst.op {
        // adds/subs rd, rn, (rm|imm) — also the 2-register imm8 forms ([rd, rd, imm8]).
        Op::AddT | Op::SubT => {
            let dst = operands[0].as_reg_no_shift().unwrap();
            let result = block_asm.guest_map(dst);
            let rhs = thumb_operand(block_asm, &operands[2], pc);
            let lhs = thumb_source_reg(block_asm, SCRATCH0, operands[1].as_reg_no_shift().unwrap(), pc);
            if inst.op == Op::AddT {
                block_asm.masm.adds_reg(result, lhs, rhs, A64ShiftKind::LSL, 0, false);
            } else {
                block_asm.masm.subs_reg(result, lhs, rhs, A64ShiftKind::LSL, 0, false);
            }
            writeback_arith_flags(block_asm);
        }
        // negs rd, rm = rsbs rd, rm, #0.
        Op::NegT => {
            let dst = operands[0].as_reg_no_shift().unwrap();
            let result = block_asm.guest_map(dst);
            let rhs = thumb_source_reg(block_asm, SCRATCH1, operands[1].as_reg_no_shift().unwrap(), pc);
            block_asm.masm.subs_reg(result, A64Reg::ZR, rhs, A64ShiftKind::LSL, 0, false);
            writeback_arith_flags(block_asm);
        }
        // add rd, rm (high registers, no flags). A pc destination goes through a scratch
        // into the guest PC slot for the driver's indirect-branch dispatch.
        Op::AddHT => {
            let dst = operands[0].as_reg_no_shift().unwrap();
            let result = if dst == Reg::PC { SCRATCH2 } else { block_asm.guest_map(dst) };
            let rhs = thumb_operand(block_asm, &operands[2], pc);
            let lhs = thumb_source_reg(block_asm, SCRATCH0, operands[1].as_reg_no_shift().unwrap(), pc);
            block_asm.masm.add_reg(result, lhs, rhs, A64ShiftKind::LSL, 0, false);
            if dst == Reg::PC {
                block_asm.store_guest(SCRATCH2, Reg::PC);
            }
        }
        // cmp/cmn rn, (rm|imm8).
        Op::CmpT | Op::CmpHT | Op::CmnT => {
            let rhs = thumb_operand(block_asm, &operands[1], pc);
            let lhs = thumb_source_reg(block_asm, SCRATCH0, operands[0].as_reg_no_shift().unwrap(), pc);
            if inst.op == Op::CmnT {
                block_asm.masm.adds_reg(result, lhs, rhs, A64ShiftKind::LSL, 0, false);
            } else {
                block_asm.masm.subs_reg(result, lhs, rhs, A64ShiftKind::LSL, 0, false);
            }
            writeback_arith_flags(block_asm);
        }
        // movs rd, #imm8: N/Z from the value, C/V untouched.
        Op::MovT => {
            let dst = operands[0].as_reg_no_shift().unwrap();
            let result = block_asm.guest_map(dst);
            let rhs = thumb_operand(block_asm, &operands[1], pc);
            block_asm.masm.orr_reg(result, A64Reg::ZR, rhs, A64ShiftKind::LSL, 0, false);
            writeback_logical_flags(block_asm, result, CarryOut::Unchanged);
        }
        // mov rd, rm (high registers, no flags).
        Op::MovHT => {
            let dst = operands[0].as_reg_no_shift().unwrap();
            let result = if dst == Reg::PC { SCRATCH2 } else { block_asm.guest_map(dst) };
            let rhs = thumb_operand(block_asm, &operands[1], pc);
            block_asm.masm.orr_reg(result, A64Reg::ZR, rhs, A64ShiftKind::LSL, 0, false);
            if dst == Reg::PC {
                block_asm.store_guest(SCRATCH2, Reg::PC);
            }
        }
        // mvns rd, rm.
        Op::MvnT => {
            let dst = operands[0].as_reg_no_shift().unwrap();
            let result = block_asm.guest_map(dst);
            let rhs = thumb_operand(block_asm, &operands[1], pc);
            block_asm.masm.orr_reg(result, A64Reg::ZR, rhs, A64ShiftKind::LSL, 0, false);
            block_asm.masm.eor_imm(result, result, u32::MAX as u64, false);
            writeback_logical_flags(block_asm, result, CarryOut::Unchanged);
        }
        // The immediate shifts lower like ARM MOVS rd, rm <shift> #imm: same encoded-zero
        // rules (LSR/ASR 0 = full 32-bit shift, LSL 0 = plain move with C untouched) and
        // the same pre-shift carry extract.
        Op::LslT | Op::LsrT | Op::AsrT => {
            let dst = operands[0].as_reg_no_shift().unwrap();
            let result = block_asm.guest_map(dst);
            let src = thumb_source_reg(block_asm, SCRATCH1, operands[1].as_reg_no_shift().unwrap(), pc);
            let imm = operands[2].as_imm().unwrap();
            let (kind, amount, carry_bit) = match inst.op {
                Op::LslT => {
                    if imm == 0 {
                        block_asm.masm.orr_reg(result, A64Reg::ZR, src, A64ShiftKind::LSL, 0, false);
                        writeback_logical_flags(block_asm, result, CarryOut::Unchanged);
                        return;
                    }
                    (A64ShiftKind::LSL, imm, 32 - imm)
                }
                Op::LsrT => {
                    let n = if imm == 0 { 32 } else { imm };
                    (A64ShiftKind::LSR, n, n - 1)
                }
                _ => {
                    let n = if imm == 0 { 32 } else { imm };
                    (A64ShiftKind::ASR, n, n - 1)
                }
            };
            block_asm.masm.ubfx(SCRATCH4, src, carry_bit.min(31), 1, false);
            if amount == 32 {
                block_asm.shift_reg_imm(result, src, kind, 31);
                block_asm.shift_reg_imm(result, result, kind, 1);
            } else {
                block_asm.shift_reg_imm(result, src, kind, amount);
            }
            writeback_logical_flags(block_asm, result, CarryOut::Dynamic);
        }
        // ands/eors/orrs/bics rd, rm — no shifter, C/V untouched.
        Op::AndT | Op::EorT | Op::OrrT | Op::BicT => {
            let dst = operands[0].as_reg_no_shift().unwrap();
            let result = block_asm.guest_map(dst);
            let rhs = thumb_operand(block_asm, &operands[2], pc);
            let lhs = thumb_source_reg(block_asm, SCRATCH0, operands[1].as_reg_no_shift().unwrap(), pc);
            match inst.op {
                Op::AndT => block_asm.masm.and_reg(result, lhs, rhs, A64ShiftKind::LSL, 0, false),
                Op::EorT => block_asm.masm.eor_reg(result, lhs, rhs, A64ShiftKind::LSL, 0, false),
                Op::OrrT => block_asm.masm.orr_reg(result, lhs, rhs, A64ShiftKind::LSL, 0, false),
                _ => block_asm.masm.bic_reg(result, lhs, rhs, A64ShiftKind::LSL, 0, false),
            }
            writeback_logical_flags(block_asm, result, CarryOut::Unchanged);
        }
        // tst rn, rm.
        Op::TstT => {
            let rhs = thumb_operand(block_asm, &operands[1], pc);
            let lhs = thumb_source_reg(block_asm, SCRATCH0, operands[0].as_reg_no_shift().unwrap(), pc);
            block_asm.masm.and_reg(result, lhs, rhs, A64ShiftKind::LSL, 0, false);
            writeback_logical_flags(block_asm, result, CarryOut::Unchanged);
        }
        // adcs/sbcs rd, rm: seed host C from the guest cpsr first.
        Op::AdcT | Op::SbcT => {
            let dst = operands[0].as_reg_no_shift().unwrap();
            let result = block_asm.guest_map(dst);
            let rhs = thumb_operand(block_asm, &operands[2], pc);
            let lhs = thumb_source_reg(block_asm, SCRATCH0, operands[1].as_reg_no_shift().unwrap(), pc);
            seed_carry(block_asm);
            if inst.op == Op::AdcT {
                block_asm.masm.adcs(result, lhs, rhs, false);
            } else {
                block_asm.masm.sbcs(result, lhs, rhs, false);
            }
            writeback_arith_flags(block_asm);
        }
        // muls rd, rm: N/Z from the result, C/V untouched (the interpreter's rule).
        Op::MulT => {
            let dst = operands[0].as_reg_no_shift().unwrap();
            let result = block_asm.guest_map(dst);
            let rhs = thumb_operand(block_asm, &operands[1], pc);
            let lhs = thumb_source_reg(block_asm, SCRATCH0, dst, pc);
            block_asm.masm.mul(result, lhs, rhs, false);
            writeback_logical_flags(block_asm, result, CarryOut::Unchanged);
        }
        // adr: rd = align4(pc + 4) + imm — a compile-time constant.
        Op::AddPcT => {
            let dst = operands[0].as_reg_no_shift().unwrap();
            let result = block_asm.guest_map(dst);
            let imm = operands[2].as_imm().unwrap();
            block_asm.mov_imm(result, ((pc + 4) & !3).wrapping_add(imm));
        }
        // add rd, sp, #imm / add sp, #±imm (the sign only survives in the raw opcode).
        Op::AddSpT | Op::AddSpImmT => {
            let dst = operands[0].as_reg_no_shift().unwrap();
            let result = block_asm.guest_map(dst);
            let imm = operands[2].as_imm().unwrap();
            let sp = block_asm.guest_map(Reg::SP);
            if inst.op == Op::AddSpImmT && inst.opcode & (1 << 7) != 0 {
                block_asm.masm.sub_imm(result, sp, imm as u64, false);
            } else {
                block_asm.masm.add_imm(result, sp, imm as u64, false);
            }
        }
        Op::BlSetupT => {}
        // BLX on the ARM7 is a no-op (NooDS rule; the interpreter logs it as executed).
        Op::BlxOffT | Op::BlxRegT => {}
        _ => unreachable!(),
    }
}
