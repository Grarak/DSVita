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
use crate::jit::interpreter::interpret_single;
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
        // Refused-but-lowerable via the single-instruction interpreter helper (mul with
        // N/Z flags, cp15, msr/mrs, swp): the block stays compiled and the helper runs the
        // one op on flushed guest memory, keeping the jit's 128 back-edge threshold. PC-dst
        // variants would return a branch result the helper can't take — refuse those.
        if needs_interpret_single(inst.op) {
            return !inst.out_regs.is_reserved(Reg::PC);
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
    // No terminal-branch requirement (arm32 parity): a region whose fill ran past a backward
    // branch into a self-modified / imm-store range ends on an arbitrary instruction. emit()
    // appends an external chain to the next pc in that case, so the trailing (backward)
    // branch observes the scheduler like the interpreter and the a32-jit — not silently local.
    let last = insts.len() - 1;
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
    /// Emit the whole block basic-block by basic block (arm32 parity). A pre-pass fixes
    /// each block's canonical entry register mapping and, when it is a local-branch target,
    /// its jump label; then every block emits in order and every edge into a block
    /// (fall-through or local branch) relocates the live mapping to that entry mapping.
    /// Conditional branches keep their out-of-line taken tails so the fall-through
    /// (condition-failed) instruction still logs like the interpreter's `Continue`; taken
    /// branches are never logged. Guest PC/CPSR stay memory-resident, so there is no
    /// cross-block cpsr-in-host dance (arm32's only extra step).
    pub fn emit(&mut self, block_asm: &mut BlockAsm, thumb: bool) {
        let arm7_hle = self.emu.settings.arm7_emu() == Arm7Emu::Hle;
        debug_assert_eq!(thumb, block_asm.thumb);
        debug_assert!(
            !self.jit_buf.insts.is_empty(),
            "{:?} compiling empty block at {:x} thumb {thumb}: execution reached undefined code",
            self.cpu,
            self.jit_buf.guest_pc_start
        );

        let insts_len = self.jit_buf.insts.len();
        let guest_pc = self.jit_buf.guest_pc_start;
        let num_blocks = self.analyzer.basic_blocks.len();
        let step_shift = if thumb { 1 } else { 2 };

        let mut inst_labels: Vec<A64Label> = (0..insts_len).map(|_| A64Label::new()).collect();
        let mut taken_branches: Vec<TakenBranch> = Vec::new();
        let mut sched_blocks: Vec<SchedBlock> = Vec::new();

        // Pre-pass: each block's jump label (only when it is a branch target) and its
        // canonical entry mapping. reserve_guest_regs emits no code (restore = false).
        for i in 0..num_blocks {
            let (start_index, inputs, output_regs) = {
                let bb = &self.analyzer.basic_blocks[i];
                (bb.start_index, bb.get_inputs(), bb.output_regs)
            };
            if self.analyzer.insts_metadata[start_index].local_branch_entry() {
                block_asm.guest_basic_block_labels[i] = Some(A64Label::new());
            }
            block_asm.init_guest_regs_mapping(inputs - Reg::PC, output_regs, i);
        }

        for i in 0..num_blocks {
            block_asm.init_guest_regs(i);
            // Block 0's inputs are still only in guest memory; later blocks receive them
            // relocated from their predecessor.
            if i == 0 {
                block_asm.reload_active_guest_regs_all();
            }
            if self.analyzer.insts_metadata[self.analyzer.basic_blocks[i].start_index].local_branch_entry() {
                block_asm.bind_basic_block(i);
            }
            self.emit_basic_block(block_asm, i, thumb, arm7_hle, &mut inst_labels, &mut taken_branches, &mut sched_blocks);
        }

        // arm32 parity (jit_asm.rs's arm branch): when the region doesn't end in an
        // unconditional branch (fill overran a backward branch into a self-modified/imm-store
        // range), the last block falls off its end. Continue at the next pc through an
        // external chain — flushing + observing the scheduler — instead of running into the
        // out-of-line epilogue.
        if !self.jit_buf.insts[insts_len - 1].is_uncond_branch() {
            let next_pc = guest_pc + ((insts_len as u32) << step_shift);
            block_asm.flush_guest_regs();
            self.emit_external_branch(block_asm, insts_len - 1, next_pc, guest_pc, arm7_hle);
        }

        self.emit_epilogue(block_asm, guest_pc, insts_len, arm7_hle, &mut inst_labels, taken_branches, sched_blocks);
    }

    fn emit_basic_block(
        &mut self,
        block_asm: &mut BlockAsm,
        basic_block_index: usize,
        thumb: bool,
        arm7_hle: bool,
        inst_labels: &mut [A64Label],
        taken_branches: &mut Vec<TakenBranch>,
        sched_blocks: &mut Vec<SchedBlock>,
    ) {
        let num_blocks = self.analyzer.basic_blocks.len();
        let (start_index, end_index) = {
            let bb = &self.analyzer.basic_blocks[basic_block_index];
            (bb.start_index, bb.end_index)
        };
        let step_shift = if thumb { 1 } else { 2 };
        let guest_pc = self.jit_buf.guest_pc_start;
        let insts_len = self.jit_buf.insts.len();

        for i in start_index..=end_index {
            // inst_labels are the linking-branch resume targets (BL/BLX rejoin at i+1); a
            // local branch jumps to the target block's label instead.
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
                    // Always taken: never logged (matching both engines). Local branches
                    // relocate to the target block inside emit_taken_branch; the other
                    // kinds work memory-direct, so the mappings flush here.
                    if !matches!(kind, BranchKind::B { target } if self.is_local_target(target, guest_pc, insts_len, thumb)) {
                        block_asm.flush_guest_regs();
                    }
                    self.emit_taken_branch(block_asm, i, &kind, guest_pc, insts_len, arm7_hle, inst_labels, sched_blocks);
                    continue;
                }
                // Taken path out of line; the fall-through is a condition-failed
                // instruction and gets logged like any other. Dirty registers spill first
                // so the out-of-line path sees current values through the restored mapping.
                block_asm.save_dirty_guest_regs();
                let mut taken = A64Label::new();
                block_asm.load_cpsr(SCRATCH3);
                block_asm.masm.msr_nzcv(SCRATCH3);
                block_asm.masm.b_cond(&mut taken, cond);
                taken_branches.push(TakenBranch {
                    inst_index: i,
                    kind,
                    label: taken,
                    dirty_guest_regs: block_asm.dirty_guest_regs,
                    guest_regs_mapping: block_asm.get_guest_regs_mapping(),
                });
            } else {
                // Allocate before the condition check so a skipped body still leaves the
                // preloaded old values in the mapped registers (outputs count as inputs
                // for conditional instructions). Multiple transfers skip allocation: an
                // rlist can exceed the pool, and their handler works on flushed memory.
                // Multi-register transfers work on flushed guest memory (the handler and the
                // unrolled fast path shuttle through scratch), so spill + clear. A single-
                // register multiple is a mapped single transfer, so it allocs like the rest.
                let multi_reg_transfer = op.is_multiple_mem_transfer() && inst.operands()[1].as_reg_list().map_or(false, |r| r.len() > 1);
                if multi_reg_transfer || (!thumb && needs_interpret_single(op)) {
                    block_asm.save_dirty_guest_regs();
                    block_asm.clear_guest_regs_mapping();
                } else {
                    let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, i);
                    block_asm.alloc_guest_inst(inst, next_live_regs);
                }

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
                } else if !thumb && needs_interpret_single(op) {
                    self.emit_interpret_single(block_asm, i, pc);
                } else if thumb {
                    thumb::emit_thumb_data_processing(block_asm, &self.jit_buf.insts[i], pc);
                } else {
                    emit_data_processing(block_asm, &self.jit_buf.insts[i], pc);
                }
                if cond != Cond::AL {
                    block_asm.masm.bind(&mut skip);
                }
            }

            let inst = &self.jit_buf.insts[i];
            if !inst.op.is_branch() {
                block_asm.add_dirty_guest_regs(inst.out_regs - Reg::PC - Reg::CPSR);
            }

            // Bisect valve: DSVITA_A64_DISABLE=r kills cross-instruction mappings.
            if class_disabled('r') {
                block_asm.flush_guest_regs();
            }

            // Post-state per-instruction log, mirroring the interpreter's Continue hook.
            // The hook reads guest memory, so dirty registers spill first.
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

        // Fall-through edge into the next block: relocate the live mapping to its entry
        // mapping (dead code after a block that ended in an unconditional branch — harmless).
        // The final block has no successor, so spill everything instead.
        if basic_block_index == num_blocks - 1 {
            block_asm.save_dirty_guest_regs();
        } else {
            let next_output_regs = self.analyzer.basic_blocks[basic_block_index + 1].output_regs;
            block_asm.relocate_for_basic_block(next_output_regs, basic_block_index + 1);
        }
    }

    /// Is `target` a local branch destination inside this block's guest range?
    fn is_local_target(&self, target: u32, guest_pc: u32, insts_len: usize, thumb: bool) -> bool {
        let step_shift = if thumb { 1 } else { 2 };
        let block_end = guest_pc + ((insts_len as u32) << step_shift);
        target >= guest_pc && target < block_end
    }

    fn emit_epilogue(
        &mut self,
        block_asm: &mut BlockAsm,
        guest_pc: u32,
        insts_len: usize,
        arm7_hle: bool,
        inst_labels: &mut [A64Label],
        taken_branches: Vec<TakenBranch>,
        mut sched_blocks: Vec<SchedBlock>,
    ) {
        // Out-of-line taken paths of conditional branches. Reached via the b.cond at the
        // branch point (registers are live in the pool there), so restore that mapping for
        // the allocator's bookkeeping, then emit the branch. Linking kinds rejoin the
        // fall-through at inst_index+1 after their call returns.
        for mut taken in taken_branches {
            block_asm.masm.bind(&mut taken.label);
            block_asm.set_guest_regs_mapping(taken.guest_regs_mapping);
            block_asm.dirty_guest_regs = taken.dirty_guest_regs;
            self.emit_taken_branch(block_asm, taken.inst_index, &taken.kind, guest_pc, insts_len, arm7_hle, inst_labels, &mut sched_blocks);
            if matches!(taken.kind, BranchKind::Bl { .. } | BranchKind::BlxReg { .. } | BranchKind::BlOff { .. }) {
                let resume_mapping = block_asm.inst_mappings[taken.inst_index + 1];
                block_asm.reload_mapping(&resume_mapping);
                block_asm.masm.b(&mut inst_labels[taken.inst_index + 1]);
            }
        }

        // Out-of-line scheduler tails of local branches.
        for sched in &mut sched_blocks {
            self.emit_local_branch_sched_tail(block_asm, sched, guest_pc, arm7_hle);
        }
    }

    /// Lower a refused single instruction (mul, cp15, msr/mrs, swp) as a call into the
    /// interpreter's single-instruction helper. The caller already flushed dirty registers
    /// and cleared the mapping, so the helper runs on coherent guest memory and the block
    /// stays compiled (128 back-edge threshold). Cycles are charged at the block's branch.
    fn emit_interpret_single(&mut self, block_asm: &mut BlockAsm, inst_index: usize, pc: u32) {
        let opcode = self.jit_buf.insts[inst_index].opcode;
        let total_cycles = self.jit_buf.insts_cycle_counts[inst_index];
        block_asm.mov_imm(A64Reg::X0, opcode);
        block_asm.mov_imm(A64Reg::X1, pc);
        block_asm.mov_imm(A64Reg::X2, total_cycles as u32);
        block_asm.call_host(match self.cpu {
            ARM9 => interpret_single::<{ ARM9 }> as *const (),
            ARM7 => interpret_single::<{ ARM7 }> as *const (),
        });
    }
}

/// Ops the a64 backend doesn't lower inline but can run through the single-instruction
/// interpreter helper (block stays compiled): the multiply family (N/Z flags only), cp15
/// (mcr/mrc), psr moves (msr/mrs), and swap.
fn needs_interpret_single(op: Op) -> bool {
    op.is_mul() || matches!(op, Op::Swp | Op::Swpb | Op::Mcr | Op::Mrc | Op::MsrRc | Op::MsrRs | Op::MsrIc | Op::MsrIs | Op::MrsRc | Op::MrsRs)
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
