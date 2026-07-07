// The aarch64 single-transfer emitter (stage-5 fastmem slice, arm32's emit_transfer
// shape): the fast path accesses the 256MB-mirrored guest window directly —
// `[mmu_base, aligned_addr, uxtw]` — inside a nop-padded window sized for the slow-path
// patch. A SIGSEGV in the window (io region, or a write into a jit-protected page)
// makes patch_slow_mem rewrite it in place with a call to the shared inst_mem_handler
// cores; the canonical register contract with the patcher is:
//   SCRATCH2 (w11) = the unaligned guest address of the access,
//   the mapped op0  = the value written / loaded (metadata carries the mapping).
// Word reads rotate unaligned values like the ARM does (the slow handler returns them
// pre-rotated); halfword/byte accesses go aligned and unrotated, matching arm32's fast
// path. Refused here (still interpreted): PC-destination loads, PC-source stores,
// ldrd/strd, RRX-shifted offsets.

use crate::core::CpuType;
use crate::jit::assembler::aarch64::{BlockAsm, SCRATCH0, SCRATCH1, SCRATCH2};
use crate::jit::inst_info::{InstInfo, Operand};
use crate::jit::inst_info::{Shift, ShiftValue};
use crate::jit::jit_asm::JitAsm;
use crate::jit::jit_memory::SLOW_MEM_SINGLE_LENGTH_A64;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use vixl::{A64AddrModeKind, A64ExtendKind, A64Reg, A64ShiftKind};

/// Whether the a64 backend can compile this single transfer.
pub(in super::super) fn is_single_transfer_supported(inst: &InstInfo, thumb: bool) -> bool {
    let transfer = match inst.op {
        Op::Ldr(transfer) | Op::LdrT(transfer) | Op::Str(transfer) | Op::StrT(transfer) => transfer,
        _ => return false,
    };
    // ldrd/strd later (needs the pair rotation dance).
    if transfer.size() == 3 {
        return false;
    }
    let operands = inst.operands();
    let op0 = match operands[0].as_reg_no_shift() {
        Some(reg) => reg,
        None => return false,
    };
    // PC-destination loads are block terminals (pop pc) — a later slice; PC-source
    // stores read pc+12 — rare, keep interpreting.
    if op0 == Reg::PC {
        return false;
    }
    // PC-base only as a pre-indexed compile-time fold (imm_transfer_addr's shape).
    if inst.operands()[1].as_reg_no_shift() == Some(Reg::PC) && !transfer.pre() {
        return false;
    }
    match &operands[2] {
        Operand::Imm(_) => true,
        Operand::Reg { reg, shift } => {
            if *reg == Reg::PC {
                return false;
            }
            match shift {
                None => true,
                // Shift-by-immediate only in addressing; amount 0 on ROR means RRX.
                Some(shift) => {
                    let (kind_is_ror, value) = match shift {
                        Shift::Lsl(v) => (false, v),
                        Shift::Lsr(v) => (false, v),
                        Shift::Asr(v) => (false, v),
                        Shift::Ror(v) => (true, v),
                    };
                    match value {
                        ShiftValue::Imm(amount) => !(kind_is_ror && *amount == 0),
                        ShiftValue::Reg(_) => false,
                    }
                }
            }
        }
        _ => false,
    }
}

/// Whether the a64 backend can compile this multiple transfer: everything except
/// PC-in-list (pop pc is a block terminal — a later slice with the PC-destination
/// loads). The lowering is always-slow (no fast path yet).
pub(in super::super) fn is_multiple_transfer_supported(inst: &InstInfo) -> bool {
    // Lowering wired end-to-end (always-slow trampoline into the portable core) but OFF:
    // with it on, hello_world's strict pair splits on ARM7 irq phase — an interrupt
    // dispatches one instruction earlier than the interpreter right after a compiled
    // thumb pop (T-loss scan: rec ~130k, pop {r0} at 0x3802308 → vector 0x23800b0).
    // Cycle-accounting detail in the multiple path vs the interpreter's spread charge —
    // pick up from the handover's playbook (text-pair + scheduler-firing grep).
    if true {
        return false;
    }
    let rlist = match inst.operands()[1].as_reg_list() {
        Some(rlist) => rlist,
        None => return false,
    };
    !rlist.is_reserved(Reg::PC)
}

