# aarch64-linux host support for DSVita

## Context

DSVita supports exactly two armv7 hosts (Vita, Linux armhf). Goal: add `aarch64-unknown-linux-gnu` as a third host — first as an interpreter-only build (the agreed first step), then with a full A64 JIT backend. The pi5 runs both armhf and aarch64 builds natively side by side, making it the tracediff box. Hard constraint throughout: **armv7 JIT, NEON kernels and the Vita build stay byte/perf-identical** — every stage carries an explicit armv7 no-change gate.

Structured around five subtasks; practical order differs from the numbering because tracediff is the tool the backend gets debugged with, and the pointer-width work splits into a boot-blocking wave and a codegen wave.

**Sequencing: S1 → S2 → (S3 ∥ S4) → S5 (S6 = its debug loop)**
Mapping: subtask 1 = S1 · subtask 5 = S2+S6 · subtask 3 = S1-waveA + S3-waveB · subtask 4 = S3 · subtask 2 = S4 · JIT port itself = S5.

## Global design decisions

- **D1 cfg axis**: introduce `target_arch = "arm"/"aarch64"` gates. Audit says all 48 existing `target_os` gates are genuinely OS-meaning (mmap/presenter/perf-map/trace) and stay; the work is adding arch gates to today-ungated arm32 asm/NEON that only compiles because linux⇒armv7 today.
- **D2 backend selection**: compile-time module swap (`#[cfg(target_arch)] #[path]`), same pattern as `src/mmap/mod.rs` / `src/presenter/mod.rs`. One backend per binary. Vita = `target_arch="arm"`, covered automatically.
- **D3 emitters are per-backend, not facaded**: the ~117 masm methods the emitters use ARE the ARM32 ISA — a semantic facade would just rename it and force arm32 shapes onto A64. Cut: shared *driver* code in `jit_asm.rs` stops Deref-calling mnemonics (hoist ~10 named BlockAsm methods); `emitter/*.rs` moves verbatim to `emitter/arm32/`; A64 emitter written fresh against the same seam. `Deref<Target=MacroAssembler>` becomes private to the arm32 backend.
- **D4 register types**: guest `Reg` stays vixl aarch32 `Reg` (it models the guest file — correct on any host). New per-backend `HostReg` alias (arm32: `= Reg`, zero change; a64: X-reg enum). Width-coupled encodings switch from raw host-reg numbers to pool indices 0..7 (`InstMemMultipleParams.op0` is u4; `breakout_after_write`'s `mapped_reg - 4`).
- **D5 pool stays 8 regs on a64 v1** (x19–x26) — everything couples to 8 (`mapping[8]`, fault-stack `[usize;8]`, `jump_to_other_guest_pc` stride, u4 fields). Widening to 10+ is a later pure-a64 optimization.
- **D6 keep all fixed addresses < 4 GB on a64** (all six already are: 0x70/0x71/0x80/0x90/0xA0/0xA1 000000) and mmap the 32 MB JIT region at a fixed <4 GB hint too (e.g. 0x20000000, today unhinted). Payoff: host-pointer materialization and entry patching stay 2-instruction movz/movk and the "bake pointer as immediate" scheme survives with a wider encoder.
- **D7 a64 runtime contract** (twin of the arm32 one): x27 = ThreadRegs ptr (callee-saved → kills the `restore_guest_regs_ptr` reload dance); x19–x26 guest pool; x0 scratch/arg/ret; x30 return; x16/x17 = vixl scratch + patcher. Guest NZCV lives in host NZCV (`mrs/msr NZCV`, same bit positions as `cpsr_f`); canonical store stays the cpsr byte `[x27, #CPSR*4+3]`. **A64 NZCV has no Q bit** — Q-setting DSP ops must compute saturation explicitly. `call_jit_entry` twin: stp x19..x30 pairs, `str sp,[x2]`, `blr`; `exit_guest_context!`/`hle_bios_uninterrupt` frames must match exactly. ldm/stm → ldp/stp; fastmem loads `ldr wD,[xBase,wAddr,uxtw]`; slow-mem stubs budget 4-inst movz/movk chains (new `SLOW_MEM_*_A64` consts). aarch64 `mcontext_t` = `.regs[31]/.sp/.pc`.
- **D8 stage-1 execution model — interpreter-only, nothing ever compiles** (the agreed first step): a64 `DEFAULT_JIT_ENTRY` → `interp_jit_entry(u32)`, no counters/thresholds. Forced-JIT patterns: HLE irq-handler blocks → just interpreted (correct, slower); fs-clear-overlay hazard can't exist when nothing compiles; TWL microcode HLE lives only in emitted code → **`-e 2` refused on aarch64 with a startup error until S5**; stage-1 scope `-e 0`/`-e 1`.
- **D9 NEON**: per-file `use core::arch::{arm|aarch64}::*` alias where 1:1 (most of the 8 SIMD files); small cfg shims for arm-only intrinsics (vtbl→vqtbl, tuple loads, lane mulls); scalar only where SIMD is pointless. math-neon C lib excluded on a64; the `screen_layouts.rs` calls route through new `math::matmul3/matvec3` wrappers (armv7 wrapper = identical inline call; a64 scalar — cold UI path).
- **D10 tracediff**: `InstLogRecord` already width-stable (80 B both arches — verified); `tools/trace_diff.py` + inst-trace skill exist. New capability = same-box same-engine strict diff: interp-vs-interp (and later jit-vs-jit) must match record-for-record with zero resync.

## Stage 1 — subtask 1: compiles AND boots interpreter-only

**Step 0 (half-day de-risk)**: throwaway pi5 program mmapping all six fixed regions + 32 MB at 0x20000000 and exercising a SIGSEGV fixup. Proves D6 + the mcontext accessors before any porting. (Risk 4 is ruinous to discover late.)

1a. Build plumbing: `.cargo/config.toml` add a64 target (linker only, no thumb rustflags); duplicate `Cargo.toml` armv7-triple dep block (clap/sdl2/affinity/backtrace/reqwest) for a64; `vitabuild` `COMMON_C_FLAGS` arch-conditional (cortex-a9/thumb flags = arm only) and `create_bindgen_builder` uses the a64 triple when targeting a64 (keeps armv7 bindings byte-identical, makes soundtouch/imgui layouts 64-bit-correct); `build.rs` gates math-neon to arm; `vixl/build.rs` gates the aarch32 C++/bindgen to arm so the crate still exports the pure-Rust `Reg/RegReserve/Cond` types (decode/analyzer/interpreter need them).
1b. Arch-gate the arm32 pipeline out of the a64 build (compile-out only, no moves yet): `emitter/`, `assembler/{block_asm,reg_alloc,arm,thumb}`, naked fns in `jit_asm.rs`, the ~14 naked wrappers in `inst_mem_handler.rs` (portable handler bodies stay — the interpreter uses them), patching half of `jit_memory.rs`, arm-coupled `const_assert`s.
1c. Host context-switch twins (the interpreter runs *inside* guest context): a64 `call_jit_entry`, `exit_guest_context!` arm, `hle_bios_uninterrupt` return — frame-matched per D7. (`get_sp_depth_size`'s `mov {},sp` is already valid A64.)
1d. a64 dispatch: `interp_jit_entry` interprets unconditionally; interpreter `Fallback` ops (SWI, MCR/MRC, SWP, DSP muls, etc. — 469 arm/14 thumb table entries) execute single-inst via new `interpreter/fallback.rs` reusing the existing portable handler bodies. Startup rejects `-e 2`.
1e. Pointer-width wave A (boot-blocking only): `jit_memory_map.rs` maps → `HeapArray<usize>` (also fix `HeapArray` default ALIGNMENT=4 under-alignment for 64-bit elements); `ArmContext` → per-arch `HostContext` + sigsegv accessors.
1f. Portability shims: the 8 NEON files per D9; portable Rust GX dispatch loop in `registers_3d.rs` for a64 (the commented-out Rust variant is the template; replace the `exe_swap_buffers` naked lr+0xC trick with a flag check); `screen_layouts.rs` wrappers; `main.rs` features → `cfg_attr(target_arch="arm", …)`.

**Acceptance**: a64 builds; pi5 boots hello_world (rendered) + one commercial ROM `-e 0 -f 1` to gameplay, interpreted; armhf + vita build clean, armhf smoke unchanged. Two commits: "compiles" (1a+1b), "boots" (1c–1f).

## Stage 2 — subtask 5 (infra half): cross-arch tracediff validates stage 1

`tools/pi_tracediff.sh`: deploy both binaries (distinct names, md5sum), identical settings (no input, RA off, same `-e 0 -f`), `--inst-log`, diff. Add `--strict` (zero-resync) mode to `trace_diff.py` for same-engine pairs; **stream the diff** (fifo/windowed) — full boots are ~10⁸ insts × 80 B per side, don't materialize them. Fix what it finds (expected: fallback.rs semantics, GX-loop ordering, shim edges).
**Acceptance**: two ROMs, full boot, zero strict divergence ARM9+ARM7 (armhf-interp @ threshold 255 vs a64-interp).

## Stage 3 — subtasks 3+4: width audit wave B + backend seam (pure armv7 refactor, byte-identical)

Wave B: shared fixes (move `GuestInstOffset`/`GuestInstMetadata` to `assembler/mod.rs` on `HostReg`; pool-indices per D4) vs arm32-owned items that stay inside the backend (every `ldr2(reg, x as u32)` bake, movw/movt patchers, SLOW_MEM budgets, size-40/12 const_asserts, `jump_to_other_guest_pc` layout). Deliverable includes an annotated `grep 'as u32'` audit of src/jit + src/mmap — **plus a 32-bit wrapping-arithmetic audit** (usize math that silently changes semantics at 64-bit, distinct from truncation).
Seam per D2/D3: `assembler/{arm32,aarch64}/`, `emitter/arm32/` (files verbatim), driver hoist in `jit_asm.rs`, patching → `arm32/patch.rs`, naked handlers → `arm32/handlers.rs`.
**Acceptance (armv7-sacred gate)**: debug-only `(guest_pc, xxh32(code))` stream at `jit_insert_block` identical pre/post over 3 ROM boots; vita compiles; pi5 armhf fps unchanged; a64 still boots.

## Stage 4 — subtask 2: wire up vixl aarch64 (parallel with S3)

`vixl/build.rs`: compile vendored `aarch64/*.cc` (`-DVIXL_INCLUDE_TARGET_A64`) for a64 targets only. **Refactor the generator to a hardcoded instruction list** (decided): one-time parse of the aarch32 masm header with the existing Condition-first regex to enumerate everything currently covered, freeze that list in `vixl/build.rs`, and drive BOTH wrapper generations from explicit lists — aarch32 keeps its current output byte-identical, aarch64 gets its own list (movz/movk, ldr/str family, ldp/stp, ALU imm/shifted, csel/cset, branches/labels, mrs/msr NZCV, bitfield, nop, growing as S5 needs) run through the same shim/bindgen machinery with an unconditional-signature template. Expose `ExactAssemblyScope` pool-blocking so literal pools can never land inside fastmem windows or patch stubs.
**Acceptance**: a64 integration test that assembles, mmap-executes, and asserts an NZCV round-trip + ldp/stp + literal case on the pi5; armv7 vixl artifacts byte-identical.

## Stage 5 — the A64 JIT backend

Slices, each behind block-level gating: (1) HostReg + reg_alloc copy (drop thumb `is_low`, pool x19–x26) + BlockAsm skeleton (prologue matching call_jit_entry frame, guest ld/st via `[x27,#i*4]`, NZCV save/restore, call via movz/movk+blr, entry-patch nop window); (2) runtime twins (`jump_to_other_guest_pc`, `validate_guest_block_hash`, the ~14 handler twins — simpler under D7 since x27 is callee-saved); (3) `is_block_jit_supported()` — unsupported block → interpreter (permanent safety valve + per-op-class bisect mask in IS_DEBUG builds); (4) emitter ladder: ALU no-S → **S-ops with explicit shifter carry-out** → local branches → external branches/return stack → single transfers slow-path-always → ldm/stm as ldp/stp → fastmem + SIGSEGV patcher (`aarch64/patch.rs`, A64 raw encoders, budgets) → swp/psr/cp15/swi → DSP/Q-ops as Rust-helper calls → HLE substitutions last (legalizes `-e 2` on a64, restores the forced-JIT gates).
**Acceptance**: hello_world + 2 commercial ROMs boot at threshold 0 (pure jit) and 100 (mixed), `-e 0/1/2`.

## Stage 6 — subtask 5 (full): backend bring-up loop + perf sanity

Primary pair: armhf-jit vs a64-jit, same box, threshold 0, strict mode — post-S3 they share all emit decisions/HLE/cycle accounting, so records must match 1:1; first divergence names the op class (bisect via the class mask, localize with `--inst-log-lazy` + SIGUSR2). Secondary: a64-jit vs a64-interp. Perf floor: bench-scene fps a64-jit ≥ armhf-jit on pi5; armhf unchanged vs baseline.

## Additional considerations (beyond the original five subtasks)

1. **Interpreter-only perf expectations**: idle-loop fast-skip lives in the *emit* path only — interpreted idle loops burn real cycles. Stage-1 a64 will look slow; the fair baseline is armhf at `INTERP_THRESHOLD=255` on the same box, not armhf-jit. If commercial ROMs are unplayable for tracediffing, an optional stage-1.5 ports idle-loop detection to the interpreter.
2. **32-bit wrapping arithmetic ≠ pointer truncation**: `usize` address math that today wraps at 2³² changes value at 64-bit even where no pointer is stored (folded into S3's audit — grep for `wrapping_*` and unmasked address subtraction in mem/mmu paths).
3. **Q flag has no host home on A64** (NZCV bit doesn't exist). Decision: keep it simple — the a64 backend emits calls to Rust helper functions for every Q-modifying instruction (QADD/QSUB/QDADD/QDSUB/SMLAxx), same shape as the existing runtime-helper calls; no inline saturation codegen. Slow but correct; inline later only if profiled hot.
4. ~~a64 jitdump/genelf~~ — dropped: no a64 perf sampling needed.
5. **`HeapArray` alignment bug is pre-existing**: default ALIGNMENT=4 under-aligns 8-byte elements — harmless today (usize=4), UB-adjacent on a64. Fixed in wave A.
6. **`check_stack_depth` limit is tuned for 4-byte frames**; 64-bit frames are fatter — the interpreter/BIOS-nesting depth constant may need scaling (cheap to verify with a deep-recursion title).
7. **Trace volume**: strict full-boot diffs are ~8 GB/side if materialized — the S2 harness streams instead.
8. **Bindgen layouts**: soundtouch/imgui/rcheevos bindings are generated today with a hardcoded thumbv7 clang target — silently wrong struct layouts on a64 (fixed in 1a; this one WOULD have been a haunted-house bug).

## Riskiest items (ranked)

1. Shifter carry-out on A64 (S5.4) — silent wrongness; strict tracediff is the detector. (Q defused via Rust helpers.)
2. emit_transfer lowering + slow-mem patch scheme on A64 (largest code mass; cross-modifying icache discipline on the A76).
3. vixl A64 masm surprises (scratch manager clobbering x16/x17, pools/veneers shifting recorded offsets) — pool-blocking scopes + S4 execute-tests.
4. Fixed-address layout vs a64 kernel ASLR/PIE — S1 step 0 kills this early.
5. S3 refactor regressing armv7 — block-hash byte-identity gate + vita compile + fps spot-check, all mandatory.

## Verification summary

Per-stage gates as listed; the armv7-sacred gate (byte-identical emitted code, vita build, fps) applies to S1, S3, and any shared-file touch in S5. End state: strict-zero tracediff armhf-jit↔a64-jit over full boots, 10+ min stable gameplay per `-e` level on a64, armhf baseline untouched.
