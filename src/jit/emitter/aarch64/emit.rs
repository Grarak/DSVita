// The aarch64 block driver: walks the decoded instructions, lowers data-processing
// through emit_alu, branches through emit_branch, and PC-writing ALU/loads through the
// indirect-branch dispatch. Full op coverage — the few shapes without a native lowering
// (see needs_interpret_single) run through the interpreter's single-instruction helper
// inside the compiled block.

use super::emit_alu::emit_data_processing;
use super::emit_branch::{BranchKind, CondIndirectBranch, SchedBlock, TakenBranch};
use super::thumb;
use super::SCRATCH3;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::aarch64::BlockAsm;
use crate::jit::emitter::map_fun_cpu;
use crate::jit::inst_cp15_handler::{cp15_read, cp15_write};
use crate::jit::inst_cpu_regs_handler::cpu_regs_halt;
use crate::jit::inst_exception_handler::software_interrupt_handler;
use crate::jit::inst_info::{InstInfo, Operand, Shift, ShiftValue};
use crate::jit::interpreter::interpret_single;
use crate::jit::jit_asm::{debug_after_exec_op, JitAsm};
use crate::jit::op::Op;
use crate::jit::reg::{reg_reserve, Reg};
use crate::jit::Cond;
use crate::logging::debug_println;
use crate::settings::Arm7Emu;
use crate::DEBUG_LOG;
use vixl::{A64Label, A64Reg};

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
        let mut cond_indirect_branches: Vec<CondIndirectBranch> = Vec::new();

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
            self.emit_basic_block(block_asm, i, thumb, arm7_hle, &mut inst_labels, &mut taken_branches, &mut sched_blocks, &mut cond_indirect_branches);
        }

        // arm32 parity (jit_asm.rs's arm branch): when the region doesn't end in an
        // unconditional branch (fill overran a backward branch into a self-modified/imm-store
        // range), the last block falls off its end. Continue at the next pc through an
        // external chain — flushing + observing the scheduler — instead of running into the
        // out-of-line epilogue.
        if !self.jit_buf.insts[insts_len - 1].is_uncond_branch() {
            let next_pc = guest_pc + ((insts_len as u32) << step_shift);
            block_asm.flush_guest_regs();
            self.emit_external_branch(block_asm, insts_len - 1, next_pc | block_asm.thumb as u32, guest_pc, arm7_hle);
        }

        self.emit_epilogue(block_asm, guest_pc, insts_len, arm7_hle, &mut inst_labels, taken_branches, sched_blocks, cond_indirect_branches);
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
        cond_indirect_branches: &mut Vec<CondIndirectBranch>,
    ) {
        let num_blocks = self.analyzer.basic_blocks.len();
        let (start_index, end_index) = {
            let bb = &self.analyzer.basic_blocks[basic_block_index];
            (bb.start_index, bb.end_index)
        };
        let step_shift = if thumb { 1 } else { 2 };
        let guest_pc = self.jit_buf.guest_pc_start;
        let insts_len = self.jit_buf.insts.len();

        debug_println!("{:?} emit basic block {basic_block_index}", self.cpu);

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

            debug_println!("{pc:x}: block {basic_block_index}: emit {inst:?}");

            let branch_kind = if thumb {
                thumb::thumb_branch_kind(inst, pc, self.cpu)
            } else {
                arm_branch_kind(inst, pc, self.cpu)
            };

            let mut skip_label = None;

            if let Some(kind) = branch_kind {
                if cond == Cond::AL {
                    // Always taken: never logged (matching both engines). Local branches
                    // relocate to the target block inside emit_taken_branch; the other
                    // kinds work memory-direct, so the mappings flush here.
                    if !matches!(kind, BranchKind::B { target } if self.is_local_target(target, guest_pc, insts_len, thumb)) {
                        block_asm.flush_guest_regs();
                    }
                    self.emit_taken_branch(block_asm, i, &kind, guest_pc, insts_len, arm7_hle, inst_labels, sched_blocks);
                } else {
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
                }
            } else {
                // Allocate before the condition check so a skipped body still leaves the
                // preloaded old values in the mapped registers (outputs count as inputs
                // for conditional instructions). Multiple transfers skip allocation: an
                // rlist can exceed the pool, and their handler works on flushed memory.
                // Multi-register transfers work on flushed guest memory (the handler and the
                // unrolled fast path shuttle through scratch), so spill + clear. A single-
                // register multiple is a mapped single transfer, so it allocs like the rest.
                let multi_reg_transfer = op.is_multiple_mem_transfer() && inst.operands()[1].as_reg_list().map_or(false, |r| r.len() > 1);
                let interp_single = needs_interpret_single(inst, thumb);
                if multi_reg_transfer || interp_single {
                    block_asm.save_dirty_guest_regs();
                    block_asm.clear_guest_regs_mapping();
                } else {
                    let next_live_regs = self.analyzer.get_next_live_regs(basic_block_index, i);
                    block_asm.alloc_guest_inst(inst, next_live_regs);
                }

                if cond != Cond::AL {
                    skip_label = Some(A64Label::new());
                    block_asm.load_cpsr(SCRATCH3);
                    block_asm.masm.msr_nzcv(SCRATCH3);
                    block_asm.masm.b_cond(skip_label.as_mut().unwrap(), !cond);
                }
                if matches!(op, Op::Swi | Op::SwiT) {
                    self.emit_swi(block_asm, i, pc);
                } else if matches!(op, Op::Mcr | Op::Mrc) {
                    self.emit_cp15(block_asm, i, pc);
                } else if op == Op::Blx {
                    // Only reached on the ARM7 (arm_branch_kind gates the branch kind to
                    // ARM9): v4 has no BLX — emit nothing, arm32 parity.
                } else if interp_single {
                    self.emit_interpret_single(block_asm, i, pc);
                } else if op.is_multiple_mem_transfer() {
                    self.emit_multiple_transfer(block_asm, i, pc);
                } else if op.is_single_mem_transfer() {
                    self.emit_single_transfer(block_asm, i, pc);
                } else if thumb {
                    thumb::emit_thumb_data_processing(block_asm, &self.jit_buf.insts[i], pc);
                } else {
                    emit_data_processing(block_asm, &self.jit_buf.insts[i], pc);
                }
            }

            let inst = &self.jit_buf.insts[i];
            if !inst.op.is_branch() {
                block_asm.add_dirty_guest_regs(inst.out_regs - Reg::PC - Reg::CPSR);
            }

            if (inst.op.is_alu() || inst.op.is_single_mem_transfer() || inst.op.is_multiple_mem_transfer()) && inst.out_regs.is_reserved(Reg::PC) {
                // ALU/load into the pc = indirect branch (arm32's handle_indirect_branch
                // seam). The emitter bodies left the raw target in the guest PC slot.
                // Conditional forms jump to an out-of-line dispatch so the fall-through
                // stays a logged condition-failed instruction; thumb and AL forms
                // dispatch inline and never return (the debug hook below becomes dead
                // code, so taken indirect branches are never logged — engine parity).
                if thumb {
                    self.emit_indirect_branch_thumb(block_asm, i, guest_pc, arm7_hle);
                } else if cond != Cond::AL {
                    let mut label = A64Label::new();
                    block_asm.masm.b(&mut label);
                    cond_indirect_branches.push(CondIndirectBranch {
                        inst_index: i,
                        label,
                        dirty_guest_regs: block_asm.dirty_guest_regs,
                        guest_regs_mapping: block_asm.get_guest_regs_mapping(),
                    });
                } else {
                    self.emit_indirect_branch(block_asm, i, guest_pc, arm7_hle);
                }
            }

            if let Some(skip_label) = skip_label.as_mut() {
                block_asm.masm.bind(skip_label);
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
        cond_indirect_branches: Vec<CondIndirectBranch>,
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
            if matches!(taken.kind, BranchKind::Bl { .. } | BranchKind::BlxReg { .. } | BranchKind::BlxImm { .. } | BranchKind::BlOff { .. }) {
                if taken.inst_index + 1 < insts_len {
                    let resume_mapping = block_asm.inst_mappings[taken.inst_index + 1];
                    block_asm.reload_mapping(&resume_mapping);
                    block_asm.masm.b(&mut inst_labels[taken.inst_index + 1]);
                } else {
                    // A conditional linking branch as the block's LAST instruction: only
                    // data decoded as code produces this (real blocks end at unconditional
                    // flow changes), so there is no fall-through instruction to rejoin —
                    // exit the block at the resume pc instead of indexing past the block.
                    let step_shift = if block_asm.thumb { 1 } else { 2 };
                    let resume_pc = guest_pc + ((taken.inst_index as u32 + 1) << step_shift);
                    block_asm.mov_imm(SCRATCH3, resume_pc);
                    block_asm.store_guest(SCRATCH3, Reg::PC);
                    self.emit_branch_out_exit(block_asm, taken.inst_index, resume_pc);
                }
            }
        }

        // Out-of-line scheduler tails of local branches.
        for sched in &mut sched_blocks {
            self.emit_local_branch_sched_tail(block_asm, sched, guest_pc, arm7_hle);
        }

        // Out-of-line dispatch of conditional pc-writing ALU/load instructions. Reached
        // via the jump behind the (condition-guarded) body, so the branch-point mapping
        // holds live values — restore the bookkeeping before the dispatch flushes them.
        for mut indirect in cond_indirect_branches {
            block_asm.masm.bind(&mut indirect.label);
            block_asm.set_guest_regs_mapping(indirect.guest_regs_mapping);
            block_asm.dirty_guest_regs = indirect.dirty_guest_regs;
            self.emit_indirect_branch(block_asm, indirect.inst_index, guest_pc, arm7_hle);
        }
    }

    /// swi: HLE bios dispatch through the shared exception handler (arm32's emit_swi).
    /// The handler runs on coherent guest memory and mutates r0/r1/r3 (bios call
    /// results) — spill first, refresh those registers' mapped homes after. A halting
    /// swi breaks out inside the handler (breakout_imm) and never returns here.
    fn emit_swi(&mut self, block_asm: &mut BlockAsm, inst_index: usize, pc: u32) {
        let opcode = self.jit_buf.insts[inst_index].opcode;
        let comment = if block_asm.thumb { opcode } else { opcode >> 16 } as u8;
        block_asm.save_dirty_guest_regs();
        block_asm.mov_imm(A64Reg::X0, comment as u32);
        block_asm.mov_imm(A64Reg::X1, pc | block_asm.thumb as u32);
        block_asm.mov_imm(A64Reg::X2, self.jit_buf.insts_cycle_counts[inst_index] as u32);
        block_asm.call_host(map_fun_cpu!(self.cpu, software_interrupt_handler));
        block_asm.reg_alloc.reload_active_guest_regs(reg_reserve!(Reg::R0, Reg::R1, Reg::R3), &mut block_asm.masm);
    }

    /// cp15 mcr/mrc (ARM9 only — the ARM7 has no coprocessor, emit nothing like arm32).
    /// The two wait-for-interrupt registers park the resume pc, halt and leave the guest
    /// context with the branch cycles charged — arm32's emit_cp15 halt shape. Routing
    /// these through the interpreter helper instead runs the scheduler inside the block
    /// (breakout_imm), which splits the cm feed differently from the arm32 backend and
    /// phase-shifts irq delivery (GTA:CTW boot death). Everything else is a direct
    /// handler call on the mapped operand.
    fn emit_cp15(&mut self, block_asm: &mut BlockAsm, inst_index: usize, pc: u32) {
        if self.cpu != ARM9 {
            return;
        }
        let inst = &self.jit_buf.insts[inst_index];
        let op = inst.op;
        let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
        let opcode = inst.opcode;
        let cn = (opcode >> 16) & 0xF;
        let cm = opcode & 0xF;
        let cp = (opcode >> 5) & 0x7;
        let cp15_reg = (cn << 16) | (cm << 8) | cp;

        if cp15_reg == 0x070004 || cp15_reg == 0x070802 {
            block_asm.save_dirty_guest_regs();
            block_asm.mov_imm(SCRATCH3, pc + 4);
            block_asm.store_guest(SCRATCH3, Reg::PC);
            block_asm.call_host(cpu_regs_halt as *const ());
            self.emit_branch_out_exit(block_asm, inst_index, pc);
        } else {
            // A pc operand only occurs in data decoded as a conditional mcr/mrc (pc isn't
            // allocator-managed here — guest_map would assert). Mcr reads the pipeline
            // value; mrc lands in the guest PC slot via scratch, like a pc-destination load.
            match op {
                Op::Mcr => {
                    block_asm.mov_imm(A64Reg::X0, cp15_reg);
                    if op0 == Reg::PC {
                        block_asm.mov_imm(A64Reg::X1, pc + 8);
                    } else {
                        let op0_mapped = block_asm.guest_map(op0);
                        block_asm.masm.mov_reg(A64Reg::X1, op0_mapped, false);
                    }
                    block_asm.call_host(cp15_write as *const ());
                }
                Op::Mrc => {
                    block_asm.mov_imm(A64Reg::X0, cp15_reg);
                    block_asm.call_host(cp15_read as *const ());
                    if op0 == Reg::PC {
                        block_asm.masm.mov_reg(SCRATCH3, A64Reg::X0, false);
                        block_asm.store_guest(SCRATCH3, Reg::PC);
                    } else {
                        let op0_mapped = block_asm.guest_map(op0);
                        block_asm.masm.mov_reg(op0_mapped, A64Reg::X0, false);
                    }
                }
                _ => unsafe { std::hint::unreachable_unchecked() },
            }
        }
    }

    /// Lower a refused single instruction (mul, msr/mrs, swp, clz/q, odd shifter shapes)
    /// as a call into the interpreter's single-instruction helper. The caller already
    /// flushed dirty registers and cleared the mapping, so the helper runs on coherent
    /// guest memory and the block stays compiled (128 back-edge threshold). Cycles are
    /// charged at the block's branch.
    fn emit_interpret_single(&mut self, block_asm: &mut BlockAsm, inst_index: usize, pc: u32) {
        let opcode = self.jit_buf.insts[inst_index].opcode;
        let total_cycles = self.jit_buf.insts_cycle_counts[inst_index];
        block_asm.mov_imm(A64Reg::X0, opcode);
        block_asm.mov_imm(A64Reg::X1, pc | block_asm.thumb as u32);
        block_asm.mov_imm(A64Reg::X2, total_cycles as u32);
        block_asm.call_host(match self.cpu {
            ARM9 => interpret_single::<{ ARM9 }> as *const (),
            ARM7 => interpret_single::<{ ARM7 }> as *const (),
        });
    }
}

