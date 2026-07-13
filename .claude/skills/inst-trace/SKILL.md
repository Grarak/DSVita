---
name: inst-trace
description: Capture, decode, and diff binary per-instruction traces to localize emulation bugs — use when debugging an interpreter-vs-jit divergence, a game hang/crash/corruption, wrong register values, or when asked to trace-diff two engine configurations.
---

# Instruction-trace debugging

**Trace hello_world.nds first** (maintainer rule) — it's the cheapest gate: tiny, boots
instantly, deterministic from boot with no input/savestate, exercises the core jit path. A
strict self-diff on it catches most emitter regressions in seconds before you spend time on
the heavier roms.

The heavy hammer. Before reaching for it, try the cheaper steps in DEVELOPMENT.md §4:
read the panic, A/B the engine (`INTERP_THRESHOLD` 0 = pure jit / 255 = always interpret /
100 = production in `src/jit/interpreter/mod.rs`), and **count events first** — with a
DEBUG_LOG build, grep-counting `send interrupt` / `interrupt {` / `can't interrupt` /
`hle ipc send` lines over a window characterizes a hang in minutes (irqs flowing = game
alive; sends without dispatches = starvation; no irq traffic = pre-irq-setup spin).

## Setup

1. `DEBUG_LOG` is derived from the build profile (`main.rs`: `const_str_equal(BUILD_PROFILE_NAME,
   "debug")`) — no source edit. **Build the `debug` profile** (plain `cargo build`, opt-level 3
   since the profile bump) to get a logging binary; `--inst-log` and friends only register under
   it. Stdout becomes huge — redirect to a real disk or pipe through `grep --line-buffered`,
   never a small tmpfs.
2. To force the JIT path (so an emitter change is actually exercised in the trace) sed
   `INTERP_THRESHOLD=0` (`src/jit/interpreter/mod.rs`) — otherwise blocks interpret for their
   first 100 executions and the emitted code the change touches may never run. Restore it after.
3. For an A/B pair, build the two variants as SEPARATE edit+build commands (chained
   `sed && cargo build` can skip the recompile via the mtime race), confirm the
   "Compiling dsvita" line appears, and `md5sum` both binaries to prove they differ.

## Capture

| mode | how |
|---|---|
| From boot | `dsvita --inst-log <path> <rom> ...` (boot with NO input = deterministic, diffs cleanly) |
| Final stretch only | `dsvita --inst-log-lazy <path> ...` with `DSVITA_DBG_PORT=<port>`, then send `inst-log` to the port at the interesting moment (`printf 'inst-log\n' \| nc -q0 127.0.0.1 <port>`) |
| Flush + stop | `kill -INT <pid>` (the panic hook also flushes — a crash self-captures its tail) |

Local capture: `tools/trace.sh <out.ilog>` boots `$DSVITA_TEST_ROM` under qemu.

## Decode & analyze

```bash
tools/trace_decode.sh <path> > out.txt         # exact per-inst text (regs + InstInfo), x86-NATIVE
grep -a "^ARM9 Executed" out.txt               # per-cpu register-state stream
tools/trace_decode.sh <path> --cpu 1 --index   # filter to one cpu (0=ARM9,1=ARM7), number records
tools/extract.sh <ilog> <out.txt> [maxlines]   # ARM7 stream shortcut
tools/sync.sh <ilog> <out.txt>                 # IPCSYNC (0x4000180) handshake traffic
tools/trace_diff.py <a.ilog> <b.ilog> [W]      # streaming binary differ with HLE-gap resync
```

`trace_decode.sh` builds/runs `tools/trace-decode` — a detached crate that reuses dsvita's REAL
disassembler (`#[path]` includes of `src/jit/disassembler/**`, `inst_info`, `op`) but links no
C/C++, so it decodes on the x86 dev box where there is no dsvita binary. Its output is identical
to the in-emulator `dsvita decode-inst-log <path>` (which still works on an arm/aarch64 box).

Interleaved `memory read/write at X with value Y` text records show io handshakes,
overlay/file loads, and pointer provenance — grep them before writing new tooling.

## Diff parity rules (each cost real time once)

- Neither engine logs taken branches; the jit logs EXTRA records the interpreter doesn't
  ("enter block" markers, return-stack-resume re-logs, same-block `bl`s). Filter to
  `Executed` records and drop records whose InstInfo writes PC before comparing.
- Same PC + different registers = real bug. Different PC sequence + same registers =
  usually benign timing skew; io-poll loops legitimately differ in iteration count.
- An interp-vs-jit diff cannot distinguish "interp = hardware, jit HLE ≠ hardware" (benign)
  from "interp ≠ hardware" (bug) — break ties with a third reference (NooDS; instrument its
  Memory::write with an address watch for a known-good write sequence).
- **Never decode or analyze traces on the test box** — always scp the .ilog to the dev
  machine first (maintainer rule). The box's SD is slow and small; the dev machine chews a
  24 GB log in seconds. Decoding/diffing is pure ilog parsing — architecture-independent, so
  it ALWAYS happens on the dev machine regardless of where the trace was captured. On an x86
  dev box (no dsvita binary) decode with `tools/trace_decode.sh` and diff with `trace_diff.py`
  — both run natively there.
- **Only run a built binary locally when the dev machine's arch matches it.** `tracediff.sh`
  runs the `aarch64-unknown-linux-gnu` side natively — that assumes an **aarch64 dev box**
  (the current setup: native a64 + `qemu-arm` for the a32 side). On an **x86 dev machine**,
  neither the a64 nor the armhf binary runs natively at useful speed: capture BOTH sides on
  the pi (armhf is native there) — or use `qemu-arm` locally only for the a32 side — and still
  pull the ilogs to decode/diff locally. Don't blindly invoke `tracediff.sh` off an aarch64
  box.
- **A byte-identical boot-window diff is NECESSARY BUT NOT SUFFICIENT** for a jit/emitter
  change. Boot (2M–4M records) never reaches JIT arena reset (`reset_blocks` when the ~28MB
  arena fills), fs-clear overlay reloads, or state that only forms after warmup (self-modifying
  code paths, a rarely-hit interrupt). An emitter change once passed a 4M-record MKDS-boot diff
  byte-identical yet crashed the game at ~40s, well past the boot window. So after the strict
  diff, ALSO run real games (release-debug) past arena reset — ~40–60s at `-f 1`, and screenshot.
  Standing set: Mario Kart DS, Pokémon Diamond, HeartGold. (Diamond/HG are NitroSDK overlay reloaders.)

## Keeping traces small

- `DSVITA_INST_LOG_TEXT=0` drops the interleaved text records — on a commercial boot they
  outweigh the instruction records. Strict A/B diffs never need them.
- Instruction records are delta-encoded automatically (~5x smaller than the old fixed 80 B
  records; keyframe every 2^20 records per cpu). All readers (decode-inst-log,
  trace_diff.py) handle both formats.
- The very frequent mem.rs "memory read/write at" lines are compact binary TAG_MEM records
  (6-10 B vs ~40-50 B text; slice/DMA element lines batch into one record). The decoder
  regenerates the exact old text lines, so decoded output and greps are unchanged.
  `DSVITA_INST_LOG_TEXT=0` drops them like any text. Logs from before July 10 2026 still
  decode; older binaries can't read new logs.
- `DSVITA_INST_LOG_MAX=N` stops the log after exactly N records, flushed from the logging
  thread — use it instead of SIGINT for A/B pairs (a signal stop can tear a record).
