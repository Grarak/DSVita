// The aarch64 branch lowering, stage-5 slice 3 (mirroring the arm32 backend):
// - A B whose target lies inside the block jumps between per-instruction labels. The taken
//   path charges the branch epilogue (+2) against the runtime cycle accounting and checks
//   the scheduler threshold — the out-of-line exceed path runs the scheduler and, on ARM9,
//   dispatches a pending interrupt before rejoining (ARM7 exits the guest context, its
//   quantum is up). Forward local branches re-validate the block's own jit entry after the
//   possible scheduler excursion, like arm32's forward-branch check.
// - A B leaving the block stores the guest PC, flushes through the shared pre_branch
//   runtime (cycles, scheduler, pre_cycle_count_sum reset), then pops the block's frame and
//   TAIL-JUMPS to the target's jit entry slot — compiled block-to-block transfers keep the
//   host stack flat; an uncompiled target lands in emit_code_block as the tail callee.
// - Scheduler-threshold parity: the arm32 backend checks local back-edges against
//   max_branch_loop_cycle_count (128) while the interpreter checks every taken branch
//   against max_loop_cycle_count (255 on ARM9) — a real cross-engine timing difference.
//   Production default mirrors arm32 (the stage-6 strict pair is armhf-jit vs a64-jit);
//   DSVITA_A64_INTERP_TIMING=1 switches to the interpreter's threshold so the
//   a64-jit-vs-a64-interp strict tracediff stays a usable regression gate.

use super::emit::class_disabled;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::aarch64::{BlockAsm, SCRATCH0, SCRATCH1, SCRATCH2};
use crate::jit::inst_branch_handler::{branch_lr, branch_reg, handle_interrupt, pre_branch, run_scheduler};
use crate::jit::jit_asm::{JitAsm, JitRuntimeData};
use crate::jit::reg::Reg;
use crate::jit::Cond;
use crate::logging::branch_println;
use crate::{BRANCH_LOG, IS_DEBUG};
use std::ptr;
use vixl::{A64Label, A64Reg, A64ShiftKind};

extern "C" fn debug_branch_label<const CPU: CpuType>(current_pc: u32, target_pc: u32) {
    branch_println!("{CPU:?} branch label from {current_pc:x} to {target_pc:x}")
}

extern "C" fn debug_branch_imm<const CPU: CpuType>(current_pc: u32, target_pc: u32) {
    branch_println!("{CPU:?} branch imm from {current_pc:x} to {target_pc:x}");
}

/// Scheduler threshold for local branches: arm32-jit parity by default (the stage-6
/// reference), interpreter parity under DSVITA_A64_INTERP_TIMING=1 (the strict
/// jit-vs-interp gate). The two differ only on ARM9 (128 vs 255).
pub(super) fn local_branch_sched_threshold(cpu: CpuType) -> u32 {
    static INTERP_TIMING: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if crate::IS_DEBUG && *INTERP_TIMING.get_or_init(|| std::env::var("DSVITA_A64_INTERP_TIMING").map(|v| v == "1").unwrap_or(false)) {
        cpu.max_loop_cycle_count()
    } else {
        cpu.max_branch_loop_cycle_count()
    }
}

/// What a taken branch instruction does — the out-of-line taken paths and the inline AL
/// emissions share these shapes.
pub(super) enum BranchKind {
    /// B: local jump or external tail call. Never falls through.
    B { target: u32 },
    /// BL: linking call through pre_branch + the target's entry; execution resumes at the
    /// following instruction.
    Bl { target: u32 },
    /// BX LR: return through the return stack (branch_lr tail call).
    BxReturn,
    /// BX other-reg: dynamic-target tail call through branch_reg.
    BxReg { reg: Reg },
    /// BLX reg: linking dynamic call through branch_reg; resumes at the next instruction.
    BlxReg { reg: Reg },
    /// Thumb BL/BLX second half: target = LR + (off11 << 1), computed at run time (the
    /// BlSetupT half staged LR). `to_arm` = BLX (mode switch, target word-aligned).
    BlOff { off: u32, to_arm: bool },
}

/// A conditional branch's taken path, emitted out of line after the block body.
pub(super) struct TakenBranch {
    pub(super) inst_index: usize,
    pub(super) kind: BranchKind,
    pub(super) label: A64Label,
}

