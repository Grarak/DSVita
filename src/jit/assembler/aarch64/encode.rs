// Raw A64 encoders for the slow-mem patcher: the SIGSEGV handler rewrites a compiled
// fast-mem window in place, outside any assembler context, so these emit fixed encodings
// straight into the buffer (the arm32 twin uses the jit::inst arm encoders the same way).

use vixl::A64Reg;

pub const NOP: u32 = 0xd503201f;

const fn r(reg: A64Reg) -> u32 {
    reg as u32
}

/// movz wd/xd, #imm16, lsl #(shift16 * 16)
pub const fn movz(rd: A64Reg, imm16: u16, shift16: u32, is64: bool) -> u32 {
    ((is64 as u32) << 31) | (0b10100101 << 23) | (shift16 << 21) | ((imm16 as u32) << 5) | r(rd)
}

/// movk wd/xd, #imm16, lsl #(shift16 * 16)
pub const fn movk(rd: A64Reg, imm16: u16, shift16: u32, is64: bool) -> u32 {
    ((is64 as u32) << 31) | (0b11100101 << 23) | (shift16 << 21) | ((imm16 as u32) << 5) | r(rd)
}

/// orr wd, wzr, wm — the canonical register move.
pub const fn mov_reg(rd: A64Reg, rm: A64Reg) -> u32 {
    0x2a0003e0 | (r(rm) << 16) | r(rd)
}

pub const fn blr(rn: A64Reg) -> u32 {
    0xd63f0000 | (r(rn) << 5)
}

/// cbz xt, #(offset_insts * 4)
pub const fn cbz64(rt: A64Reg, offset_insts: i32) -> u32 {
    0xb4000000 | (((offset_insts as u32) & 0x7FFFF) << 5) | r(rt)
}

/// Emit `mov` of an arbitrary 64-bit constant (movz + up to 3 movk, fixed 4-inst form so
/// patch sizes stay constant).
pub fn emit_mov_imm64(buf: &mut Vec<u32>, rd: A64Reg, value: u64) {
    buf.push(movz(rd, value as u16, 0, true));
    buf.push(movk(rd, (value >> 16) as u16, 1, true));
    buf.push(movk(rd, (value >> 32) as u16, 2, true));
    buf.push(movk(rd, (value >> 48) as u16, 3, true));
}
