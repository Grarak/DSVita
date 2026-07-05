# Development Notes

Lessons, invariants, and debugging strategies accumulated while building the cold-block
interpreter and hunting compatibility bugs (boot hangs, save-resume corruption, interrupt
starvation). Written for anyone continuing this work — human or AI session. File/line
references may drift; module-level statements are stable.

Reference implementation for guest semantics: **NooDS** (`interpreter_*.cpp`) — a complete,
known-good interpreter. When this repo's JIT and NooDS disagree on a quirk, the interpreter
must match **this repo's JIT** (the two engines must be trace-identical); the deviations are
listed below.

---

## 1. Core invariants (violating any of these caused a real bug)

### Execution model
- Both guest CPUs run on ONE host thread. The ARM9 runs until its cycle budget
  (`max_loop_cycle_count`: 255 ARM9 / 128 ARM7) trips a scheduler check at a **taken branch**;
  `run_scheduler` then runs cycle-manager events and gives the ARM7 a slice. There is no
  preemption between branches — anything that must interrupt execution either waits for the
  next branch-point check or uses the `breakout_imm` mechanism (set by gx-fifo fill /
  renderer sync, checked after stores).
- **Immediate events (`schedule_imm`) only fire when the scheduler actually runs.** A pending
  interrupt is dispatched by an imm event; if the guest controls when the scheduler runs
  (fixed-cycle loops), the event can be systematically starved (see case study §5.3).

### Guest state conventions
- **`regs.pc` and every branch target carry the thumb bit in bit 0** at all
  handback/scheduler/exception boundaries. `call_jit_fun` consumes it; the HLE BIOS irq entry
  reads `pc & 1` for the SPSR T bit. A thumb dispatch loop must write `addr | 1` on
  breakout/fallback.
- **CPSR cross-block contract**: blocks are emitted assuming guest CPSR memory is clean at
  entry; keeping flags in a host register across block edges is unsound.
