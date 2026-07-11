// The aarch64 thumb emitter, laid out like emitter/arm32/thumb: emit_alu_thumb.rs
// lowers the thumb data-processing ops, emit_branch_thumb.rs constructs the thumb
// branch kinds and lowers the long-call halves. The branch execution machinery itself
// (labels, accounting, tails, chains) is shared with ARM in ../emit_branch.rs.

mod emit_alu_thumb;
mod emit_branch_thumb;

pub(super) use emit_alu_thumb::emit_thumb_data_processing;
pub(super) use emit_branch_thumb::thumb_branch_kind;