impl JitAsm<'_> {
    /// ldm/stm as a single call into the portable multiple-transfer core through the
    /// dump-dance trampoline (arm32's slow-mem shape, selected at emit time — no fast
    /// path, no patching). The handler works on guest memory and refreshes mapped pool
    /// registers through the dump; dirty values spill first so it reads current state.
    pub(in super::super) fn emit_multiple_transfer(&mut self, block_asm: &mut BlockAsm, inst_index: usize, pc: u32) {
        let inst = &self.jit_buf.insts[inst_index];
        let thumb = block_asm.thumb;

        let transfer = match inst.op {
            Op::Ldm(transfer) | Op::LdmT(transfer) | Op::Stm(transfer) | Op::StmT(transfer) => transfer,
            _ => unsafe { std::hint::unreachable_unchecked() },
        };
        let is_write = inst.op.is_write_mem_transfer();
        let op0 = inst.operands()[0].as_reg_no_shift().unwrap();
        let rlist = inst.operands()[1].as_reg_list().unwrap();
        let user = transfer.user() && !rlist.is_reserved(Reg::PC);
        let valid = !transfer.write_back() || !rlist.is_reserved(op0);

        // The driver already spilled and cleared (before the condition check — a
        // skipped body must not skip the spills). The empty mapping snapshot in the
        // metadata makes the handler's dump-writes no-ops.
        let cursor = block_asm.masm.get_cursor_offset();
        block_asm.guest_inst_metadata(self.jit_buf.insts_cycle_counts[inst_index], inst, cursor, vixl::A64Reg::ZR, pc | (thumb as u32));
        let metadata_ptr = block_asm.last_guest_inst_metadata();

        let mut pre = transfer.pre();
        if !transfer.add() {
            pre = !pre;
        }
        let params = crate::jit::inst_mem_handler::InstMemMultipleParams::new(
            rlist.0 as u16,
            bilge::arbitrary_int::u4::new(rlist.len() as u8),
            bilge::arbitrary_int::u4::new(op0 as u8),
            pre,
            transfer.user(),
            bilge::arbitrary_int::u6::new(0),
        );

        let handler = match self.cpu {
            CpuType::ARM9 => crate::jit::jit_memory::JitMemory::get_inst_mem_multiple_handler_fun_a64::<{ CpuType::ARM9 }>(is_write, transfer, user, valid, false),
            CpuType::ARM7 => crate::jit::jit_memory::JitMemory::get_inst_mem_multiple_handler_fun_a64::<{ CpuType::ARM7 }>(is_write, transfer, user, valid, false),
        };

        block_asm.mov_imm(A64Reg::X0, u32::from(params));
        block_asm.masm.mov_imm64(A64Reg::X1, metadata_ptr as u64);
        block_asm.call_host(handler);
    }

    /// The offset operand as a value in a register (or a fold into the compile-time
    /// address when both base and offset are constant — handled by the caller).
    fn transfer_offset_value(block_asm: &mut BlockAsm, operand: &Operand) -> Option<A64Reg> {
        match operand {
            Operand::Imm(imm) => {
                if *imm == 0 {
                    None
                } else {
                    block_asm.mov_imm(SCRATCH0, *imm);
                    Some(SCRATCH0)
                }
            }
            Operand::Reg { reg, shift: None } => Some(block_asm.guest_map(*reg)),
            Operand::Reg { reg, shift: Some(shift) } => {
                let mapped = block_asm.guest_map(*reg);
                let (kind, amount) = match shift {
                    Shift::Lsl(ShiftValue::Imm(amount)) => (A64ShiftKind::LSL, *amount),
                    Shift::Lsr(ShiftValue::Imm(amount)) => (A64ShiftKind::LSR, *amount),
                    Shift::Asr(ShiftValue::Imm(amount)) => (A64ShiftKind::ASR, *amount),
                    Shift::Ror(ShiftValue::Imm(amount)) => (A64ShiftKind::ROR, *amount),
                    _ => unsafe { std::hint::unreachable_unchecked() },
                };
                // A32 encodes lsr/asr #32 as amount 0.
                let amount = if amount == 0 && matches!(kind, A64ShiftKind::LSR | A64ShiftKind::ASR) {
                    32
                } else {
                    amount as u32
                };
                if amount == 0 {
                    Some(mapped)
                } else {
                    block_asm.shift_reg_imm(SCRATCH0, mapped, kind, amount % 32);
                    if amount == 32 && kind == A64ShiftKind::LSR {
                        block_asm.mov_imm(SCRATCH0, 0);
                    } else if amount == 32 && kind == A64ShiftKind::ASR {
                        block_asm.masm.asr_imm(SCRATCH0, mapped, 31, false);
                    }
                    Some(SCRATCH0)
                }
            }
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub(in super::super) fn emit_single_transfer(&mut self, block_asm: &mut BlockAsm, inst_index: usize, pc: u32) {
        let inst = &self.jit_buf.insts[inst_index];
        let thumb = block_asm.thumb;

        let transfer = match inst.op {
            Op::Ldr(transfer) | Op::LdrT(transfer) | Op::Str(transfer) | Op::StrT(transfer) => transfer,
            _ => unsafe { std::hint::unreachable_unchecked() },
        };
        let is_write = inst.op.is_write_mem_transfer();
        let size: u8 = 1 << transfer.size();

        let operands = inst.operands();
        let op0 = operands[0].as_reg_no_shift().unwrap();
        let op1 = operands[1].as_reg_no_shift().unwrap();
        let op0_mapped = block_asm.guest_map(op0);

        // --- Pre-window: the canonical unaligned access address into SCRATCH2 (w11),
        // base writeback into the mapped base. All patch-immune.
        let imm_addr = inst.imm_transfer_addr(pc);
        match imm_addr {
            Some(imm_addr) => {
                // Base is PC (or fully constant): fold the whole address.
                block_asm.mov_imm(SCRATCH2, if transfer.pre() { imm_addr } else { unsafe { std::hint::unreachable_unchecked() } });
            }
            None => {
                if op1 == Reg::PC {
                    // PC base with a register offset: materialize the pipeline value.
                    let pc_value = if thumb { (pc + 4) & !3 } else { pc + 8 };
                    block_asm.mov_imm(SCRATCH1, pc_value);
                }
                let base = if op1 == Reg::PC { SCRATCH1 } else { block_asm.guest_map(op1) };
                let offset = Self::transfer_offset_value(block_asm, &operands[2]);

                if transfer.pre() {
                    match offset {
                        Some(offset) => {
                            if transfer.add() {
                                block_asm.masm.add_reg(SCRATCH2, base, offset, A64ShiftKind::LSL, 0, false);
                            } else {
                                block_asm.masm.sub_reg(SCRATCH2, base, offset, A64ShiftKind::LSL, 0, false);
                            }
                        }
                        None => block_asm.masm.mov_reg(SCRATCH2, base, false),
                    }
                } else {
                    // Post-indexed: access at the original base.
                    block_asm.masm.mov_reg(SCRATCH2, base, false);
                }

                // Writeback (skipped when a load's destination is the base — the loaded
                // value wins, like arm32).
                let write_back = if transfer.pre() { transfer.write_back() } else { true };
                if write_back && (is_write || op0 != op1) && op1 != Reg::PC {
                    let base_mapped = block_asm.guest_map(op1);
                    if transfer.pre() {
                        block_asm.masm.mov_reg(base_mapped, SCRATCH2, false);
                    } else {
                        match offset {
                            Some(offset) => {
                                if transfer.add() {
                                    block_asm.masm.add_reg(base_mapped, base_mapped, offset, A64ShiftKind::LSL, 0, false);
                                } else {
                                    block_asm.masm.sub_reg(base_mapped, base_mapped, offset, A64ShiftKind::LSL, 0, false);
                                }
                            }
                            None => {}
                        }
                    }
                    block_asm.mark_guest_dirty(op1);
                }
            }
        }

        // Mirror base for the fast window (patch-immune).
        block_asm.masm.mov_imm64(SCRATCH1, self.cpu.mmu_tcm_addr() as u64);

        // --- The patchable window.
        let fast_mem_start = block_asm.masm.get_cursor_offset();

        // Strip the top nibble and align.
        let mask = !(0xF000_0000u32 | (size as u32 - 1));
        block_asm.masm.and_imm(SCRATCH0, SCRATCH2, mask as u64, false);

        // Keyed by the faulting instruction's exact offset — record just before it.
        block_asm.guest_inst_metadata(self.jit_buf.insts_cycle_counts[inst_index], inst, fast_mem_start, op0_mapped, pc | (thumb as u32));

        if is_write {
            match size {
                1 => block_asm.masm.strb_regoff(op0_mapped, SCRATCH1, SCRATCH0, A64ExtendKind::UXTW, 0),
                2 => block_asm.masm.strh_regoff(op0_mapped, SCRATCH1, SCRATCH0, A64ExtendKind::UXTW, 0),
                4 => block_asm.masm.str_regoff(op0_mapped, false, SCRATCH1, SCRATCH0, A64ExtendKind::UXTW, 0),
                _ => unsafe { std::hint::unreachable_unchecked() },
            }
        } else {
            match (size, transfer.signed()) {
                (1, false) => block_asm.masm.ldrb_regoff(op0_mapped, SCRATCH1, SCRATCH0, A64ExtendKind::UXTW, 0),
                (1, true) => block_asm.masm.ldrsb_regoff(op0_mapped, SCRATCH1, SCRATCH0, A64ExtendKind::UXTW, 0),
                (2, false) => block_asm.masm.ldrh_regoff(op0_mapped, SCRATCH1, SCRATCH0, A64ExtendKind::UXTW, 0),
                (2, true) => block_asm.masm.ldrsh_regoff(op0_mapped, SCRATCH1, SCRATCH0, A64ExtendKind::UXTW, 0),
                (4, _) => {
                    block_asm.masm.ldr_regoff(op0_mapped, false, SCRATCH1, SCRATCH0, A64ExtendKind::UXTW, 0);
                    // Unaligned word reads rotate (the slow handler returns rotated
                    // values; compile-time-constant addresses fold the rotation).
                    match imm_addr {
                        Some(imm_addr) => {
                            let shift = (imm_addr & 3) << 3;
                            if shift != 0 {
                                block_asm.masm.ror_imm(op0_mapped, op0_mapped, shift, false);
                            }
                        }
                        None => {
                            block_asm.masm.lsl_imm(SCRATCH0, SCRATCH2, 3, false);
                            block_asm.masm.rorv(op0_mapped, op0_mapped, SCRATCH0);
                        }
                    }
                }
                _ => unsafe { std::hint::unreachable_unchecked() },
            }
            block_asm.mark_guest_dirty(op0);
        }

        // Pad the window to the slow-path size and record it.
        let fast_mem_end = block_asm.masm.get_cursor_offset();
        let fast_len = (fast_mem_end - fast_mem_start) as usize;
        debug_assert!(fast_len <= SLOW_MEM_SINGLE_LENGTH_A64);
        for _ in (fast_len..SLOW_MEM_SINGLE_LENGTH_A64).step_by(4) {
            block_asm.masm.nop();
        }
        block_asm.set_fast_mem_size_last(SLOW_MEM_SINGLE_LENGTH_A64 as u16);
    }
}
