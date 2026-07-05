// Branches. Semantics ported from NooDS/src/interpreter_branch.cpp.

use super::{Ctx, InstResult};

/// Sign-extended 24-bit branch offset (<< 2). The PC-relative add is genuinely modular, so it uses
/// `wrapping_add_signed`; everything else here is a plain address add (overflow would be a bug).
#[inline]
fn branch_offset(opcode: u32) -> i32 {
    (opcode << 8) as i32 >> 6
}

pub(super) fn b(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let target = ctx.reg(15).wrapping_add_signed(branch_offset(opcode));
    InstResult::Branch(1, target & !3)
}

pub(super) fn bl(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let target = ctx.reg(15).wrapping_add_signed(branch_offset(opcode));
    let lr = ctx.inst_addr + 4;
    ctx.set_reg(14, lr);
    InstResult::BranchLink(1, target & !3, lr)
}

pub(super) fn bx(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let target = ctx.reg(opcode & 0xF);
    if opcode & 0xF == 14 {
        // `bx lr`: the jit routes this through the return stack (branch_lr).
        InstResult::BranchReturn(1, target)
    } else {
        InstResult::Branch(1, target)
    }
}

pub(super) fn blx_reg(ctx: &mut Ctx, opcode: u32) -> InstResult {
    let target = ctx.reg(opcode & 0xF);
    let lr = ctx.inst_addr + 4;
    ctx.set_reg(14, lr);
    InstResult::BranchLink(1, target, lr)
}
