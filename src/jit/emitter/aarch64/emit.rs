// The aarch64 block driver, stage-5 slices 1-3: walks the decoded instructions, lowers
// data-processing through emit_alu and branches through emit_branch, and owns the
// support check — everything refused by `is_block_jit_supported` stays interpreted, the
// permanent safety valve (plan S5.3), plus the DSVITA_A64_DISABLE bisect classes.

use super::emit_alu::{emit_data_processing, op_class, FlagMode};
use super::emit_branch::{BranchKind, SchedBlock, TakenBranch};
use super::thumb;
use super::SCRATCH3;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::aarch64::BlockAsm;
use crate::jit::inst_info::{InstInfo, Operand, Shift, ShiftValue};
use crate::jit::jit_asm::{debug_after_exec_op, JitAsm};
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use crate::jit::Cond;
use crate::settings::Arm7Emu;
use crate::DEBUG_LOG;
use vixl::{A64Label, A64Reg};

/// Bisect mask for divergence hunts: DSVITA_A64_DISABLE=c,l,a,i,b,t disables conditional
/// execution / logical-S / arith-S / carry-in ops / non-final branches / thumb blocks
/// respectively; 'v' additionally skips the forward-local-branch validity check.
pub fn class_disabled(class: char) -> bool {
    static MASK: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    if !crate::IS_DEBUG {
        return false;
    }
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
        // Branches: B/BL any condition (local or external targets), BX/BLX-reg dynamic
        // targets through the shared runtimes. BLX imm decodes as cond 0xF and
        // fill_jit_insts_buf stops before it — it never reaches the emitter.
        if matches!(inst.op, Op::B | Op::Bl | Op::Bx | Op::BlxReg) {
            return true;
        }
        // Single transfers: the fastmem window + SIGSEGV slow-path patching.
        if inst.op.is_single_mem_transfer() && !class_disabled('m') {
            return super::emit_transfer::is_single_transfer_supported(inst, false);
        }
        // Multiple transfers: always-slow handler calls.
        if inst.op.is_multiple_mem_transfer() && !class_disabled('m') {
            return super::emit_transfer::is_multiple_transfer_supported(inst);
        }
        return false;
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

/// A block is compilable when every instruction is supported and the block ENDS in one of
/// the unconditional terminal shapes fill_jit_insts_buf stops on (ARM: AL B or BX; thumb:
/// B or BX reg): control then never falls off the end, and at the exit the interpreter
/// charges the same +2 epilogue and checks the scheduler at the same boundary.
/// Conditional and mid-block branches — including the linking kinds, which resume at the
/// next instruction — are fine.
pub fn is_block_jit_supported(insts: &[InstInfo], thumb: bool) -> bool {
    if insts.is_empty() {
        return false;
    }
    if thumb && class_disabled('t') {
        return false;
    }
    let last = insts.len() - 1;
    let terminal = if thumb {
        matches!(insts[last].op, Op::BT | Op::BxRegT)
    } else {
        matches!(insts[last].op, Op::B | Op::Bx) && insts[last].cond == Cond::AL
    };
    if !terminal {
        return false;
    }
    for (i, inst) in insts.iter().enumerate() {
        if thumb {
            if !thumb::is_inst_supported_thumb(inst) {
                return false;
            }
        } else {
            if !is_inst_supported(inst) {
                return false;
            }
            // Bisect valve: DSVITA_A64_DISABLE=b falls back to the slices-1/2 shape (a
            // single final AL B, nothing else). Thumb is all branches — 't' covers it.
            if matches!(inst.op, Op::B | Op::Bl | Op::Bx | Op::BlxReg) && class_disabled('b') && !(i == last && inst.op == Op::B && inst.cond == Cond::AL) {
                return false;
            }
        }
    }
    true
}

impl JitAsm<'_> {
    /// Emit the whole block (already validated by is_block_jit_supported; the shared
    /// driver ran the analyzer and stamped jit_buf.guest_pc_start).
    pub fn emit(&mut self, block_asm: &mut BlockAsm, thumb: bool) {
        let arm7_hle = self.emu.settings.arm7_emu() == Arm7Emu::Hle;
        debug_assert_eq!(thumb, block_asm.thumb);
        let step_shift = if thumb { 1 } else { 2 };
        let insts_len = self.jit_buf.insts.len();
        let guest_pc = self.jit_buf.guest_pc_start;
        let guest_pc_end = guest_pc + ((insts_len as u32) << step_shift);

        let mut inst_labels: Vec<A64Label> = (0..insts_len).map(|_| A64Label::new()).collect();
        let mut taken_branches: Vec<TakenBranch> = Vec::new();
        let mut sched_blocks: Vec<SchedBlock> = Vec::new();

        // Pre-scan the local branch targets: register mappings live only within
        // straight-line runs, so every arrival point flushes and rebuilds on demand.
        let mut is_local_target = vec![false; insts_len];
        for i in 0..insts_len {
            let inst = &self.jit_buf.insts[i];
            let pc = guest_pc + ((i as u32) << step_shift);
            let kind = if thumb { thumb::thumb_branch_kind(inst, pc, self.cpu) } else { arm_branch_kind(inst, pc) };
            if let Some(BranchKind::B { target }) = kind {
                if target >= guest_pc && target < guest_pc_end {
                    is_local_target[((target - guest_pc) >> step_shift) as usize] = true;
                }
            }
        }

        for i in 0..insts_len {
            // Arrivals (local jumps, mid-entries) land at the label with coherent guest
            // memory and no live mappings; the fall-in path spills its state first.
            if is_local_target[i] {
                block_asm.flush_guest_regs();
            }
            block_asm.masm.bind(&mut inst_labels[i]);
            block_asm.record_inst_offset(self.jit_buf.insts_cycle_counts[i] - self.jit_buf.insts[i].cycle as u16);

            let inst = &self.jit_buf.insts[i];
            let pc = guest_pc + ((i as u32) << step_shift);
            let op = inst.op;
            let opcode = inst.opcode;
            let cond = inst.cond;

            let branch_kind = if thumb { thumb::thumb_branch_kind(inst, pc, self.cpu) } else { arm_branch_kind(inst, pc) };

            if let Some(kind) = branch_kind {
                if cond == Cond::AL {
                    // Always taken: never logged (matching both existing engines). The
                    // branch machinery works memory-direct, so the mappings flush here;
                    // the linking kinds resume at the next instruction with none live.
                    block_asm.flush_guest_regs();
                    self.emit_taken_branch(block_asm, i, &kind, guest_pc, insts_len, arm7_hle, &mut inst_labels, &mut sched_blocks);
                    continue;
                }
                // Taken path out of line; the fall-through is a condition-failed
                // instruction and gets logged like any other (the interpreter logs it).
                // Dirty registers get written back first so the out-of-line path (which
                // works memory-direct) sees current values; the fall-through keeps its
                // mappings.
                block_asm.save_dirty_guest_regs();
                let mut taken = A64Label::new();
                block_asm.load_cpsr(SCRATCH3);
                block_asm.masm.msr_nzcv(SCRATCH3);
                block_asm.masm.b_cond(&mut taken, cond);
                taken_branches.push(TakenBranch { inst_index: i, kind, label: taken });
            } else {
                // Allocate before the condition check so a skipped body still leaves the
                // preloaded old values in the mapped registers (outputs count as inputs
                // for conditional instructions — see alloc_guest_inst). Multiple
                // transfers skip allocation entirely: an rlist can exceed the pool, and
                // their handler works on flushed guest memory.
                if op.is_multiple_mem_transfer() {
                    // The handler works on guest memory: dirty values spill BEFORE the
                    // condition check (a skipped body must not skip the spills), and the
                    // compile-time mappings clear unconditionally — the handler refreshes
                    // memory, so surviving mapped values would be stale on the taken path.
                    block_asm.save_dirty_guest_regs();
                    block_asm.clear_guest_regs_mapping();
                } else {
                    let next_live_regs = self.analyzer.get_next_live_regs(self.analyzer.get_basic_block_from_inst(i), i);
                    block_asm.alloc_guest_inst(inst, next_live_regs);
                }

                // Conditional execution: one b.cond over the body on the inverted guest
                // condition; the debug hook stays outside (the interpreter logs
                // condition-failed instructions too).
                let mut skip = A64Label::new();
                if cond != Cond::AL {
                    block_asm.load_cpsr(SCRATCH3);
                    block_asm.masm.msr_nzcv(SCRATCH3);
                    block_asm.masm.b_cond(&mut skip, !cond);
                }
                if op.is_multiple_mem_transfer() {
                    self.emit_multiple_transfer(block_asm, i, pc);
                } else if op.is_single_mem_transfer() {
                    self.emit_single_transfer(block_asm, i, pc);
                } else if thumb {
                    thumb::emit_thumb_data_processing(block_asm, &self.jit_buf.insts[i], pc);
                } else {
                    emit_data_processing(block_asm, &self.jit_buf.insts[i], pc);
                }
                if cond != Cond::AL {
                    block_asm.masm.bind(&mut skip);
                }
            }

            // Bisect valve: DSVITA_A64_DISABLE=r kills cross-instruction mappings (flush
            // after every instruction — allocator-off shape for isolating lifetime bugs).
            if class_disabled('r') {
                block_asm.flush_guest_regs();
            }

            // Post-state per-instruction log, mirroring the interpreter's DEBUG_LOG hook so
            // mixed jit/interp traces stay comparable record-for-record. Taken branches are
            // not logged (matching both existing engines). The hook reads guest memory, so
            // dirty registers spill first (trace builds are effectively store-through).
            if DEBUG_LOG {
                block_asm.save_dirty_guest_regs();
                block_asm.mov_imm(A64Reg::X0, pc);
                block_asm.mov_imm(A64Reg::X1, opcode);
                block_asm.call_host(match self.cpu {
                    ARM9 => debug_after_exec_op::<{ ARM9 }> as *const (),
                    ARM7 => debug_after_exec_op::<{ ARM7 }> as *const (),
                });
            }
        }

        // Out-of-line taken paths of conditional branches. The linking kinds rejoin the
        // block at the instruction after the branch — their call ran arbitrary guest
        // code, so the fall-through's register mapping gets reloaded first.
        for mut taken in taken_branches {
            block_asm.masm.bind(&mut taken.label);
            self.emit_taken_branch(block_asm, taken.inst_index, &taken.kind, guest_pc, insts_len, arm7_hle, &mut inst_labels, &mut sched_blocks);
            if matches!(taken.kind, BranchKind::Bl { .. } | BranchKind::BlxReg { .. } | BranchKind::BlOff { .. }) {
                let resume_mapping = block_asm.inst_mappings[taken.inst_index + 1];
                block_asm.reload_mapping(&resume_mapping);
                block_asm.masm.b(&mut inst_labels[taken.inst_index + 1]);
            }
        }

        // Out-of-line scheduler tails of local branches.
        for mut sched in sched_blocks {
            self.emit_local_branch_sched_tail(block_asm, &mut sched, guest_pc, arm7_hle);
        }
    }
}

/// What a taken ARM branch does, if `inst` is one.
fn arm_branch_kind(inst: &InstInfo, pc: u32) -> Option<BranchKind> {
    match inst.op {
        Op::B | Op::Bl => {
            let relative = inst.operands()[0].as_imm().unwrap() as i32;
            let target = pc.wrapping_add(8).wrapping_add_signed(relative) & !3;
            Some(if inst.op == Op::B { BranchKind::B { target } } else { BranchKind::Bl { target } })
        }
        Op::Bx => {
            let reg = inst.operands()[0].as_reg_no_shift().unwrap();
            Some(if reg == Reg::LR { BranchKind::BxReturn } else { BranchKind::BxReg { reg } })
        }
        Op::BlxReg => Some(BranchKind::BlxReg {
            reg: inst.operands()[0].as_reg_no_shift().unwrap(),
        }),
        _ => None,
    }
}