/// Ops the a64 backend doesn't lower inline but can run through the single-instruction
/// interpreter helper (block stays compiled): the ARM multiply family (N/Z flags only),
/// cp15 (mcr/mrc), psr moves (msr/mrs), swap, clz and the saturating q ops, ldrd/strd
/// (the pair rotation dance), and the shifter shapes the alu/transfer emitters don't
/// lower — register amounts (dynamic carry; also their thumb forms) and RRX. A pc
/// destination combined with such a shifter (UNPREDICTABLE on hardware; reached only by
/// data decoded as always-false conditional code) goes through the interpreter too —
/// the post-inst indirect dispatch picks up whatever pc it writes.
fn needs_interpret_single(inst: &InstInfo, thumb: bool) -> bool {
    let op = inst.op;
    if thumb {
        return matches!(op, Op::RorT) || (matches!(op, Op::LslT | Op::LsrT | Op::AsrT) && inst.operands()[2].as_imm().is_none());
    }
    // Register-amount or RRX (ror #0 encoding) shift on the last operand (alu operand-2
    // or the transfer offset).
    let unlowered_shift = matches!(
        inst.operands().last(),
        Some(Operand::Reg { shift: Some(shift), .. })
            if matches!(
                shift,
                Shift::Lsl(ShiftValue::Reg(_)) | Shift::Lsr(ShiftValue::Reg(_)) | Shift::Asr(ShiftValue::Reg(_)) | Shift::Ror(ShiftValue::Reg(_)) | Shift::Ror(ShiftValue::Imm(0))
            )
    );
    if op.is_single_mem_transfer() {
        let size3 = match op {
            Op::Ldr(transfer) | Op::Str(transfer) => transfer.size() == 3,
            _ => false,
        };
        return size3 || unlowered_shift;
    }
    // User-bank ldm/stm (`^` without pc in the list): the unrolled fast path works on the
    // current-bank ThreadRegs view, so the banked redirection must come from the
    // interpreter. `^` WITH pc is an spsr-restore return on current-bank registers — the
    // fast path plus the indirect-branch dispatch handle that one.
    if let Op::Ldm(transfer) | Op::Stm(transfer) = op {
        if transfer.user() && !inst.operands()[1].as_reg_list().unwrap().is_reserved(Reg::PC) {
            return true;
        }
    }
    op.is_mul()
        || matches!(
            op,
            Op::Swp | Op::Swpb | Op::MsrRc | Op::MsrRs | Op::MsrIc | Op::MsrIs | Op::MrsRc | Op::MrsRs | Op::Clz | Op::Qadd | Op::Qsub | Op::Qdadd | Op::Qdsub
        )
        || (op.is_alu() && unlowered_shift)
}

/// What a taken ARM branch does, if `inst` is one.
fn arm_branch_kind(inst: &InstInfo, pc: u32, cpu: crate::core::CpuType) -> Option<BranchKind> {
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
        // BLX imm switches to thumb: the target carries the mode tag (the decoder folded
        // the H halfword bit into the offset). ARM9 only — the ARM7 emits nothing (v4).
        Op::Blx if cpu == ARM9 => {
            let relative = inst.operands()[0].as_imm().unwrap() as i32;
            let target = pc.wrapping_add(8).wrapping_add_signed(relative) | 1;
            Some(BranchKind::BlxImm { target })
        }
        _ => None,
    }
}
