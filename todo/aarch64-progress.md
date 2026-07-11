# aarch64 port — progress and open items

Plan: `todo/aarch64-port.md`. Work branch: `aarch64-port-s1`.
Test setup: arm64 binaries run on the local machine; arm32 binaries on the pi5 (qemu-arm only as fallback).

## Stage 1 — interpreter-only host

### Done (main, `0768d31`)
- De-risk probe: all six fixed regions + 32MB RWX at 0x20000000 map on both boxes; aarch64
  mcontext accessors and icache flush verified.
- Build plumbing: `aarch64-unknown-linux-gnu` target, arch-correct C flags/bindgen triples,
  math-neon gated to arm, vendored simd-adler32 with conditional feature gate.
- arm32 jit pipeline cfg-gated out; shared types (`GuestInst*`, pool consts) moved to
  `assembler::`; jit memory map widened u32 → usize (PIE pointers); per-arch guest-context
  frames (call_jit_entry / exit_guest_context / interrupt return) and sigsegv accessors.
- NEON imports aliased per-arch; portable GX dispatch loop (accounting-exact twin of the asm);
  scalar 3x3 wrappers for the math-neon calls.
- hello_world runs fully interpreted on aarch64.

### Done (branch, since `f25fd7e`)
- Full interpreter instruction coverage from the NooDS reference (no fallback module, no
  idle-loop detection — both rejected): swi, mcr/mrc (+cp15 wait-for-irq halt), swp/swpb,
  ldrd/strd, dsp extensions (qadd family, halfword multiplies, sticky Q), blx imm in the
  cond-0xF gate (arm7: NV never executes; arm9 non-blx falls to the jit), user-banked ldm/stm,
  empty rlists as no-ops.
- User-bank routing bug found via healthy-vs-changed trace diff: only sp/lr are banked outside
  fiq; r8-r12 share the active file (jit rule in `inst_mem_handler.rs get_reg_usr_mut`).
  This was the mode-0 cpsr corruption killing every deep boot.
- Bios-sentinel branch targets null-check the hotness counter instead of asserting.
- Empty-block compile hardened into a diagnosable assert (`emit.rs`).
- Cycle counts audited against NooDS: ldrd 3→2, smlal* 1→2, swp/swpb per-cpu (arm7 4 / arm9 2).
  Others confirmed (mcr/mrc/dsp/qadd = 1, swi = 3).

### Resolved — the threshold-100 crash regression (was blocking the merge)
- **Root cause: the interpreter's cond-0xF BLX imm never wrote the guest LR** (and charged
  3 cycles where the disassembler/jit charge 1). `bl`/`blx reg`/thumb handlers all set LR in
  the handler; the inline BLX imm path in `interpret_block_inner` only passed the return
  address to the return-stack push. An interpreted `blx imm` callsite (ARM→thumb interwork)
  then ran its callee with a stale LR: the callee's `bx lr` jumped mid-way back into an
  outer function, the return stack mismatched (safety-net exit), and the outer function
  re-ran half its body with sp still 0x30 low — its epilogue stores landed on a callback
  table in the caller's frame (dtcm 0x27e35c0-c8 zeroed), and the next dispatch called the
  nulled slot → ITCM garbage walk → unmapped-dispatch panic. Both observed failures (pi
  deterministic sound-tick crash, qemu flaky second-ipc-handshake death) were this one bug
  at different callsites/timings.
- Found by scanning both 24GB pi ilogs (failing + healthy) for every access to the table
  region with a standalone EA-reconstructing record scanner; the healthy run executed the
  same epilogue 2775x at sp=27e3588 (harmless), the failing run once at sp=27e3558.
  The trace text `failed to branch lr from 20cb11e to 2098fe8 desired: 209dc8c` right at a
  `branch reg from 209dc88 to 20cb109` with no register holding the target named the exact
  instruction.
- Fix: write `(*regs).lr = addr + 4` in the BLX imm arm and charge 1 cycle (disassembler
  value; the flat loop's +2 branch epilogue applies like every branch).
- Verified on the pi5 (release, threshold 100): HG -e 0 uncapped 3/3 runs clean past the
  old deterministic death point (120s/60s/60s wall vs 1-3s to crash before), title intro
  rendering; a64 local build runs it clean too. Vita vpk builds. No emitter/jit changes.
- **Disassembler cycle values disagree with NooDS** for: swp (disasm 4, NooDS arm9 2),
  ldrd (disasm 3, NooDS 2), ldm/stm formula (above). The jit charges the disassembler values.
  Interpreter now follows NooDS for the newly implemented ops → interp/jit parity for these
  ops differs by the delta. Needs a maintainer call: fix the disassembler (changes jit timing)
  or mirror the disassembler in the interpreter.
- a64 visual verification of a commercial boot still outstanding (local machine is headless;
  HG runs 180s+ deep and clean by trace/marker evidence).

## Stage 2 — cross-arch tracediff harness
- Not started as a harness. Learned: record pairing breaks across jit-vs-interp variants of
  the same function (unlogged taken branches + jit-only bl records) — strict mode needs
  same-engine pairs or a smarter differ.

## Stages 3-6
- Not started (seam refactor, vixl a64 with hardcoded instruction lists, A64 backend,
  jit-vs-jit tracediff loop).

## Env notes
- qemu HG: release ≈ 500 vblank-lines/s, release-debug+DEBUG_LOG ≈ 50/s.
- DEBUG_LOG=true with the plain release profile is an ILLEGAL combo (BRANCH_LOG paths call
  debug-only accessors that panic in release). Trace = release-debug profile only.
- ilogs are tens of GB — stream them, delete after.
