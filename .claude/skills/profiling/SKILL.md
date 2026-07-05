---
name: profiling
description: Profile DSVita on the Linux ARM test box — flat perf profiles, per-instruction annotation of native functions and inside JIT blocks (jitdump), and A/B throughput benchmarking. Use when asked to profile, find hotspots, annotate, or measure a performance change.
---

# Profiling DSVita

Prerequisite: `.env` with `DSVITA_PI_HOST` (see run-on-testbox skill for deploy/keys/rules).
perf works unprivileged on the box (`perf_event_paranoid=-1`). Decide first which of the
three tools answers the question:

| question | tool |
|---|---|
| which functions/blocks are hot | flat profile (perf map) |
| which instructions inside them | annotate (native) / jitdump (JIT blocks) |
| did my change help | A/B throughput — the only tool that catches fence/contention costs |

## Builds

- Flat profile / JIT-block annotate: `--profile release-debug` (optimized + debuginfo).
- Native-code annotate with source lines: same build; `CARGO_PROFILE_RELEASE_DEBUG=2` if
  using `--release` (env `RUSTFLAGS` replaces `.cargo/config.toml` rustflags — repeat
  `-Ctarget-feature=+v7,+neon,+vfp3,+thumb2,+thumb-mode` if you set it).
- Callgraphs through the JIT are impossible (no fp chains in emitted code) — don't try;
  flat attribution plus jitdump is the workflow.

## Flat profile

The emulator always writes `/tmp/perf-<pid>.map` on Linux (`ARM9_<guest_pc>` symbols).

```bash
perf record -F 997 -p <pid> -o out.data -- sleep 30
perf report -i out.data --comms cpu --no-children -s symbol -g none --percent-limit 0.05 --stdio
```

`--comms cpu` = the emulation thread. Other threads: `audio_out` (busy-wait pacing by
design, ignore its share), `actual_main` (GL present), `process_3d`.

## Inside JIT blocks (jitdump)

Custom perf on the box: `~/linux-7.0/tools/perf/perf` (built from kernel 7.0 with patched
`util/genelf.c|h`: thumb bit on the symbol, `$t` mapping symbol, EM_ARM/ELFCLASS32 forced —
if it's ever rebuilt, those patches must survive or thumb blocks disassemble as garbage).

```bash
DSVITA_JITDUMP=1 ./dsvita <rom> ...            # writes /tmp/jit-<pid>.dump
perf record -F 997 -k CLOCK_MONOTONIC -p <pid> -o out.data -- sleep 30   # mono clock REQUIRED
perf inject --jit -i out.data -o out.jitted.data                          # -> /tmp/jitted-<pid>-N.so
perf annotate -i out.jitted.data --stdio -s ARM9_<guest_pc>
```

To find systematic emitter overhead, aggregate hot lines across the top ~25 blocks (loop:
report `-s dso` → readelf the .so for the symbol → annotate, keep lines ≥ N%).

Attribution caveats:
- One jitdump record per debug-info sub-block: a branch between records of the same
  compiled unit LOOKS cross-block but is local.
- High % on a cheap instruction right after a load = the load's latency (skid).
- Fence/lock/contention cost barely shows in samples at all (a lock that capped throughput
  5x sat at ~1%). If a subsystem "can't be the problem" by sample share, A/B it anyway.

## A/B throughput benchmark

Metric: per-second vblank count lines the emulator logs on stdout (uncapped = raw speed).

1. Deploy both binaries under distinct names; `md5sum` locally first (stale-binary trap:
   never chain edit && build in one command).
2. Navigate to the SAME test scene at `-f 1` (choreography timed against log-line counts
   breaks at uncapped speed), then press F10 to uncap live (F1–F9 = framelimit 1–9).
3. `N=$(wc -l < log); sleep 42; tail -n +$((N+1)) log | head -40` → mean ± sd.
4. Rules: back-to-back runs (thermal drift between sessions beats the effect size),
   audio-on and audio-off are different baselines, sd is typically ±7 — treat anything
   under ~2% as noise, and re-measure the baseline whenever a result surprises you.

## Identifying hot guest blocks

Top `ARM9_<pc>` symbol → dump live guest memory through the fastmem mirror
(`/proc/<pid>/mem` at host `0x80000000 + guest_addr` on Linux — catches runtime-loaded and
decompressed code the static rom misses) → `objdump -D -b binary -m arm` (add
`-M force-thumb` for thumb regions) → identify against the SDK sources or decompilation
projects before writing any HLE pattern. Read constants a pattern's handler needs from the
guest literal pool at runtime; never hardcode them.

## The verdict rule

The dev box is out-of-order silicon with a different thread architecture; the target is
in-order. Wins here can be nil there (and vice versa — several target-positive changes
measured neutral here). A perf change only counts after the target hardware measured it.
One change per commit so each can be accepted or dropped independently.
