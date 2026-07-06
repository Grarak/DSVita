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
  Post-fix gameplay check: HG title → Continue → save loaded → walking the overworld
  (follower + in-game menu working); Diamond -e 2 title → Continue → save loaded, adventure
  journal interactive (its close is touch-only — no remote touch injection, overworld view
  unverified). Mechanism-proof trace: the fixed blx callsite ran 100x with correct lr, first
  occurrence at the exact record index where the broken trace diverged.
- **Disassembler cycle values disagree with NooDS** for: swp (disasm 4, NooDS arm9 2),
  ldrd (disasm 3, NooDS 2), ldm/stm formula (above). The jit charges the disassembler values.
  Interpreter now follows NooDS for the newly implemented ops → interp/jit parity for these
  ops differs by the delta. Needs a maintainer call: fix the disassembler (changes jit timing)
  or mirror the disassembler in the interpreter.
- a64 visual verification of a commercial boot still outstanding (the dev box blocks
  unattended screenshots: GNOME denies the dbus capture, no grim; HG runs 240s+ deep and
  clean by survival/vblank evidence). The pi visual run stands in for rendering.
- **The long-standing pi "bottom-screen 2D garble" is CLOSED: it was the box's GPU driver,
  not emulation.** Software GL (`LIBGL_ALWAYS_SOFTWARE=1`) renders pixel-perfect. Rule
  (maintainer): always run with software rendering — baked into tools/pi_run.sh (the local
  launch scripts already set it) and the run-on-testbox skill.

## Stage 2 — cross-arch tracediff harness: DONE, acceptance met
- `tools/tracediff.sh <rom> [records] [arm7_emu]`: builds armhf (threshold 255) + a64 with
  DEBUG_LOG, runs both from the test box's .sav with `--hle-irq 0`, captures exactly N
  records per side (`DSVITA_INST_LOG_MAX`, race-free logger-side stop — SIGINT tears
  records), pulls, `trace_diff.py --strict` (record-for-record, no resync, first diff
  exits 1).
- **Acceptance: hello_world 12M and a commercial boot 100M records strict-identical,
  ARM9+ARM7 interleaved, zero divergence** (armhf-interp on the pi5 vs a64-interp local).
- Two reference-config bugs found on the way: (1) the fs-clear-overlay gate compiled
  everything on NTR-sdk titles even at threshold 255 (now stands down there; const-folds
  away at production threshold — `rely_on_fs_invalidation()` has NO hle condition, memory
  of "HLE-only" was wrong); (2) the os irq handler substitution needs `--hle-irq 0` on
  both sides (HLE substitution exists only in compiled code).
- Trace size work (maintainer ask): delta-encoded records (~6.5x smaller, 12.5 B/record on
  a real boot; keyframe per 2^20/cpu; zeroed padding → identical runs give byte-identical
  files), `DSVITA_INST_LOG_TEXT=0` drops text records (commercial boot: text > inst bytes).
  All readers handle old+new format. Pull-speed benchmark: rsync -z / rsync+zstd / scp all
  ~equal on the LAN (the box-side read is the bottleneck, ~60 MB/s); an ssh|zstd pipe wins
  only ~10% — not adopted; the delta format is the real 6.5x.
- Rules (maintainer): never decode traces on the box — always pull the .ilog and analyze
  locally.

## Stage 4 — vixl aarch64: DONE, acceptance met
- Hardcoded instruction lists on BOTH arches (maintainer ask): the aarch32 build no longer
  parses the expanded macro assembler (clang-format+regex gone) — the frozen parse lives in
  `vixl/aarch32_masm_list.txt` and drives the same generation deterministically (sorted
  output verified semantically identical to the old parse). aarch64's explicit list is the
  hand-written shim layer itself: `vixl_src/src/aarch64/wrapper-aarch64.{h,cc}` (in the
  vixl fork submodule — **local commit 764e9f47, Grarak/vixl needs a push**), plain-
  primitive C functions constructing vixl operands internally; extend it by adding shims.
- Coverage v1: mov/movz/movk, ALU imm+shifted-reg (+flags variants), shifts, bitfields,
  csel/cset/csinc, ldr/str imm-offset all widths + regoff (fastmem `[xB, wA, uxtw]`),
  ldp/stp, literal-pool loads, b/b.cond/bl/cbz/cbnz/tbz/tbnz/br/blr/ret/adr, mrs/msr NZCV,
  nop/brk, raw nop, ExactAssemblyScope (pool blocking for fastmem/patch windows).
- **Acceptance: 7 execute tests assemble + mmap-execute on the dev box** (NZCV round-trip
  driving csel, ldp/stp pre/post-index, literal pool, branch loop, exact scope, regoff
  load). Pitfalls hit: A64 `GetBuffer()` returns a reference; pool literals must register
  with the masm's LiteralPool or FinalizeCode never places them; macro insts assert inside
  exact scopes (raw forms only); two upstream unqualified RawLiteral enum uses fail C++17
  lookup (patched in the fork).
- Gates: armhf + a64 dsvita build, vita vpk builds, armv7 vixl generation semantically
  identical.

## Stage 3 — backend seam refactor: DONE, acceptance met
- Pure moves + driver hoist, armv7-sacred: `emitter/*` → `emitter/arm32/` verbatim; the
  arm32 assembler files (arm/, thumb/, block_asm, reg_alloc, vixl glue) → `assembler/arm32/`
  with compatibility re-exports (zero call-site churn); the shared block driver in
  jit_asm.rs no longer assembles mnemonics through Deref — its four inline sequences are
  named BlockAsm seam methods (emit_validate_block_hash / emit_enter_block_hook /
  emit_entry_pc_dispatch / emit_set_guest_pc_const) a second backend provides; `HostReg`
  alias per D4 (arm32 = guest Reg; runtime-data register encodings deliberately unchanged —
  see todo/aarch64-width-audit.md §D). Width + wrapping audits delivered in that file.
- **Byte-identity gate PASSED: 283 + 2488 + 2041 blocks (hello_world / HG / Diamond -e 2,
  threshold 0) byte-identical pre-vs-post refactor, zero excluded blocks.** Gate =
  DSVITA_BLOCK_HASH_LOG stream at jit_insert_block + tools/armv7_gate.sh (capture/compare,
  two same-build baselines calibrate) + DSVITA_BLOCK_DUMP_PC companion.
- The gate itself took six iterations to make sound — all masking/tooling, no codegen
  issues found in the refactor: host fn pointers under qemu-PIE live INSIDE guest-value
  ranges (value masking impossible → mask by materialization pattern into scratch regs);
  T32 splits pointers across movw/movt; vixl picks movw OR a rotated mov for the low half
  depending on the VALUE (heap layout!) → matched pairs must be canonicalized, not just
  imm-masked; blanket value-masking corrupts instruction words; and two same-mtime-tick
  stale-build races (capture now touches the whole tree — same pitfall as DEVELOPMENT.md §4).
- Other gates: vita vpk builds; a64 release boots HG (~630 uncapped vblank-lines/s local);
  pi5 armhf release fps spot-check unchanged (HG uncapped 2080-2716 lines/s, same band as
  pre-refactor 2286-2900).

## Stages 5, 6
- Not started (S5 A64 backend on the vixl glue + seam; S6 jit-vs-jit tracediff loop).

## Env notes
- qemu HG: release ≈ 500 vblank-lines/s, release-debug+DEBUG_LOG ≈ 50/s.
- DEBUG_LOG=true with the plain release profile is an ILLEGAL combo (BRANCH_LOG paths call
  debug-only accessors that panic in release). Trace = release-debug profile only.
- ilogs are tens of GB — stream them, delete after.