/// The out-of-line tail of a local branch: the scheduler-exceed path and, for forward
/// branches, the stale-block exit. `cont_label` was bound on the branch's fast path.
pub(super) struct SchedBlock {
    inst_index: usize,
    target: u32,
    sched_label: A64Label,
    cont_label: A64Label,
    invalid_label: Option<A64Label>,
}

impl JitAsm<'_> {
    /// Emit a taken branch of any kind. B never returns to the block; the linking kinds
    /// (BL, BLX reg) resume at the following instruction — inline that's the natural fall
    /// through, out of line the caller jumps back via `inst_labels[inst_index + 1]`.
    pub(super) fn emit_taken_branch(
        &mut self,
        block_asm: &mut BlockAsm,
        inst_index: usize,
        kind: &BranchKind,
        guest_pc: u32,
        insts_len: usize,
        arm7_hle: bool,
        inst_labels: &mut [A64Label],
        sched_blocks: &mut Vec<SchedBlock>,
    ) {
        match kind {
            BranchKind::B { target } => self.emit_branch(block_asm, inst_index, *target, guest_pc, insts_len, arm7_hle, inst_labels, sched_blocks),
            BranchKind::Bl { target } => self.emit_branch_link(block_asm, inst_index, *target, guest_pc, arm7_hle),
            BranchKind::BxReturn => self.emit_branch_return(block_asm, inst_index, guest_pc, arm7_hle),
            BranchKind::BxReg { reg } => self.emit_branch_reg(block_asm, inst_index, *reg, false, guest_pc, arm7_hle),
            BranchKind::BlxReg { reg } => self.emit_branch_reg(block_asm, inst_index, *reg, true, guest_pc, arm7_hle),
            BranchKind::BlOff { off, to_arm } => self.emit_branch_link_off(block_asm, inst_index, *off, *to_arm, guest_pc, arm7_hle),
        }
    }

    /// A taken B at `inst_index`: local targets jump inside the block, external targets
    /// tail-call out through the shared runtime.
    pub(super) fn emit_branch(
        &mut self,
        block_asm: &mut BlockAsm,
        inst_index: usize,
        target: u32,
        guest_pc: u32,
        insts_len: usize,
        arm7_hle: bool,
        inst_labels: &mut [A64Label],
        sched_blocks: &mut Vec<SchedBlock>,
    ) {
        let step_shift = if block_asm.thumb { 1 } else { 2 };
        let block_end = guest_pc + ((insts_len as u32) << step_shift);
        if target >= guest_pc && target < block_end {
            let target_index = ((target - guest_pc) >> step_shift) as usize;
            self.emit_local_branch(block_asm, inst_index, target_index, guest_pc, inst_labels, sched_blocks);
        } else {
            self.emit_external_branch(block_asm, inst_index, target, guest_pc, arm7_hle);
        }
    }

    /// BL / BLX imm: write the guest LR and PC, flush through pre_branch (which pushes the
    /// return stack and checks the ARM9 host-stack depth guard), then CALL the target's
    /// entry slot — the host frame is unwound by the callee's eventual matched return
    /// through branch_lr's `ret`, exactly like arm32's blx call sites. On return the
    /// block's instructions so far are pre-charged (pre_cycle_count_sum = counts[i]).
    pub(super) fn emit_branch_link(&mut self, block_asm: &mut BlockAsm, inst_index: usize, target: u32, guest_pc: u32, arm7_hle: bool) {
        debug_assert!(!block_asm.thumb);
        let current_pc = guest_pc + (inst_index as u32) * 4;
        let total_cycles = self.jit_buf.insts_cycle_counts[inst_index];
        let lr = current_pc + 4;

        block_asm.mov_imm(SCRATCH0, lr);
        block_asm.store_guest(SCRATCH0, Reg::LR);
        block_asm.mov_imm(SCRATCH0, target);
        block_asm.store_guest(SCRATCH0, Reg::PC);

        block_asm.masm.mov_imm64(A64Reg::X0, self as *mut JitAsm as u64);
        block_asm.mov_imm(A64Reg::X1, total_cycles as u32);
        block_asm.mov_imm(A64Reg::X2, lr);
        block_asm.mov_imm(A64Reg::X3, current_pc);
        block_asm.call_host(match (self.cpu, arm7_hle) {
            (ARM9, true) => pre_branch::<{ ARM9 }, true, true> as *const (),
            (ARM9, false) => pre_branch::<{ ARM9 }, true, false> as *const (),
            (ARM7, true) => pre_branch::<{ ARM7 }, true, true> as *const (),
            (ARM7, false) => pre_branch::<{ ARM7 }, true, false> as *const (),
        });

        if BRANCH_LOG {
            block_asm.mov_imm(A64Reg::X0, current_pc);
            block_asm.mov_imm(A64Reg::X1, target);
            block_asm.call_host(match self.cpu {
                ARM9 => debug_branch_imm::<{ ARM9 }> as *const (),
                ARM7 => debug_branch_imm::<{ ARM7 }> as *const (),
            });
        }

        // Call the entry slot (re-read at run time; a cold target lands in
        // emit_code_block). An ARM-mode BL never changes mode — no thumb tag handling.
        let target_entry_slot = self.emu.jit.jit_memory_map.get_jit_entry(target);
        block_asm.masm.mov_imm64(SCRATCH0, target_entry_slot as u64);
        block_asm.masm.ldr_off(SCRATCH0, true, SCRATCH0, 0, vixl::A64AddrModeKind::Offset);
        block_asm.mov_imm(A64Reg::X0, target);
        block_asm.masm.blr(SCRATCH0);

        // Resume behind the call: everything up to and including the BL is pre-charged.
        block_asm.masm.mov_imm64(A64Reg::X8, ptr::addr_of_mut!(self.runtime_data) as u64);
        block_asm.mov_imm(SCRATCH0, total_cycles as u32);
        block_asm
            .masm
            .strh_off(SCRATCH0, A64Reg::X8, JitRuntimeData::get_pre_cycle_count_sum_offset() as i64, vixl::A64AddrModeKind::Offset);
    }

    /// BX LR: return through the return stack — store the guest PC, pop the frame and
    /// tail-jump into branch_lr, whose `ret` on a matched pop unwinds straight to the
    /// caller's post-BL code (the guest call/return pairs map onto host call/returns).
    pub(super) fn emit_branch_return(&mut self, block_asm: &mut BlockAsm, inst_index: usize, guest_pc: u32, arm7_hle: bool) {
        let current_pc = guest_pc + ((inst_index as u32) << if block_asm.thumb { 1 } else { 2 });
        let total_cycles = self.jit_buf.insts_cycle_counts[inst_index];

        block_asm.load_guest(A64Reg::X1, Reg::LR);
        block_asm.store_guest(A64Reg::X1, Reg::PC);
        block_asm.mov_imm(A64Reg::X0, total_cycles as u32);
        block_asm.mov_imm(A64Reg::X2, current_pc);
        block_asm.masm.mov_imm64(
            A64Reg::X8,
            match (self.cpu, arm7_hle) {
                (ARM9, true) => branch_lr::<{ ARM9 }, true> as *const () as u64,
                (ARM9, false) => branch_lr::<{ ARM9 }, false> as *const () as u64,
                (ARM7, true) => branch_lr::<{ ARM7 }, true> as *const () as u64,
                (ARM7, false) => branch_lr::<{ ARM7 }, false> as *const () as u64,
            },
        );
        block_asm.restore_frame();
        block_asm.masm.br(A64Reg::X8);
    }

    /// BX reg (dynamic target, no link): tail-jump into branch_reg, which handles the
    /// thumb interworking bit and dispatches. BLX reg (link): CALL branch_reg instead —
    /// it pushes the return stack, runs the callee and restores pre_cycle_count_sum
    /// itself before returning here.
    pub(super) fn emit_branch_reg(&mut self, block_asm: &mut BlockAsm, inst_index: usize, target_reg: Reg, has_return: bool, guest_pc: u32, arm7_hle: bool) {
        let thumb = block_asm.thumb;
        let current_pc = guest_pc + ((inst_index as u32) << if thumb { 1 } else { 2 });
        let total_cycles = self.jit_buf.insts_cycle_counts[inst_index];

        if target_reg == Reg::PC {
            // Pipeline pc read: instruction address + 8 in ARM, + 4 in thumb.
            block_asm.mov_imm(A64Reg::X1, current_pc + if thumb { 4 } else { 8 });
        } else {
            block_asm.load_guest(A64Reg::X1, target_reg);
        }
        block_asm.store_guest(A64Reg::X1, Reg::PC);
        if has_return {
            // The return address is the following instruction, mode-tagged in thumb.
            let lr = if thumb { (current_pc + 2) | 1 } else { current_pc + 4 };
            block_asm.mov_imm(SCRATCH0, lr);
            block_asm.store_guest(SCRATCH0, Reg::LR);
            block_asm.mov_imm(A64Reg::X2, lr);
        } else {
            block_asm.mov_imm(A64Reg::X2, 0);
        }
        block_asm.mov_imm(A64Reg::X0, total_cycles as u32);
        block_asm.mov_imm(A64Reg::X3, current_pc);

        let fun = match (self.cpu, has_return, arm7_hle) {
            (ARM9, true, true) => branch_reg::<{ ARM9 }, true, true> as *const (),
            (ARM9, true, false) => branch_reg::<{ ARM9 }, true, false> as *const (),
            (ARM9, false, true) => branch_reg::<{ ARM9 }, false, true> as *const (),
            (ARM9, false, false) => branch_reg::<{ ARM9 }, false, false> as *const (),
            (ARM7, true, true) => branch_reg::<{ ARM7 }, true, true> as *const (),
            (ARM7, true, false) => branch_reg::<{ ARM7 }, true, false> as *const (),
            (ARM7, false, true) => branch_reg::<{ ARM7 }, false, true> as *const (),
            (ARM7, false, false) => branch_reg::<{ ARM7 }, false, false> as *const (),
        };
        if has_return {
            block_asm.call_host(fun);
        } else {
            block_asm.masm.mov_imm64(A64Reg::X8, fun as u64);
            block_asm.restore_frame();
            block_asm.masm.br(A64Reg::X8);
        }
    }

    /// Local branch fast path: charge the boundary cycles (+2 branch epilogue, minus the
    /// already-accounted pre_cycle_count_sum), check the scheduler threshold, set the
    /// target's pre_cycle_count_sum and jump to its label. The exceed/invalid tails are
    /// deferred to emit_local_branch_sched_tail.
    fn emit_local_branch(&mut self, block_asm: &mut BlockAsm, inst_index: usize, target_index: usize, guest_pc: u32, inst_labels: &mut [A64Label], sched_blocks: &mut Vec<SchedBlock>) {
        let step_shift = if block_asm.thumb { 1 } else { 2 };
        let current_pc = guest_pc + ((inst_index as u32) << step_shift);
        let target_pc = guest_pc + ((target_index as u32) << step_shift);
        let total_cycles = self.jit_buf.insts_cycle_counts[inst_index];
        let target_pre_cycle_count_sum = self.jit_buf.insts_cycle_counts[target_index] - self.jit_buf.insts[target_index].cycle as u16;
        let runtime_data_addr = ptr::addr_of_mut!(self.runtime_data) as u64;

        let mut sched_label = A64Label::new();
        let mut cont_label = A64Label::new();

        block_asm.masm.mov_imm64(A64Reg::X8, runtime_data_addr);
        block_asm
            .masm
            .ldrh_off(SCRATCH0, A64Reg::X8, JitRuntimeData::get_accumulated_cycles_offset() as i64, vixl::A64AddrModeKind::Offset);
        block_asm
            .masm
            .ldrh_off(SCRATCH1, A64Reg::X8, JitRuntimeData::get_pre_cycle_count_sum_offset() as i64, vixl::A64AddrModeKind::Offset);
        // +2 for branching, like the interpreter's taken-branch charge and arm32's
        // emit_count_cycles.
        block_asm.mov_imm(SCRATCH2, total_cycles as u32 + 2);
        block_asm.masm.add_reg(SCRATCH0, SCRATCH0, SCRATCH2, A64ShiftKind::LSL, 0, false);
        block_asm.masm.sub_reg(SCRATCH0, SCRATCH0, SCRATCH1, A64ShiftKind::LSL, 0, false);
        block_asm
            .masm
            .strh_off(SCRATCH0, A64Reg::X8, JitRuntimeData::get_accumulated_cycles_offset() as i64, vixl::A64AddrModeKind::Offset);
        block_asm.masm.cmp_imm(SCRATCH0, local_branch_sched_threshold(self.cpu) as u64, false);
        block_asm.masm.b_cond(&mut sched_label, Cond::HS);
        block_asm.masm.bind(&mut cont_label);

        // Forward branches re-validate the block's jit entry (arm32 parity: the scheduler
        // excursion may have run the other cpu, which can invalidate this block).
        let invalid_label = if target_index > inst_index && !class_disabled('v') {
            let mut invalid = A64Label::new();
            let target_entry_slot = self.emu.jit.jit_memory_map.get_jit_entry(target_pc);
            block_asm.masm.mov_imm64(SCRATCH0, target_entry_slot as u64);
            block_asm.masm.ldr_off(SCRATCH0, true, SCRATCH0, 0, vixl::A64AddrModeKind::Offset);
            block_asm.adr_start(SCRATCH1);
            block_asm.masm.cmp_reg(SCRATCH0, SCRATCH1, A64ShiftKind::LSL, 0, true);
            block_asm.masm.b_cond(&mut invalid, Cond::NE);
            Some(invalid)
        } else {
            None
        };

        if BRANCH_LOG {
            block_asm.mov_imm(A64Reg::X0, current_pc);
            block_asm.mov_imm(A64Reg::X1, target_pc);
            block_asm.call_host(match self.cpu {
                ARM9 => debug_branch_label::<{ ARM9 }> as *const (),
                ARM7 => debug_branch_label::<{ ARM7 }> as *const (),
            });
            block_asm.masm.mov_imm64(A64Reg::X8, runtime_data_addr);
        }

        block_asm.mov_imm(SCRATCH0, target_pre_cycle_count_sum as u32);
        block_asm
            .masm
            .strh_off(SCRATCH0, A64Reg::X8, JitRuntimeData::get_pre_cycle_count_sum_offset() as i64, vixl::A64AddrModeKind::Offset);
        block_asm.masm.b(&mut inst_labels[target_index]);

        sched_blocks.push(SchedBlock {
            inst_index,
            target: target_pc,
            sched_label,
            cont_label,
            invalid_label,
        });
    }

    /// The out-of-line tails of a local branch: scheduler-exceed (run the scheduler, on
    /// ARM9 dispatch a pending interrupt, rejoin the fast path) and, for forward branches,
    /// the stale-block exit.
    pub(super) fn emit_local_branch_sched_tail(&mut self, block_asm: &mut BlockAsm, sched: &mut SchedBlock, guest_pc: u32, arm7_hle: bool) {
        let current_pc = guest_pc + ((sched.inst_index as u32) << if block_asm.thumb { 1 } else { 2 });
        let jit_asm_addr = self as *mut JitAsm as u64;
        let runtime_data_addr = ptr::addr_of_mut!(self.runtime_data) as u64;

        block_asm.masm.bind(&mut sched.sched_label);
        // The interrupt/exit resume point: like the interpreter, the guest PC is the
        // branch target before the scheduler can observe it.
        block_asm.mov_imm(SCRATCH0, sched.target);
        block_asm.store_guest(SCRATCH0, Reg::PC);
        match self.cpu {
            ARM9 => {
                block_asm.masm.mov_imm64(A64Reg::X0, jit_asm_addr);
                block_asm.mov_imm(A64Reg::X1, current_pc);
                block_asm.call_host(if arm7_hle { run_scheduler::<true> as *const () } else { run_scheduler::<false> as *const () });
                // An interrupt moved the guest PC: dispatch it and return here after.
                block_asm.load_guest(SCRATCH0, Reg::PC);
                block_asm.mov_imm(SCRATCH1, sched.target);
                block_asm.masm.cmp_reg(SCRATCH0, SCRATCH1, A64ShiftKind::LSL, 0, false);
                // The fast path's continue point stores through x8 — the calls clobbered
                // it, reload before rejoining (arm32 reloads r0 the same way).
                block_asm.masm.mov_imm64(A64Reg::X8, runtime_data_addr);
                block_asm.masm.b_cond(&mut sched.cont_label, Cond::EQ);
                block_asm.masm.mov_imm64(A64Reg::X0, jit_asm_addr);
                block_asm.mov_imm(A64Reg::X1, sched.target);
                block_asm.mov_imm(A64Reg::X2, current_pc);
                block_asm.call_host(handle_interrupt as *const ());
                block_asm.masm.mov_imm64(A64Reg::X8, runtime_data_addr);
                block_asm.masm.b(&mut sched.cont_label);
            }
            ARM7 => {
                // The quantum is up: exit the guest context (the interpreter's
                // exe_scheduler does the same for ARM7).
                if IS_DEBUG {
                    block_asm.masm.mov_imm64(A64Reg::X8, runtime_data_addr);
                    block_asm.mov_imm(SCRATCH0, current_pc);
                    block_asm
                        .masm
                        .str_off(SCRATCH0, false, A64Reg::X8, JitRuntimeData::get_branch_out_pc_offset() as i64, vixl::A64AddrModeKind::Offset);
                }
                block_asm.emit_exit_guest_context(ptr::addr_of_mut!(self.runtime_data.host_sp));
            }
        }

        if let Some(invalid_label) = &mut sched.invalid_label {
            // The block was invalidated under our feet: exit the guest context; execution
            // resumes at the stored target PC with fresh code.
            block_asm.masm.bind(invalid_label);
            block_asm.mov_imm(SCRATCH0, sched.target);
            block_asm.store_guest(SCRATCH0, Reg::PC);
            if IS_DEBUG {
                block_asm.masm.mov_imm64(A64Reg::X8, runtime_data_addr);
                block_asm.mov_imm(SCRATCH0, current_pc);
                block_asm
                    .masm
                    .str_off(SCRATCH0, false, A64Reg::X8, JitRuntimeData::get_branch_out_pc_offset() as i64, vixl::A64AddrModeKind::Offset);
            }
            block_asm.emit_exit_guest_context(ptr::addr_of_mut!(self.runtime_data.host_sp));
        }
    }

    /// External branch: store the guest PC, flush through pre_branch (cycles, scheduler,
    /// pre_cycle_count_sum), then pop the frame and tail-jump to the target's jit entry —
    /// the host stack stays flat across compiled block chains (arm32's restore_stack + bx).
    fn emit_external_branch(&mut self, block_asm: &mut BlockAsm, inst_index: usize, target: u32, guest_pc: u32, arm7_hle: bool) {
        let thumb = block_asm.thumb;
        let current_pc = guest_pc + ((inst_index as u32) << if thumb { 1 } else { 2 });
        let total_cycles = self.jit_buf.insts_cycle_counts[inst_index];
        let tagged_target = target | thumb as u32;

        block_asm.mov_imm(SCRATCH0, tagged_target);
        block_asm.store_guest(SCRATCH0, Reg::PC);

        block_asm.masm.mov_imm64(A64Reg::X0, self as *mut JitAsm as u64);
        block_asm.mov_imm(A64Reg::X1, total_cycles as u32);
        block_asm.mov_imm(A64Reg::X2, 0);
        block_asm.mov_imm(A64Reg::X3, current_pc);
        block_asm.call_host(match (self.cpu, arm7_hle) {
            (ARM9, true) => pre_branch::<{ ARM9 }, false, true> as *const (),
            (ARM9, false) => pre_branch::<{ ARM9 }, false, false> as *const (),
            (ARM7, true) => pre_branch::<{ ARM7 }, false, true> as *const (),
            (ARM7, false) => pre_branch::<{ ARM7 }, false, false> as *const (),
        });

        if BRANCH_LOG {
            block_asm.mov_imm(A64Reg::X0, current_pc);
            block_asm.mov_imm(A64Reg::X1, target);
            block_asm.call_host(match self.cpu {
                ARM9 => debug_branch_imm::<{ ARM9 }> as *const (),
                ARM7 => debug_branch_imm::<{ ARM7 }> as *const (),
            });
        }

        // The entry slot is re-read at run time — invalidation or a fresh compile of the
        // target swaps what the chain lands in (DEFAULT_JIT_ENTRY = emit_code_block).
        // An ARM-mode B never changes mode, so no thumb-bit handling here; the thumb
        // slice must strip/route entry bit0 tags before br (a64 can't interwork-jump).
        let target_entry_slot = self.emu.jit.jit_memory_map.get_jit_entry(target);
        block_asm.masm.mov_imm64(SCRATCH0, target_entry_slot as u64);
        block_asm.masm.ldr_off(SCRATCH0, true, SCRATCH0, 0, vixl::A64AddrModeKind::Offset);
        block_asm.mov_imm(A64Reg::X0, tagged_target);
        block_asm.restore_frame();
        block_asm.masm.br(SCRATCH0);
    }
}
