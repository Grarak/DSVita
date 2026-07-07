// The aarch64 thumb branch lowering (stage-5 slice 5): kind construction for the thumb
// branch ops (B/Bcc offsets are relative to pc + 4 and pre-shifted by the decoder; the
// InstInfo conversion already mapped the Bcc conditions onto Cond, so the shared
// conditional machinery applies unchanged) and the BL/BLX long-call second half.

use super::super::emit_branch::BranchKind;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::aarch64::{BlockAsm, SCRATCH0};
use crate::jit::inst_branch_handler::branch_reg;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm::JitAsm;
use crate::jit::op::Op;
use crate::jit::reg::Reg;
use vixl::A64Reg;

/// What a taken thumb branch does, if `inst` is one. The ARM7 BLX forms are no-ops
/// (NooDS rule) — they return None and fall to the data-processing path, which emits
/// nothing but keeps the per-inst log hook.
pub(in super::super) fn thumb_branch_kind(inst: &InstInfo, pc: u32, cpu: CpuType) -> Option<BranchKind> {
    match inst.op {
        Op::BT | Op::BeqT | Op::BneT | Op::BcsT | Op::BccT | Op::BmiT | Op::BplT | Op::BvsT | Op::BvcT | Op::BhiT | Op::BlsT | Op::BgeT | Op::BltT | Op::BgtT | Op::BleT => {
            let relative = inst.operands()[0].as_imm().unwrap() as i32;
            Some(BranchKind::B {
                target: pc.wrapping_add(4).wrapping_add_signed(relative),
            })
        }
        Op::BxRegT => {
            let reg = inst.operands()[0].as_reg_no_shift().unwrap();
            Some(if reg == Reg::LR { BranchKind::BxReturn } else { BranchKind::BxReg { reg } })
        }
        Op::BlxRegT if cpu == ARM9 => Some(BranchKind::BlxReg {
            reg: inst.operands()[0].as_reg_no_shift().unwrap(),
        }),
        Op::BlOffT => Some(BranchKind::BlOff {
            off: inst.operands()[0].as_imm().unwrap(),
            to_arm: false,
        }),
        Op::BlxOffT if cpu == ARM9 => Some(BranchKind::BlOff {
            off: inst.operands()[0].as_imm().unwrap(),
            to_arm: true,
        }),
        _ => None,
    }
}

impl JitAsm<'_> {
    /// Thumb BL/BLX second half: the target is LR + (off11 << 1) at run time (BlSetupT
    /// staged LR; solo halves — a branch into the pair — still behave, exactly like the
    /// interpreter, which also just reads LR). Calls through branch_reg like BLX reg.
    pub(in super::super) fn emit_branch_link_off(&mut self, block_asm: &mut BlockAsm, inst_index: usize, off: u32, to_arm: bool, guest_pc: u32, arm7_hle: bool) {
        debug_assert!(block_asm.thumb);
        let current_pc = guest_pc + ((inst_index as u32) << 1);
        let total_cycles = self.jit_buf.insts_cycle_counts[inst_index];
        let lr = (current_pc + 2) | 1;

        block_asm.load_guest(A64Reg::X1, Reg::LR);
        block_asm.masm.add_imm(A64Reg::X1, A64Reg::X1, off as u64, false);
        if to_arm {
            // BLX: ARM-mode target, word-aligned, bit 0 clear.
            block_asm.masm.and_imm(A64Reg::X1, A64Reg::X1, !3u32 as u64, false);
        } else {
            // BL: stay in thumb ((target & !1) | 1 == target | 1).
            block_asm.masm.orr_imm(A64Reg::X1, A64Reg::X1, 1, false);
        }
        block_asm.store_guest(A64Reg::X1, Reg::PC);
        block_asm.mov_imm(SCRATCH0, lr);
        block_asm.store_guest(SCRATCH0, Reg::LR);

        block_asm.mov_imm(A64Reg::X0, total_cycles as u32);
        block_asm.mov_imm(A64Reg::X2, lr);
        block_asm.mov_imm(A64Reg::X3, current_pc);
        block_asm.call_host(match (self.cpu, arm7_hle) {
            (ARM9, true) => branch_reg::<{ ARM9 }, true, true> as *const (),
            (ARM9, false) => branch_reg::<{ ARM9 }, true, false> as *const (),
            (ARM7, true) => branch_reg::<{ ARM7 }, true, true> as *const (),
            (ARM7, false) => branch_reg::<{ ARM7 }, true, false> as *const (),
        });
    }
}
