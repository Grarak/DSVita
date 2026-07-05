// Thumb branches. Branch targets carry the mode in bit 0 (1 = stay in thumb), like everywhere else
// in the interpreter. Semantics ported from NooDS/src/interpreter_branch.cpp (the *T functions).

use super::{Ctx, InstResult, C_BIT, N_BIT, V_BIT, Z_BIT};
use crate::core::CpuType::ARM7;

/// Conditional branches (format 16): 8-bit offset doubled, relative to pc + 4.
macro_rules! def_bcond {
    ($name:ident, |$cpsr:ident| $taken:expr) => {
        pub(super) fn $name(ctx: &mut Ctx, opcode: u16) -> InstResult {
            let $cpsr = ctx.cpsr();
            if $taken {
                let target = (ctx.inst_addr + 4).wrapping_add_signed((opcode as i8 as i32) << 1);
                InstResult::Branch(1, target | 1)
            } else {
                InstResult::Continue(1)
            }
        }
    };
}

def_bcond!(beq_t, |c| c & Z_BIT != 0);
def_bcond!(bne_t, |c| c & Z_BIT == 0);
def_bcond!(bcs_t, |c| c & C_BIT != 0);
def_bcond!(bcc_t, |c| c & C_BIT == 0);
def_bcond!(bmi_t, |c| c & N_BIT != 0);
def_bcond!(bpl_t, |c| c & N_BIT == 0);
def_bcond!(bvs_t, |c| c & V_BIT != 0);
def_bcond!(bvc_t, |c| c & V_BIT == 0);
def_bcond!(bhi_t, |c| c & C_BIT != 0 && c & Z_BIT == 0);
def_bcond!(bls_t, |c| c & C_BIT == 0 || c & Z_BIT != 0);
def_bcond!(bge_t, |c| (c ^ (c << 3)) & N_BIT == 0);
def_bcond!(blt_t, |c| (c ^ (c << 3)) & N_BIT != 0);
def_bcond!(bgt_t, |c| c & Z_BIT == 0 && (c ^ (c << 3)) & N_BIT == 0);
def_bcond!(ble_t, |c| c & Z_BIT != 0 || (c ^ (c << 3)) & N_BIT != 0);

/// Sign-extended 11-bit offset doubled (formats 18/19).
#[inline]
fn branch_offset(opcode: u16) -> i32 {
    (((opcode << 5) as i16) >> 4) as i32
}

pub(super) fn b_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let target = (ctx.inst_addr + 4).wrapping_add_signed(branch_offset(opcode));
    InstResult::Branch(1, target | 1)
}

/// First half of a long BL/BLX: stage the upper offset bits in LR.
pub(super) fn bl_setup_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let lr = (ctx.inst_addr + 4).wrapping_add_signed(branch_offset(opcode) << 11);
    ctx.set_reg(14, lr);
    InstResult::Continue(1)
}

pub(super) fn bl_off_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    let target = ctx.reg(14).wrapping_add(((opcode & 0x7FF) as u32) << 1);
    let lr = (ctx.inst_addr + 2) | 1;
    ctx.set_reg(14, lr);
    InstResult::BranchLink(1, (target & !1) | 1, lr)
}

pub(super) fn blx_off_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    // ARM9 exclusive, a no-op on ARM7 (like NooDS).
    if ctx.cpu == ARM7 {
        return InstResult::Continue(1);
    }
    let target = ctx.reg(14).wrapping_add(((opcode & 0x7FF) as u32) << 1);
    let lr = (ctx.inst_addr + 2) | 1;
    ctx.set_reg(14, lr);
    InstResult::BranchLink(1, target & !3, lr)
}

pub(super) fn bx_reg_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    // Bit 0 of the target selects the mode.
    let target = ctx.reg_t(((opcode >> 3) & 0xF) as u32);
    if (opcode >> 3) & 0xF == 14 {
        // `bx lr`: the jit routes this through the return stack (branch_lr).
        InstResult::BranchReturn(1, target)
    } else {
        InstResult::Branch(1, target)
    }
}

pub(super) fn blx_reg_t(ctx: &mut Ctx, opcode: u16) -> InstResult {
    // ARM9 exclusive, a no-op on ARM7 (like NooDS).
    if ctx.cpu == ARM7 {
        return InstResult::Continue(1);
    }
    let target = ctx.reg_t(((opcode >> 3) & 0xF) as u32);
    let lr = (ctx.inst_addr + 2) | 1;
    ctx.set_reg(14, lr);
    InstResult::BranchLink(1, target, lr)
}
