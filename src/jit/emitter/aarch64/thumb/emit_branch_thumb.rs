// The aarch64 thumb branch lowering: kind construction for the thumb
// branch ops (B/Bcc offsets are relative to pc + 4 and pre-shifted by the decoder; the
// InstInfo conversion already mapped the Bcc conditions onto Cond, so the shared
// conditional machinery applies unchanged) and the BL/BLX long-call second half.

use super::super::emit_branch::BranchKind;
use crate::core::CpuType;
use crate::core::CpuType::{ARM7, ARM9};
use crate::jit::assembler::aarch64::{BlockAsm, SCRATCH0};
use crate::jit::inst_branch_handler::branch_reg;
use crate::jit::inst_info::InstInfo;
use crate::jit::jit_asm::{align_guest_pc, JitAsm};
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

        let previous_inst_info = &self.jit_buf.insts[inst_index - 1];
        let relative_pc = if previous_inst_info.op != Op::BlSetupT {
            0
        } else {
            previous_inst_info.operands()[0].as_imm().unwrap() as i32
        } + 4;

        let current_pc = guest_pc + ((inst_index as u32) << 1);
        let mut target_pc = (current_pc as i32 - 2 + relative_pc) as u32 + off;

        if to_arm {
            target_pc &= !1;
        } else {
            target_pc |= 1;
        }

        let lr = (current_pc + 2) | 1;

        let is_thumb = target_pc & 1 == 1;
        target_pc = align_guest_pc(target_pc) | is_thumb as u32;

        self.emit_branch_link_external(block_asm, inst_index, target_pc, current_pc, lr, arm7_hle);
    }
}
