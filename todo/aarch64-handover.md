# aarch64 port — handover

One doc: current state, how the A64 jit works, what's pending, how to debug it, and the
lessons that cost real time. Supersedes the old plan/progress/session notes (git history
has them; the width/wrapping audit survives as `todo/aarch64-width-audit.md`).

## State (July 7 2026 night, branch `aarch64-port-s1`)

Stages 1-4 DONE. Stage 5 slices 1-8 DONE and gated (ALU, flags, branches+tail-calls,
bl/blx/bx+return-stack, thumb, HLE substitutions, register allocator, single-transfer
fastmem + SIGSEGV patcher). One shared compile_block; pool x19-x26+x28; ARM7 homebrew
hash validation; the ORIGINAL vixl aarch32 generation pipeline restored (cc-expand +
clang-format + Condition-regex, no list file); every DSVITA_* debug valve behind
IS_DEBUG (release builds fold them out — device bisects need release-debug builds).
ldm/stm lowering wired end-to-end but DEFAULTED OFF (irq-phase split — see pending).
Gates at HEAD: hello_world 3M strict, armv7-sacred byte-identical ×3 baselines, HG
jit-vs-jit determinism byte-identical, release smokes healthy.

Debug instruments (all IS_DEBUG): DSVITA_VBLANK_HASH=N[,M] (main/vram/palettes/oam +
2D-register-file xxh32 at vblank anchors), MEMHASH at the inst-log budget stop,
DSVITA_FRAME_DUMP=N[,M] (glReadPixels of the final framebuffer pre-swap → frame_N.rgba
+ GLINFO renderer/version line).

**Blocked on maintainer**: push the vixl fork submodule commits (`764e9f47` + `733af427`
+ `ca310352` + `736846d9`) to Grarak/vixl; disassembler-vs-NooDS cycle call; merge
decision.

## A64 visual corruption investigation (ACTIVE — narrowed to frozen engine-A tables)

Symptoms (maintainer): 2D glitches on a64 even with software GL; Black crash. Pi5 = the
visual box; pinned sav (~/hg_master.sav); pkill -x only (never -f — matches the ssh
cmdline); presented-frame pace on the pi ≈ 19fps (llvmpipe present is the wall — an
uncapped run reaches presented-frame 300 in ~15s while emulation races ahead).

**Established, in order:**
1. Guest emulation EXONERATED: vram/palettes/oam byte-identical armhf↔a64 at vblank 300
   AND 2700 (01005a6c/88d53f07/6e3fbd46, pi-sav).
2. Pixels (glReadPixels DSVITA_FRAME_DUMP): armhf = perfect HG title; a64 = 3D screen
   PIXEL-PERFECT, 2D screen broken (logo doubled, ghosting, garbage rows, missing 2D
   overlay). Deterministic across runs. Evidence: scratchpad frame_{a64,armhf}.png +
   frame_compare.png.
3. DRIVER EXONERATED (was the maintainer's theory — tested): GLINFO identical on both
   sides — `llvmpipe (LLVM 19.1.7, 128 bits)`, Mesa 25.0.7 (no V3D fallback).
4. **THE FINDING: a64's engine-A per-scanline UBO table is FROZEN.** Its hash is
   eab5662c at presented-frame 300 uncapped AND presented-frame 2700 capped — wildly
   different guest times, identical table — while armhf's varies with the scene
   (b2a113c9 at its scene). ubo_b matches across arches (constant/blank engine B).
   2D = GL shaders fed per-scanline tables (disp_cnts/bg_cnts/win_bg/bg/blend ubos in
   renderer_regs_2d_shared, double-buffered [1]→[0], swap under the `rendering` mutex in
   gpu_renderer::on_scanline_finish; cpu thread samples per scanline via on_scanline).
   Frozen table + live vram = exactly the doubled-logo/ghosting signature.

**Next: why does engine-A sampling never update on a64?** Candidates, in checking order:
- `sample_2d` stuck false (set from `sync_3d = !geometry_3d_skip()` in
  on_scanline_finish; init sets it how?) — then on_scanline never writes [1].
- The [0]↔[1] swap never runs: guarded by `if !*rendering && self.ready_2d` — if the
  render thread never clears `rendering` (or ready_2d never sets), [0] stays initial.
- on_scanline not called at all on a64 (the per-scanline event wiring).
Instrument sketch (was written, rejected mid-run — re-add if needed): dump
disp_cnts[0]/[96], bg_cnts[0..4], bg_ubo.ofs[0..4] alongside FRAMEDUMP — zeros ⇒ never
sampled; stale-but-nonzero ⇒ swap starvation. One uncapped release-debug run per side
(~40s each) answers it. Then reconcile with S1's clean title (same pi/driver — S1's
sampling worked; diff the sampling call-path S1..HEAD, suspects: the scanline-event
scheduling changes from the f25fd7e-era interpreter rework — f25fd7e black-stall fits
"tables never sampled" too).

