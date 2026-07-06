// The aarch64 block driver, stage-5 slices 1-3: walks the decoded instructions, lowers
// data-processing through emit_alu and branches through emit_branch, and owns the
// support check — everything refused by `is_block_jit_supported` stays interpreted, the
// permanent safety valve (plan S5.3), plus the DSVITA_A64_DISABLE bisect classes.

use super::emit_alu::{emit_data_processing, op_class, FlagMode};
use super::emit_branch::{SchedBlock, TakenBranch};
use super::SCRATCH3;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::aarch64::A64BlockAsm;
use crate::jit::inst_info::{InstInfo, Operand, Shift, ShiftValue};
use crate::jit::jit_asm::{debug_after_exec_op, JitAsm};
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::Cond;
use crate::settings::Arm7Emu;
use crate::DEBUG_LOG;
use vixl::{A64Label, A64Reg};

/// Bisect mask for divergence hunts: DSVITA_A64_DISABLE=c,l,a,i,b disables conditional
/// execution / logical-S / arith-S / carry-in ops / non-final branches respectively;
/// 'v' additionally skips the forward-local-branch validity check.
pub(super) fn class_disabled(class: char) -> bool {
    static MASK: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MASK.get_or_init(|| std::env::var("DSVITA_A64_DISABLE").unwrap_or_default()).contains(class)
}

fn is_inst_supported(inst: &InstInfo) -> bool {
    if inst.cond == Cond::NV {
        return false;
    }
    if inst.cond != Cond::AL && class_disabled('c') {
        return false;
    }
    if let Some((_, flag_mode, carry_in)) = op_class(inst.op) {
        if flag_mode == FlagMode::Logical && class_disabled('l') {
            return false;
        }
        if flag_mode == FlagMode::Arith && class_disabled('a') {
            return false;
        }
        if carry_in && class_disabled('i') {
            return false;
        }
    }
    let Some((_, flag_mode, _)) = op_class(inst.op) else {
        // Branches: any condition, local or external target (slice 3).
        return inst.op == Op::B;
    };
    if inst.out_regs.is_reserved(Reg::PC) {
        return false;
    }
    for operand in inst.operands() {
        if let Operand::Reg { shift, .. } = operand {
            match shift {
                None => {}
                Some(Shift::Lsl(ShiftValue::Imm(_))) | Some(Shift::Lsr(ShiftValue::Imm(_))) | Some(Shift::Asr(ShiftValue::Imm(_))) => {}
                Some(Shift::Ror(ShiftValue::Imm(imm))) => {
                    // ROR #0 encodes RRX (carry-in shifter) — unsupported.
                    if *imm == 0 {
                        return false;
                    }
                }
                // Shift-by-register: dynamic amounts change the carry rules (amount 0
                // leaves C untouched) — refused until a dedicated slice.
                _ => return false,
            }
        }
    }
    let _ = flag_mode;
    true
}

/// A block is compilable when every instruction is supported, the block is ARM-mode, and
/// it ENDS in an unconditional AL B (local or external): control then never falls off the
/// end, and at the external exit the interpreter charges the same +2 epilogue and checks
/// the scheduler at the same boundary. Conditional and mid-block Bs are fine since slice 3.
/// (Thumb lands in a later slice.)
pub fn is_block_jit_supported(insts: &[InstInfo], thumb: bool) -> bool {
    if thumb || insts.is_empty() {
        return false;
    }
    let last = insts.len() - 1;
    if insts[last].op != Op::B || insts[last].cond != Cond::AL {
        return false;
    }
    for (i, inst) in insts.iter().enumerate() {
        if !is_inst_supported(inst) {
            return false;
        }
        // Bisect valve: DSVITA_A64_DISABLE=b falls back to the slices-1/2 shape (a single
        // final AL B, nothing else).
        if inst.op == Op::B && class_disabled('b') && (i != last || inst.cond != Cond::AL) {
            return false;
        }
    }
    true
}

impl JitAsm<'_> {
    /// Emit the whole block (already validated by is_block_jit_supported).
    pub fn emit_a64(&mut self, block_asm: &mut A64BlockAsm, guest_pc: u32, guest_pc_end: u32) {
        let arm7_hle = self.emu.settings.arm7_emu() == Arm7Emu::Hle;
        let insts_len = self.jit_buf.insts.len();
        debug_assert_eq!(guest_pc_end, guest_pc + insts_len as u32 * 4);

        let mut inst_labels: Vec<A64Label> = (0..insts_len).map(|_| A64Label::new()).collect();
        let mut taken_branches: Vec<TakenBranch> = Vec::new();
        let mut sched_blocks: Vec<SchedBlock> = Vec::new();

        for i in 0..insts_len {
            block_asm.masm.bind(&mut inst_labels[i]);
            block_asm.record_inst_offset();

            let inst = &self.jit_buf.insts[i];
            let pc = guest_pc + (i as u32) * 4;
            let op = inst.op;
            let opcode = inst.opcode;
            let cond = inst.cond;

            if op == Op::B {
                let relative = inst.operands()[0].as_imm().unwrap() as i32;
                let target = pc.wrapping_add(8).wrapping_add_signed(relative) & !3;
                if cond == Cond::AL {
                    // Always taken: never logged (matching both existing engines), no
                    // fall-through — anything after is only reachable via labels.
                    self.emit_branch(block_asm, i, target, guest_pc, insts_len, arm7_hle, &mut inst_labels, &mut sched_blocks);
                    continue;
                }
                // Taken path out of line; the fall-through is a condition-failed
                // instruction and gets logged like any other (the interpreter logs it).
                let mut taken = A64Label::new();
                block_asm.load_cpsr(SCRATCH3);
                block_asm.masm.msr_nzcv(SCRATCH3);
                block_asm.masm.b_cond(&mut taken, cond);
                taken_branches.push(TakenBranch { inst_index: i, target, label: taken });
            } else {
                // Conditional execution: one b.cond over the body on the inverted guest
                // condition; the debug hook stays outside (the interpreter logs
                // condition-failed instructions too).
                let mut skip = A64Label::new();
                if cond != Cond::AL {
                    block_asm.load_cpsr(SCRATCH3);
                    block_asm.masm.msr_nzcv(SCRATCH3);
                    block_asm.masm.b_cond(&mut skip, !cond);
                }
                emit_data_processing(block_asm, &self.jit_buf.insts[i], pc);
                if cond != Cond::AL {
                    block_asm.masm.bind(&mut skip);
                }
            }

            // Post-state per-instruction log, mirroring the interpreter's DEBUG_LOG hook so
            // mixed jit/interp traces stay comparable record-for-record. Taken branches are
            // not logged (matching both existing engines).
            if DEBUG_LOG {
                block_asm.mov_imm(A64Reg::X0, pc);
                block_asm.mov_imm(A64Reg::X1, opcode);
                block_asm.call_host(match self.cpu {
                    ARM9 => debug_after_exec_op::<{ ARM9 }> as *const (),
                    ARM7 => debug_after_exec_op::<{ ARM7 }> as *const (),
                });
            }
        }

        // Out-of-line taken paths of conditional branches.
        for mut taken in taken_branches {
            block_asm.masm.bind(&mut taken.label);
            self.emit_branch(block_asm, taken.inst_index, taken.target, guest_pc, insts_len, arm7_hle, &mut inst_labels, &mut sched_blocks);
        }

        // Out-of-line scheduler tails of local branches.
        for mut sched in sched_blocks {
            self.emit_local_branch_sched_tail(block_asm, &mut sched, guest_pc, arm7_hle);
        }
    }
}