- Guest regs map to host R4–R11 contiguously; `GUEST_REGS_PTR_REG = R3`, `CPSR_TMP_REG = R0`
  — R0–R3/R12/LR are scratch, so a helper call emitted mid-block clobbers them. (A hand-rolled
  emitter change that parked a value in R0 across a call faulted instantly; even R8 parking
  conflicted with block-internal allocation. Don't insert calls into emitted blocks without
  understanding the allocator's live state at that point.)
- Host code is emitted in the guest's mode (ARM guest block → ARM host code, thumb → thumb);
  entry pointers carry the thumb bit. This is also why frame-pointer profiling is impossible
  on this target (r7/r11 fp split).
- `exit_guest_context` long-jumps out of arbitrarily deep guest call chains by restoring the
  saved host sp — it abandons Rust frames. Nothing on that path may own anything with a `Drop`.

### Cycle accounting (interpreter must mirror the JIT exactly)
- Per-inst cycles = the disassembler's `InstInfo::cycle`, prefix-summed per block.
  Condition-failed ARM instructions still charge their full static cycle (the JIT's per-block
  prefix sums include skipped instructions).
- Every taken branch adds **+2** (the JIT's `emit_count_cycles` epilogue: `total + 2 -
  pre_sum`) — but NOT on fallback handback (the compiled block adds its own +2) and NOT on
  `breakout_imm` (the JIT's breakout flush has no +2 either).
- **Scheduler checks happen only at taken branches.** Adding a mid-straight-line budget check
  to "improve responsiveness" skews irq timing between the engines and breaks trace parity.

### Address arithmetic policy
- release-debug builds run with overflow-checks + debug-assertions ON. Computed guest
  addresses use plain `+`/`-` so a genuine guest bug panics loudly — but **negative register
  offsets are routine guest behavior** (`ldrh r1, [r2, r1]` with r1 = −4 is legal). Load/store
  *address* computations must use wrapping arithmetic; only treat *data* overflow as suspect.
  A "plain + catches guest bugs" convention applied to addresses produced false-positive
  panics in real games.
- Corollary: a bug that asserts on release-debug **silently corrupts** on release. "Game gets
  stuck on hardware" is often an assert-class bug — reproduce on release-debug first.
- Audit `assert_unchecked` bounds for off-by-one at the boundary: `start + LEN < buf.len()`
  rejects the last valid slot when `start + LEN` is an exclusive slice end (must be `<=`).
  This aborted long game sessions only after a cache filled — invisible in short tests.

---

## 2. Interpreter design rules (deliberate; weaker designs were rejected)

- **Flat literal fn-pointer tables**, exactly like the disassembler's own lookup tables:
  `ARM_TABLE: [ArmInterpFn; 4096]` indexed `((op>>16)&0xFF0)|((op>>4)&0xF)`, `THUMB_TABLE:
  [ThumbInterpFn; 1024]` indexed `op >> 6`. Every entry is a fully specialized handler built
  with const generics + `paste` (mirroring `disassembler/delegations.rs`). **Zero matching on
  the execution path** — no `match op`, no in-handler addressing/shift classification.
- The tables are **generated from the disassembler's lookup tables**
  (`tools/gen_thumb_table.py`; same approach for ARM) so JIT-decode and interpreter-decode
  can never disagree. Regenerate after adding handlers; unknown names → `inst_fallback`.
- ARM condition check: literal `CONDITION[((op>>24)&0xF0)|(cpsr>>28)]` table (0=skip, 1=run,
  2=reserved→fallback). Unconditional fast path: `opcode >= 0xE0000000` skips the cpsr load.
- **Handback model**: interpret straight-line until a branch/pc-write, then hand to the next
  guest address's jit entry ("one block then call next entry"). Anything unimplemented →
  `inst_fallback` → that block compiles. Correctness holds at ANY coverage level.
- **Return-stack parity**: `bl`-shapes route through the JIT's own `branch_reg` (push +
  native call + resume interpreting the tail) and `bx lr`-shapes through `branch_lr`
  (pop-compare + native return). Without this, every interpreted call/return produced a
  return-stack mismatch → `exit_guest_context` → quantum reset — a massive scheduler-timing
  divergence vs the JIT.
- **Flat loop for cold branch targets**: a taken branch to another cold same-mode block
  continues inside the dispatch loop instead of recursing a host frame per block (compiled
  branches tail-call; naive recursion overflowed the host stack in interpreted spin loops —
  and the SIGSEGV was eaten by the fastmem fault handler, appearing as a silent death).
  A stack-depth guard covers the remaining forward-call chains.
- **Hotness counters live in `jit_memory`**, one u8 per halfword of executable memory, laid
  out per region exactly like the jit entries with the same mirror collapse. Direct-mapped
  per-cpu arrays keyed on raw pc were rejected: they count the same physical code separately
  per mirror address and duplicate storage. A null counter pointer = non-executable region
  (e.g. HLE BIOS trampoline addresses) — such targets must reach their special jit entries,
  never the interpreter.
- Semantics quirks aligned to this JIT (deviating from NooDS where they differ): word loads
  rotate misaligned reads; halfword loads do NOT model the ARM7 misaligned-rotate quirk;
  thumb `stmia` models the ARM7 "written-back base is stored unless lowest listed reg" quirk;
  `NEG` sets V. (The ARM-side `ldm_stm` does not model the stm quirk — known divergence
  suspect if an ARM7-heavy title misbehaves.)
- Block transfers gather/scatter through a stack buffer with a single multiple-slice memory
  request (one region/mmu resolve per transfer, mirroring the JIT's slow path). The register
  count stays runtime — rlist bits aren't in the table index, so it can't be const.
- `breakout_imm` is checked **only after stores** (only stores can set it) — matching the
  JIT's write handlers and avoiding a per-inst load+branch.

## 3. Blocks that must NEVER be interpreted

The JIT performs compile-time pattern substitutions that carry **semantics**, not just speed.
Interpreting these pcs skips the substitution:

1. **The os irq handler** — replaced by an HLE version at compile time. Interpreting the real
   handler bypasses the replacement.
2. **`FSi_ClearOverlayImage`** (NTR-sdk titles under HLE arm7) — its compiled hook is the
   SOLE overlay-reload jit invalidator in that mode (`jit_insert_block` deliberately skips
   ARM9 main live ranges there, so per-write invalidation does not cover overlay reloads).
   While the reliance condition holds and the function hasn't been located yet, the
   interpreter stays fully off so the compile-time detection can find it; once found, only
   that pc keeps force-compiling. Detection must remain compile-time-only: running the
   detection from the dispatch gate crawled boot to 3% (r0 legitimately points at valid
   overlay headers throughout FS init).
3. **The TWL cpu-sync microcode window (0x1FF8xxx, ARM9 + HLE arm7 + TWL sdk)** — TWL titles
   copy shakehand/wait-agreement microcode there; only the JIT's pattern substitution can
   answer it (the real code spins on an ARM7 reply that HLE never sends). Additional trap:
   the spin loop's hotness counter belongs to a MID-PATTERN pc, so even after the threshold
   the block compiles from the middle of the microcode, the pattern match fails, and it
   compiles as a real spin loop — interpret-then-compile does NOT self-heal here.

Rule of thumb: HLE substitutions of *accelerator* functions (cpu copy/fill, gx fifo sends)
are safe to interpret first — the real code is functionally equivalent. Substitutions that
replace code which *cannot work* under HLE (microcode handshakes, the irq dispatcher) and
hooks with *side effects* (overlay invalidation) must bypass the interpreter. When adding a
new substitution, decide which class it is in.

---

## 4. Debugging playbook

Ordered by cost. Every technique below cracked at least one real bug.

1. **Read the panic.** release-debug has overflow-checks and debug-asserts; the panic hook
   prints a full backtrace plus the last interpreted instruction per cpu with all guest regs,
   and the memory bounds asserts print the offending address.
2. **A/B the engine.** Flip `INTERP_THRESHOLD` (0 = pure jit, 255 = always interpret, 100 =
   production) and rebuild. Pure jit reproducing it rules the interpreter out.
3. **A/B a suspect feature** with a temporary `const DISABLE_X: bool` knob. Minutes to
   exonerate a subsystem.
4. **Count events in the debug log before tracing instructions.** With `DEBUG_LOG = true`,
   grep-counting `send interrupt` / `interrupt {` / `can't interrupt` / `hle ipc send` lines
   over a time window characterizes a hang instantly: irqs flowing but no progress = game
   loop alive (look elsewhere); requests sent but zero dispatches = starvation; no irq
   traffic at all = spinning before irq setup (very early boot). This is how both a "GPU
   never starts" hang and an interrupt-starvation hang were localized in minutes each.
5. **Instruction-trace diff** (the heavy hammer). `--inst-log <path>` records from boot;
   `--inst-log-lazy <path>` arms on SIGUSR2 (capture the final stretch); SIGINT and the panic
   hook flush. Decode with `dsvita decode-inst-log <path>`. Diff two engines (threshold 0 vs
   100/255 builds) or two arm7-emulation modes.
   - **Parity caveats (each one cost real time):** neither engine logs taken branches, but
     the jit logs extra records the interpreter doesn't — "enter block" markers, records
     re-emitted on return-stack resume, same-block `bl`s. Filter to per-cpu `Executed`
     records and drop records whose InstInfo writes PC before comparing.
   - First divergence with same PC but different registers = real bug. Different PC sequence
     with same registers = usually benign timing skew; io-poll loops legitimately differ in
     iteration count between engines.
   - **Boot with no input is deterministic** — full-boot traces diff cleanly. Traces that
     start from an interactive point are polluted by input-frame differences.
   - `tools/trace_diff.py` diffs the binary logs directly (no decode) and resyncs across
     HLE gaps. Its three resync pitfalls are documented in the header; the important one:
     pick the resync with the SMALLEST total skip, and include offset 0 in the common-pc
     search, or it aligns far-future visits and reports bogus divergences.
   - Interleaved `memory read/write at X with value Y` text records show io handshakes,
     overlay/file loads, and callback-pointer provenance for free.
6. **Static disassembly of guest code.** armhf binutils via qemu:
   `qemu-arm -L <sysroot> .../arm-linux-gnueabihf-objdump -D -b binary -m armv5te
   [-M force-thumb] --adjust-vma=<ram_addr> <bin>`. Extract the arm9 binary from a .nds via
   header fields at 0x20 (rom offset, entry, ram addr, size); overlays via the overlay table
   (0x50) + FAT (0x48), 32-byte entries. Searching raw code bytes in the rom identifies which
   overlay/file some memory content came from.
7. **Use a third reference to break interp-vs-jit ambiguity.** An interp-vs-jit diff cannot
   distinguish "interp = hardware, jit's HLE ≠ hardware" (benign) from "interp ≠ hardware"
   (the bug) — both look identical. Compare against NooDS at the divergence pcs. When a
   memory corruption needs a known-good write sequence, **instrument NooDS's `Memory::write`
   with an address watch** and capture the reference stream — this pinpointed an
   event-ordering bug that pure trace diffing could not.

### Environment / tooling pitfalls

- **`sed -i file && cargo build` chained in one command can produce a STALE binary**: the
  edit lands in the same mtime tick as the previous build's fingerprint and cargo skips the
  recompile. This faked an A/B result and sent a whole investigation the wrong way. Run edits
  and builds as separate commands, watch for the "Compiling" line, and `md5sum` A/B binaries
  to prove they differ.
- **DEBUG_LOG stdout is huge** (GBs in minutes). Never point it at a small tmpfs: a full
  /tmp made the perf-map writer panic with StorageFull and the cpu thread died while the UI
  kept rendering — indistinguishable from an emulation hang. Redirect to a real disk, or
  pipe through `grep --line-buffered` and keep only the lines you need.
- **`pkill -f` / `pgrep -f` over ssh match the remote shell's own command line** (it contains
  the binary name) and kill your own session. Use exact-name matching (`pkill -x`) and name
  deployed test binaries distinctly.
- Test binaries built from different configs must be checksummed, not trusted by filename.
- On the pi test box: Wayland — screenshot with `grim` (scrot sees black), key injection with
  `wtype`; always run with a framelimit (`-f 1` interactive, `-f 5`/`-f 9` to fast-forward
  boot); the V3D driver has a known cosmetic glyph/tile rendering offset — verify a suspected
  rendering bug against a pure-jit build and another renderer before blaming emulation.
  Under qemu+Xwayland, mouse injection works but keyboard does not reach SDL.

---

## 5. Case studies (root causes worth remembering)

### 5.1 Stale JIT blocks across overlay reload
An NTR-sdk title crashed/hung on save-resume, interpreter-only, deterministic. The causal
chain ran backwards from a wild read through a stale callback pointer into overlay code that
flowed off its end into data. Root cause: `FSi_ClearOverlayImage` ran interpreted, skipping
the compiled hook that is the sole overlay jit-invalidator in HLE mode → the jit cache kept
compiled blocks of the *previous* overlay at the same base while guest memory correctly held
the new one. Guest memory was right; the code cache was stale. Lesson: **when an
invalidation hook exists only in compiled code, interpretation is a correctness hazard, not
a perf choice** (§3). Also: two overlays deliberately sharing a base address with different
sizes is normal SDK practice — "identical memory content, different executed code" means
code-cache staleness, not memory corruption.

### 5.2 HLE side-effect divergence poisons trace diffs
The same hunt was nearly derailed: the first "divergence" (a +0x28 heap shift) was the HLE
cpu-copy function not replicating the real function's r1 post-increment. The interpreter
matched hardware; the JIT's HLE didn't; the game tolerates both. Real bugs hide among such
benign divergences — hence playbook §4.7 (third reference). If HLE functions ever get
side-effect-exact (r1/r12/flags), interp-vs-jit diffs become directly trustworthy again.

### 5.3 Interrupt starvation by quantum resonance
A title hung forever on a black screen at boot, cpu busy. Event counting (§4.4) showed
thousands of gx-fifo irq requests, zero dispatches, and every dispatch attempt logging
"can't interrupt" with irqs disabled. The guest's send loop toggles cpsr.I with a fixed
per-iteration cycle count; the scheduler quantum expiry **phase-locked into the
irq-disabled window**, so the imm event that dispatches interrupts always found I set and
the pending irq starved — while hardware takes the irq the moment I clears. Fix: on an
irq-enable that finds a request pending, saturate the cpu's cycle budget so the existing
taken-branch scheduler check fires at the next branch (coherent pc, ≤1 quantum of skew).
Lesson: **fixed-cadence guest loops can resonate with any fixed scheduling quantum**; when
an event "never fires", check whether the guest controls the phase at which the event
processor runs. Also: prefer routing fixes through the existing branch-point checks over
adding new mid-block breakout paths — a first attempt that emitted a breakout call inside
`msr` blocks faulted on register-allocation contract violations (§1) and was abandoned for
the quantum-saturation design, which needed no emitter changes at all.

### 5.4 TWL microcode spin (interpret-then-compile does not self-heal)
Described in §3.3. The subtle part: the hotness threshold normally guarantees "interpreted
now, compiled soon, substitution eventually" — but a loop whose back-edge target is
mid-pattern compiles from the wrong start pc and the pattern match never fires. Any
compile-time pattern substitution is only reachable from the pattern's FIRST pc.

### 5.5 Diagnosing a black screen: no-irq spin vs starved irq vs dead thread
Three distinct black screens seen, distinguishable in minutes with §4.4: (a) zero irq
traffic after boot lines = spinning before irq setup (the TWL shakehand); (b) irq requests
without dispatches = starvation (5.3); (c) healthy irq traffic but a dead render = look at
the frontend/GL or a crashed helper thread (the StorageFull panic killed the cpu thread
while the UI kept running).

---

## 6. Performance lessons — measured on hardware, do NOT retry

All tried and reverted for zero or negative gain:

1. Back-edge cycle-accounting merge (paired u16 ldr/str).
2. Inlining `pre_branch`.
3. SPU sample batching, both variants (one broke frame pacing — the 1024-cycle SpuSample
   event cadence IS the frame limiter; do not batch it).
4. NEON-vectorizing scalar hot-loop math — much slower: Cortex-A9 punishes NEON↔integer
   register transfers in scalar loops; the crossing stall dominates.
5. Bulk struct copy + field patch in the vertex path — slower.
6. perf frame-pointer callgraph infrastructure — fundamentally unsound on this target
   (r7-thumb/r11-arm fp split; JIT emits host code in guest mode). Flat profiles work via
   the perf map (`/tmp/perf-<pid>.map`, `ARM9_<guest_pc>` symbols); callgraphs need dwarf.

Meta-lesson: the geometry/JIT/SPU core is tuned out; instruction-count reasoning has
repeatedly been wrong about it. Don't micro-optimize the core without a profile showing the
target dominating. The interpreter (skipping compilation of code executed < threshold times)
is the one structural perf avenue that survived.

Interpreter-specific perf that DID land: sequential opcode fetch (resolve the code's shm
offset once per straight-line run, re-resolve on 4KB page cross or jump), cached ThreadRegs
pointer in the dispatch context, the flat loop for cold branch targets, the AL-condition
fast path, batched ldm/stm, store-only breakout checks.

---

## 7. Known open items / divergence suspects

- ARM-side interpreter `ldm_stm` doesn't model the ARM7 stm-base-writeback quirk (the JIT
  and thumb `stmia_t` do) — suspect if an ARM7-heavy title misbehaves interpreter-only.
- HLE cpu-copy/fill functions don't replicate the real functions' r1/r12/flags side effects
  — benign vs hardware-tolerant games, but pollutes trace diffs (5.2).
- WRAMCNT=3 leaves the ARM9 shared-wram slow path on a `usize::MAX` sentinel: bounds assert
  on release-debug, wild read on release (fastmem survives via mmu entry 0). Consider
  open-bus 0 like NooDS.
- One title still shows an HLE-mode heap-ordering divergence on boot (a list push lands
  after a destroy-fill instead of before → later free walks poisoned memory). The LLE run
  matches a NooDS write-sequence reference exactly, so the skew is HLE event-timing.
  Reference technique: §4.7 NooDS write-watch.
- `cpu_send_interrupt`'s enabled-arrival path could theoretically starve like 5.3 (no known
  repro; the fix pattern is the same quantum saturation).
- Interpreter coverage gaps that fall back to the JIT (fine for correctness): SWI, MCR/MRC,
  LDRD/STRD, SWP, DSP muls, cond=0xF space, user-banked ldm/stm, empty-rlist thumb block
  transfers.