Bisect worktrees: scratchpad/bisect (S1-era), bisect2 (free). Worktree checkouts DISCARD
instruments — re-verify before runs. simd-adler vendor swap + submodule fetch from the
main tree (recipes in git history of this section). Pi binaries: ~/dsvita_{a64dbg,hfdbg}
(release-debug, instruments live) + older tags; frame dumps land in ~ as frame_N.rgba.

## How the a64 jit works

Files: `src/jit/emitter/aarch64/{emit,emit_alu,emit_branch}.rs` (driver+valve, ALU,
branches) and `src/jit/assembler/aarch64/block_asm.rs` (A64BlockAsm), mirroring the
arm32 layout. The arm32 backend is untouched and remains the reference for every
lowering decision (stage 6 demands record-identical armhf-jit ↔ a64-jit).

### Dispatch and block lifecycle
- Cold pcs interpret; a per-address exec counter (shared with the interpreter's hotness
  scheme) crosses `INTERP_THRESHOLD` → `emit_code_block` decodes the block
  (`fill_jit_insts_buf`) and asks `is_block_jit_supported`. Refused blocks set counter
  255 = permanent tried-and-refused sentinel (re-deciding per handback was a measured
  20x boot slowdown; genuine saturation lands on the same value harmlessly) and keep
  interpreting. The interpreter covers the full instruction set, so the backend can
  refuse anything.
- `jit_insert_block_a64` copies code into the RWX jit region (page-aligned blocks),
  stamps `jit_memory_map` slots for the whole guest range, applies write-protect
  invalidation ALWAYS (no fs-clear hook exists on a64 — overlay reloads invalidate via
  the generic per-write path), stores per-block `A64BlockMeta`, and returns `flushed`;
  the driver exits the guest context after a flushing insert (host frames above may
  point into freed blocks — same rule as arm32).
- HLE substitutions (os irq handler, nitro-sdk patterns, TWL microcode) exist only in
  the arm32 emitter → on a64 those pcs interpret; `-e 2` stays refused at startup until
  the HLE slice.

### Block anatomy (runtime contract)
- A block is a normal AAPCS64 function taking the tagged guest pc in w0. Prologue: fp/lr
  frame + save caller's x27 + pin ThreadRegs in x27 (callee-saved → survives every
  runtime call). Guest regs live in memory at `[x27, #reg*4]`; every instruction body
  loads sources fresh and stores results back — no allocator yet (reg_alloc port is a
  later slice; pool will be x19-x26, 8 regs like arm32).
- **Entry-pc dispatch**: prologue compares w0 against the block's start pc; a mismatch
  (interrupt returns, hot mid-range branch targets — the map stamps the whole range)
  goes through `a64_jump_to_other_guest_pc`, which uses the per-page `A64BlockMeta`
  (per-inst host code offset + pre_cycle_count_sum) to jump to the exact instruction.
  Nothing else needs restoring on this backend — registers and flags are memory-resident.
  The block's own base address is materialized with `adr` against a label bound at
  offset 0 (blocks are page-aligned, so base == jit entry).
- **Flags**: guest NZCV lives in the stored cpsr word. Arithmetic S-ops emit the A64
  W-form S-instruction (bit-exact NZCV for 32-bit add/sub families) and merge host NZCV
  into the cpsr; logical S-ops take N/Z from the result and compute the A32 shifter
  carry-out explicitly (statically for rotated immediates — the rotation only survives
  in the raw opcode — and via pre-shift `ubfx` for immediate-amount register shifts).
  adc/sbc/rsc seed host C from the guest cpsr first. Conditional instructions load cpsr
  → `msr NZCV` → one inverted `b.cond` over the body; the per-inst DEBUG_LOG hook stays
  outside the skip (the interpreter logs condition-failed instructions too).
  Shift-by-register S-forms and RRX are refused (dedicated slice later).
