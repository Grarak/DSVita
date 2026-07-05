---
name: inst-trace
description: Capture, decode, and diff binary per-instruction traces to localize emulation bugs — use when debugging an interpreter-vs-jit divergence, a game hang/crash/corruption, wrong register values, or when asked to trace-diff two engine configurations.
---

# Instruction-trace debugging

The heavy hammer. Before reaching for it, try the cheaper steps in DEVELOPMENT.md §4:
read the panic, A/B the engine (`INTERP_THRESHOLD` 0 = pure jit / 255 = always interpret /
100 = production in `src/jit/interpreter/mod.rs`), and **count events first** — with a
DEBUG_LOG build, grep-counting `send interrupt` / `interrupt {` / `can't interrupt` /
`hle ipc send` lines over a window characterizes a hang in minutes (irqs flowing = game
alive; sends without dispatches = starvation; no irq traffic = pre-irq-setup spin).

## Setup

1. Flip `pub const DEBUG_LOG: bool = true;` in `src/main.rs` (line ~74). **Revert before
   committing.** Stdout becomes huge — redirect to a real disk or pipe through
   `grep --line-buffered`, never a small tmpfs.
2. For an A/B pair, build the two variants as SEPARATE edit+build commands (chained
   `sed && cargo build` can skip the recompile via the mtime race), confirm the
   "Compiling dsvita" line appears, and `md5sum` both binaries to prove they differ.

## Capture

| mode | how |
|---|---|
| From boot | `dsvita --inst-log <path> <rom> ...` (boot with NO input = deterministic, diffs cleanly) |
| Final stretch only | `dsvita --inst-log-lazy <path> ...`, then `kill -USR2 <pid>` at the interesting moment |
| Flush + stop | `kill -INT <pid>` (the panic hook also flushes — a crash self-captures its tail) |

Local capture: `tools/trace.sh <out.ilog>` boots `$DSVITA_TEST_ROM` under qemu.

## Decode & analyze

```bash
dsvita decode-inst-log <path> > out.txt        # exact per-inst text (regs + InstInfo)
grep -a "^ARM9 Executed" out.txt               # per-cpu register-state stream
tools/extract.sh <ilog> <out.txt> [maxlines]   # ARM7 stream shortcut
tools/sync.sh <ilog> <out.txt>                 # IPCSYNC (0x4000180) handshake traffic
tools/trace_diff.py <a.ilog> <b.ilog> [W]      # streaming binary differ with HLE-gap resync
```

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
- Process multi-GB decoded traces on the dev machine, not the test box.
