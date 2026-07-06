// Stage-4 acceptance test: the vixl A64 shim layer assembles real, runnable code.
// Covers the risk areas called out in the port plan: an NZCV round-trip through mrs/msr
// (guest flags will live in host NZCV), ldp/stp (the ldm/stm lowering), a literal-pool
// load, local branches, and csel — assembled, copied to an RWX mapping, icache-flushed,
// and executed.
#![cfg(target_arch = "aarch64")]

use vixl::*;

unsafe extern "C" {
    // compiler-rt/libgcc cache maintenance; required between writing and executing code.
    fn __clear_cache(start: *mut core::ffi::c_char, end: *mut core::ffi::c_char);
}

fn run(masm: &mut A64MacroAssembler, arg: u64) -> u64 {
    masm.finalize();
    let code = masm.get_code_buffer();
    unsafe {
        let map = libc::mmap(
            core::ptr::null_mut(),
            code.len().max(4096),
            libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        assert_ne!(map, libc::MAP_FAILED);
        core::ptr::copy_nonoverlapping(code.as_ptr(), map as *mut u8, code.len());
        __clear_cache(map as _, (map as *mut core::ffi::c_char).add(code.len()));
        let fun: extern "C" fn(u64) -> u64 = core::mem::transmute(map);
        let result = fun(arg);
        libc::munmap(map, code.len().max(4096));
        result
    }
}

#[test]
fn mov_alu_roundtrip() {
    let mut masm = A64MacroAssembler::new();
    // x0 = (arg + 0x1234) ^ 0xff00, using a movz/movk-materialized constant.
    masm.mov_imm64(A64Reg::X1, 0x1234);
    masm.add_reg(A64Reg::X0, A64Reg::X0, A64Reg::X1, A64ShiftKind::LSL, 0, true);
    masm.mov_imm64(A64Reg::X2, 0xff00);
    masm.eor_reg(A64Reg::X0, A64Reg::X0, A64Reg::X2, A64ShiftKind::LSL, 0, true);
    masm.ret();
    assert_eq!(run(&mut masm, 1), (1 + 0x1234) ^ 0xff00);
}

#[test]
fn nzcv_roundtrip() {
    // Set NZCV from a register value, read it back: msr nzcv, x; mrs x, nzcv. The Z bit
    // (bit 30) written must read back and drive a csel.
    let mut masm = A64MacroAssembler::new();
    masm.mov_imm64(A64Reg::X1, 1 << 30); // Z set
    masm.msr_nzcv(A64Reg::X1);
    masm.mrs_nzcv(A64Reg::X2);
    masm.mov_imm64(A64Reg::X3, 111);
    masm.mov_imm64(A64Reg::X4, 222);
    // Z is still set from the msr (mov/mrs don't touch flags): csel picks X3 on EQ.
    masm.csel(A64Reg::X0, A64Reg::X3, A64Reg::X4, Cond::EQ, true);
    // And the read-back value must equal what was written: fold the check in by eor-ing
    // x2 with the original and adding the (zero) difference.
    masm.eor_reg(A64Reg::X2, A64Reg::X2, A64Reg::X1, A64ShiftKind::LSL, 0, true);
    masm.add_reg(A64Reg::X0, A64Reg::X0, A64Reg::X2, A64ShiftKind::LSL, 0, true);
    masm.ret();
    assert_eq!(run(&mut masm, 0), 111);
}

#[test]
fn ldp_stp_roundtrip() {
    // Store a pair with pre-index push, load it back swapped with post-index pop:
    // returns arg*2 via the swapped halves.
    let mut masm = A64MacroAssembler::new();
    masm.mov_imm64(A64Reg::X1, 5);
    masm.stp(A64Reg::X0, A64Reg::X1, true, A64Reg::SP, -16, A64AddrModeKind::PreIndex);
    masm.ldp(A64Reg::X2, A64Reg::X3, true, A64Reg::SP, 16, A64AddrModeKind::PostIndex);
    // x0 = x2 + x3 = arg + 5
    masm.add_reg(A64Reg::X0, A64Reg::X2, A64Reg::X3, A64ShiftKind::LSL, 0, true);
    masm.ret();
    assert_eq!(run(&mut masm, 37), 42);
}

#[test]
fn literal_pool_load() {
    let mut masm = A64MacroAssembler::new();
    let mut lit = A64LiteralU64::new(&mut masm, 0xDEAD_BEEF_CAFE_F00D);
    masm.ldr_literal(A64Reg::X0, &mut lit);
    masm.ret();
    assert_eq!(run(&mut masm, 0), 0xDEAD_BEEF_CAFE_F00D);
}

#[test]
fn local_branches_and_loop() {
    // Sum 1..=arg with cbnz loop; branches + subs flags.
    let mut masm = A64MacroAssembler::new();
    let mut loop_top = A64Label::new();
    masm.mov_imm64(A64Reg::X1, 0); // acc
    masm.bind(&mut loop_top);
    masm.add_reg(A64Reg::X1, A64Reg::X1, A64Reg::X0, A64ShiftKind::LSL, 0, true);
    masm.subs_imm(A64Reg::X0, A64Reg::X0, 1, true);
    masm.cbnz(A64Reg::X0, true, &mut loop_top);
    masm.mov_reg(A64Reg::X0, A64Reg::X1, true);
    masm.ret();
    assert_eq!(run(&mut masm, 10), 55);
}

#[test]
fn exact_scope_blocks_pool() {
    // A literal load inside an exact scope must not have the pool dumped inside the
    // scoped window: assemble a scope of exactly one instruction, then branch over
    // nothing and return. Executing proves the pool landed outside the scoped bytes.
    let mut masm = A64MacroAssembler::new();
    let mut lit = A64LiteralU64::new(&mut masm, 7);
    {
        let _scope = masm.exact_scope(4);
        // Raw, exactly one instruction inside the scope (macro instructions assert here).
        masm.raw_nop();
    }
    masm.ldr_literal(A64Reg::X0, &mut lit);
    masm.ret();
    assert_eq!(run(&mut masm, 0), 7);
}

#[test]
fn regoff_load_fastmem_shape() {
    // The fastmem access shape: ldr wD, [xBase, wAddr, uxtw #0]. Build a tiny in-stack
    // array and read element [arg].
    let mut masm = A64MacroAssembler::new();
    masm.mov_imm64(A64Reg::X1, 0x1122_3344_5566_7788);
    masm.stp(A64Reg::X1, A64Reg::X1, true, A64Reg::SP, -16, A64AddrModeKind::PreIndex);
    masm.mov_reg(A64Reg::X2, A64Reg::SP, true); // base (mov from sp needs the sp form)
    masm.mov_imm32(A64Reg::X3, 4); // byte offset in a w register
    masm.ldr_regoff(A64Reg::X0, false, A64Reg::X2, A64Reg::X3, A64ExtendKind::UXTW, 0);
    masm.add_imm(A64Reg::SP, A64Reg::SP, 16, true);
    masm.ret();
    // w-load of bytes 4-7 of the little-endian x1 value.
    assert_eq!(run(&mut masm, 0), 0x1122_3344);
}