- **Branches** (`emit_branch.rs`): a B with a target inside the block jumps between
  per-instruction labels; the taken path charges `counts[i]+2 − pre_cycle_count_sum`
  against `accumulated_cycles` (ldrh/strh on JitRuntimeData), checks the scheduler
  threshold, sets the target's pre_cycle_count_sum and jumps. The out-of-line exceed
  tail stores PC=target, calls `run_scheduler`, re-compares the guest PC and dispatches
  a pending interrupt via `handle_interrupt` (ARM9) or exits the guest context (ARM7,
  quantum up) — then rejoins the fast path (x8 must be reloaded there, see lessons).
  Forward local branches re-validate the block's entry slot against the adr-derived base
  first (arm32 parity; a scheduler excursion can invalidate the block). External
  branches store PC, flush through `pre_branch` (cycles+2, scheduler,
  pre_cycle_count_sum=0), then pop the block frame and **`br` to the target's entry
  slot** — a tail call: compiled block-to-block transfers never grow the host stack, and
  a cold target lands in `emit_code_block` as the tail callee. Blocks must END in an
  unconditional AL B — a fall-through exit would charge +2 and add an interrupt point
  where the interpreter's flat loop has neither (caught as an irq-timing trace split).
- **Cycle accounting contract**: at every taken-branch boundary both engines charge the
  cumulative cycles of the segment plus the +2 branch epilogue and observe
  `accumulated_cycles` against a threshold. The jit charges lump-at-boundary via
  cumulative `insts_cycle_counts[i] − pre_cycle_count_sum`; the interpreter spreads
  per-inst then checks at the branch — totals and check points are identical by
  construction. **Threshold fork**: arm32-jit checks local back-edges against
  `max_branch_loop_cycle_count` (128) while the interpreter checks every taken branch
  against `max_loop_cycle_count` (255 on ARM9, 128 on ARM7). Production a64 mirrors
  arm32 (stage-6 pair is armhf-jit ↔ a64-jit); `DSVITA_A64_INTERP_TIMING=1` selects the
  interpreter thresholds — **mandatory for jit-vs-interp strict gate runs**, ARM9-only
  difference.

### Valves and env switches
| env | effect |
|---|---|
| `DSVITA_A64_JIT=0` | kill switch, interpreter-only |
| `DSVITA_A64_INTERP_TIMING=1` | interpreter scheduler thresholds on local branches (gate runs) |
| `DSVITA_A64_DISABLE=c,l,a,i,b,v,t` | bisect classes: cond exec / logical-S / arith-S / carry-in / branch shapes / fwd validity check / thumb |
| `DSVITA_A64_MAX_BLOCKS=N` | compile only the first N support-passing blocks |
| `DSVITA_A64_SKIP_PC=hexpc,hexpc` | refuse specific block start pcs |
| `DSVITA_BLOCK_HASH_LOG=path` | per-insert `cpu pc thumb len hash` stream (also the armv7 byte-identity gate) |

### Design decisions still in force (from the original plan)
- Emitters are per-backend, not facaded; the shared driver talks through named BlockAsm
  seam methods. Guest `Reg` stays the vixl aarch32 enum; `HostReg` is a per-backend alias.
- Register pool stays 8 (x19-x26) when the allocator lands — everything couples to 8.
- All fixed guest regions are < 4 GB and the jit region should get a < 4 GB hint so
  pointer materialization stays short (movz/movk) when entry patching arrives.
- Q flag has no A64 home: DSP/Q ops become Rust-helper calls, no inline saturation.
- Remaining top risks (plan ranking): emit_transfer lowering + slow-mem patch scheme +
  cross-modifying icache discipline on the A76; vixl masm surprises around pools/veneers
  (use ExactAssemblyScope for patch/fastmem windows; execute-tests exist).
- Stage-6 note: arm32's analyzer-driven idle-loop detection changes timing; the a64
  backend will need the same analysis before armhf-jit ↔ a64-jit strict can pass.

## Pending work

1. **Visual corruption** — active, section above (finish the armhf half of the frame/UBO
   comparison, then the verdict tree).
