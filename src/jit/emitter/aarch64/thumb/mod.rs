// The aarch64 thumb emitter, laid out like emitter/arm32/thumb: emit_alu_thumb.rs
// lowers the thumb data-processing ops, emit_branch_thumb.rs constructs the thumb
// branch kinds and lowers the long-call halves. The branch execution machinery itself
// (labels, accounting, tails, chains) is shared with ARM in ../emit_branch.rs.

use crate::jit::inst_info::{InstInfo, Operand};
use crate::jit::op::Op;
use crate::jit::reg::Reg;

mod emit_alu_thumb;
mod emit_branch_thumb;

pub(super) use emit_alu_thumb::emit_thumb_data_processing;
pub(super) use emit_branch_thumb::thumb_branch_kind;

/// Thumb per-instruction support: the ALU and branch coverage of the thumb slice.
/// Transfers, push/pop/ldm/stm, swi and the register-amount shift forms stay refused
/// (the same shift-by-register carry rules the ARM path refuses).
pub(super) fn is_inst_supported_thumb(inst: &InstInfo) -> bool {
    if inst.op.is_single_mem_transfer() && !super::emit::class_disabled('m') {
        return super::emit_transfer::is_single_transfer_supported(inst, true);
    }
    if inst.op.is_multiple_mem_transfer() && !super::emit::class_disabled('m') {
        return super::emit_transfer::is_multiple_transfer_supported(inst);
    }
    match inst.op {
        // The dedicated shift instructions: immediate-amount forms only.
        Op::LslT | Op::LsrT | Op::AsrT => matches!(inst.operands()[2], Operand::Imm(_)),
        Op::RorT => false, // register-amount only in thumb
        // add/mov with a high-register destination can target the PC — refuse those.
        Op::AddHT | Op::MovHT => !inst.out_regs.is_reserved(Reg::PC),
        Op::AddT
        | Op::SubT
        | Op::NegT
        | Op::CmpT
        | Op::CmpHT
        | Op::CmnT
        | Op::MovT
        | Op::MvnT
        | Op::AndT
        | Op::EorT
        | Op::OrrT
        | Op::BicT
        | Op::TstT
        | Op::AdcT
        | Op::SbcT
        | Op::MulT
        | Op::AddPcT
        | Op::AddSpT
        | Op::AddSpImmT
        | Op::BlSetupT => true,
        Op::BT
        | Op::BeqT
        | Op::BneT
        | Op::BcsT
        | Op::BccT
        | Op::BmiT
        | Op::BplT
        | Op::BvsT
        | Op::BvcT
        | Op::BhiT
        | Op::BlsT
        | Op::BgeT
        | Op::BltT
        | Op::BgtT
        | Op::BleT
        | Op::BxRegT
        | Op::BlxRegT
        | Op::BlOffT
        | Op::BlxOffT => true,
        _ => false,
    }
}