2. **Full op coverage → delete is_block_jit_supported.** Singles DONE (fastmem).
   ldm/stm WIRED but OFF in is_multiple_transfer_supported: with it on, hello_world's
   strict pair splits at ~130k records on ARM7 irq phase — an interrupt dispatches one
   instruction earlier than the interpreter right after a compiled thumb pop (T-loss
   scan via trace_diff Stream: pop {r0} at 0x3802308 → vector 0x23800b0). Find the
   cycle-accounting detail (text-pair + scheduler-firing grep playbook); also refuse or
   no-op EMPTY rlists before re-enabling (the core debug_asserts non-empty).
   Then the rest, designs settled: PC-dst loads as terminals; ldrd/strd window
   extension; swp two-call; mrs/msr/cp15/swi via the interpreter helpers; mul family
   inline (N/Z only); shift-by-reg S-forms + RRX with csel carry rules; undefined →
   inst_undefined. Then delete the valve + REFUSED sentinel + the arm32-only
   fall-through cfg island, and run stage 6.
3. **Stage 6**: armhf-jit ↔ a64-jit strict pairs at threshold 0 — the definitive value
   gate (jit-vs-interp died with fastmem write-breakouts by design; methodology section
   below). Perf floor: a64-jit ≥ armhf-jit on the pi5.
4. **NSMB strict divergence (parked, possibly moot under stage 6)** — DSVITA_A64_SKIP_PC
   =2067318 alone makes 12M pass; scheduler firings phase-shift one iteration after that
   block's first execution; charge algebra provably equal. If revived: print
   (pc, accumulated_cycles) at the branch arm + block chain, one text pair, hand-count.

### Register pool (maintainer ruling July 7)
All callee-saved registers allocate: x19-x26 + x28 (GUEST_REG_POOL_SIZE 9 on a64, 8 on
arm32 — per-arch const). x27 stays the pinned ThreadRegs base: the spill/restore fabric
addresses guest slots through it on every access. Block frames save fp/lr + x27/x28 +
x19-x26 (112 bytes); the breakout shim dumps 9 (x28 at dump[8], host_pool_index maps
the x27 gap).

### ARM7 homebrew hash validation (arm32 parity)
ARM7 blocks outside VRAM with no nitro-sdk detected rehash their guest code (xxh32) at
every fresh entry — homebrew self-modifies ARM7 code without any write-protect contract.
Mismatch → a64_validate_guest_block_hash recompiles and the emitted check tail-jumps
into the new entry (x19 parks the tagged entry pc across the call; allocator holds
nothing that early).

### Gate methodology after fastmem
Strict jit-vs-interp pairs die by design once write fastmem is live: a write into a
jit-protected page breakouts (extra run_scheduler observation point the interpreter's
flat loop doesn't have) → the cpu interleave shifts (HG/Kart split at the ARM7 boot copy
loop ~235K records, no value divergence before the shift). hello_world still passes
(no protected writes). arm32-jit has the same property — the definitive value gate is
stage 6 (armhf-jit ↔ a64-jit strict pairs, both breakout identically), which needs full
op coverage first (coverage gaps interpret, and the interpreter's scheduler thresholds
differ at those pcs). Until then: hello_world strict + release smokes + jit-vs-jit
determinism pairs (same binary twice, byte-identical — budget the capture time, a 6M
HG debug capture needs >5 min per side).

## Debugging playbook

Preferred reference (maintainer steer): **armhf-jit ↔ a64-jit strict pairs** — trace
against the known-working arm32 implementation whenever the a64 op coverage allows
(tools/tracediff.sh is the cross-box harness). The same-box interp pair below remains
the fallback for op classes arm32 compiles but a64 doesn't yet: reference =
`DSVITA_A64_JIT=0`, candidate = `DSVITA_A64_INTERP_TIMING=1`, otherwise identical.

- **Builds**: `cargo build` (the dev profile IS the trace build since the opt-level
  bump — DEBUG_LOG keys off the profile name, binary at `target/debug/dsvita`). Plain
  release + DEBUG_LOG remains an illegal combo. Always confirm the `Compiling dsvita`
  line — sed/touch+build chains can race the mtime fingerprint into a stale binary.
- **Capture**: `--inst-log X.ilog --hle-irq 0 -f 0`, `DSVITA_INST_LOG_MAX=N` (logger
  closes itself on a record boundary; SIGINT tears records), `DSVITA_INST_LOG_TEXT=0`
  for pure record streams / `=1` when you want the debug text records (branch prints,
  `flush cycles`, `run scheduler at` — they interleave into the ilog). The emulator
  keeps running after the logger closes: watch the ilog size and kill on stability.
  **Never capture stdout of a DEBUG_LOG build** (per-inst flood, tens of GB in minutes);
  `>/dev/null 2>err.log`. Copy the rom's .sav aside and restore it between the two runs.
- **Compare, coarse→fine**:
  1. `cmp a.ilog b.ilog` — identical runs give byte-identical files (delta format).
  2. `tools/trace_diff.py a b --strict` — first divergent record, with context.
  3. `tools/block_diff.py a b <cpu>` — per-cpu BLOCK-ENTRY streams (pc discontinuities),
     engine-neutral; finds the first control-flow-level difference and, on same-pc
     streams, inst-index skew. Use before staring at records.
  4. `trace_diff.py --cpu 0|1` (resync mode) — per-cpu content comparison; "0 divergences
     + skews" means pure interleave shift, register tallies show what data moved.
  5. Text-stream forensics: `dsvita decode-inst-log X.ilog | grep -a "flush cycles"`
     (accumulated at every runtime flush — jit prints at chains, interp only at
     branch_reg/branch_lr, so diff VALUES at common lines, treat jit-only lines as
     insertions) and `grep -a "run scheduler at\|handle interrupt at"` (engine-common
     firing stream — its first diff localizes a timing seed exactly).
  6. Bisect: `DSVITA_A64_DISABLE` classes → `DSVITA_A64_MAX_BLOCKS` halving (compile
     order = `DSVITA_BLOCK_HASH_LOG` line order) → `DSVITA_A64_SKIP_PC` single blocks.
- **Reading blocks**: hash-log names every compiled block; get a block's guest code from
  a trace via the record opcodes (`trace_diff.read_inst`/`Stream`) or
  `decode-inst-log | grep "PC: <hex>"`. `DSVITA_BLOCK_DUMP_PC=hexpc` dumps emitted bytes.
- **Hygiene**: kill stray emulator/qemu processes between timing-sensitive runs
  (`pkill -f dsvita`); traces are tens of GB — stream, delete after; never decode on the
  pi, pull first.

## Lessons learned (paid for in hours)

- **Reload pointers after calls at emitted rejoin points.** The scheduler tail rejoined
  the fast path with x8 (runtime-data pointer) clobbered by run_scheduler /
  handle_interrupt → wild strh → heap corruption crashing later in malloc. arm32 reloads
  r0 at exactly those joins; every future out-of-line tail must audit registers live
  across its calls.
- **Boundary parity beats instruction parity.** Both engines must charge and OBSERVE
  cycles at the same guest boundaries: blocks end in uncond AL B; external exits flush
  +2 then check; the 128-vs-255 threshold fork is real and env-selected. A hidden
  accounting drift can stay invisible for millions of records and only bite when a
  threshold crossing lands on a different side of an irq raise (NSMB).
- **Byte-reproducible traces are the sharpest gate.** The delta ilog zeroes padding, so
  `cmp` alone proves strict identity — use it before the differ.
- **Block-level first, records second** (maintainer steer that cracked NSMB's
  localization): reduce to block-entry/firing streams, find the first structural
  difference, then zoom. Record-level diffs point at the symptom, not the seed.
- **The interpreter is the semantic reference for gates, arm32 for lowering.** When they
  disagree (thresholds, spurious validity exits), production follows arm32 and the gate
  gets a parity valve — the stage-6 pair is jit-vs-jit.
- **Refused ops keep everything honest**: any block containing an unsupported op
  interprets whole; stores don't compile yet, which also means compiled blocks can't
  invalidate themselves mid-run.
- **Panic hygiene matters for debugging**: the RA request thread held a raw pointer into
  actual_main's stack; any panic became a use-after-free abort that buried the real
  error (cost: a detour chasing a "crash" that was a wrong rom path). Fixed with a
  join-on-drop guard; the lesson — background threads must be joined on unwind paths, or
  every panic turns into a red herring.
- **vixl a64 pitfalls** (from S4, still relevant): `GetBuffer()` returns a reference;
  pool literals must register with the masm's LiteralPool or FinalizeCode never places
  them; macro-instructions assert inside exact scopes (raw forms only); `adr` against a
  label bound at offset 0 self-identifies a page-aligned block.
- **qemu-PIE + masking lessons** for byte-identity gates live in
  `todo/aarch64-width-audit.md` §D and the armv7_gate tooling: mask by materialization
  pattern, never blanket-value-mask, canonicalize movw-vs-rotated-mov pairs.
