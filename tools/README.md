# Debugging / testing helper scripts

Tooling used while developing the cold-block interpreter (see `DEVELOPMENT.md` in the repo
root for the invariants, workflows, and debugging playbook). All scripts are
environment-agnostic: they source `env.sh`, which loads `<repo>/.env` — copy `.env.example`
from the repo root to `.env` and fill in your paths (sysroot, rom dirs, remote test box).

## Running the emulator

| script | what it does |
|---|---|
| `launch.sh` | Launch the UI (`--ui $DSVITA_ROMS_DIR`) locally under qemu-arm, detached, log to `/tmp/dsvita.log` |
| `launch_hw.sh` | Same but boots `$DSVITA_TEST_ROM` directly |
| `pi_run.sh '<args>'` | Kill + relaunch `$DSVITA_PI_BIN <args>` on the remote ARM test box (native speed). ALWAYS pass a framelimit (`-f 1`). |

## Screenshots

| script | what it does |
|---|---|
| `shot.sh <out.png>` | Local: `xwd` the dsvita window and convert via `xwd2png.py` |
| `pi_shot.sh <out.png>` | Remote box: `grim` the wayland screen, scp it back (scrot only sees X11 → black for the SDL window) |
| `xwd2png.py` | Minimal XWD → PNG converter (no ImageMagick needed) |

## Input injection (remote box)

| script | what it does |
|---|---|
| `pi_key.sh <xkb-key> [hold_ms]` | Press a key via wayland virtual-keyboard (`wtype`; installable root-lessly via `apt download wtype && dpkg -x`) |

Emulator keyboard map: WASD = dpad, K = A, J = B, I = X, U = Y, B = Start, V = Select, 8/9 = L/R.

## Instruction traces (the main interpreter-debugging tool)

| script | what it does |
|---|---|
| `trace.sh <out.ilog>` | Local qemu run of `$DSVITA_TEST_ROM` with `--inst-log <out.ilog>` |
| `trace_decode.sh <ilog> [--cpu N] [--limit N] [--start N] [--index]` | Decode a binary ilog to text (regs + real `InstInfo` disassembly), **x86-native** — a small detached crate (`tools/trace-decode`) that reuses dsvita's actual disassembler but links no C/C++, so it runs on the dev box where there is no dsvita binary. Output matches the in-emulator `decode-inst-log`. |
| `extract.sh <ilog> <out.txt> [maxlines]` | Decode an ilog and keep only `^ARM7 Executed` register lines |
| `sync.sh <ilog> <out.txt> [lines]` | Decode and grep the IPCSYNC (0x4000180) read/write traffic |
| `gen_thumb_table.py` | Regenerate `src/jit/interpreter/thumb_table.rs` from the thumb disassembler layout |
| `trace_diff.py <a.ilog> <b.ilog> [W] [--strict] [--cpu N]` | Streaming BINARY ilog differ — finds the first divergence (same-pc-diff-regs or unresolvable control-flow split) between two traces without decoding to text. Default mode resyncs around HLE gaps, same-block `bl` parity gaps, and spin-loop iteration-count skew (`W` = resync window, default 100000). `--strict` = record-for-record, first mismatch wins. `--cpu 0\|1` filters to one cpu's stream — the way past benign ARM9/ARM7 interleave shifts (per-cpu streams identical + interleave different = timing skew, not a value bug). See `DEVELOPMENT.md` §4 for workflow and parity caveats. |
| `block_diff.py <a> <b> <cpu>` | Per-cpu BLOCK-ENTRY streams (pc discontinuities), engine-neutral; first control-flow-level difference before staring at records |
| `tracediff.sh <rom> [records] [arm7_emu]` | Cross-arch strict pair: armhf interpreter (on the test box) vs aarch64 interpreter (local), identical settings, then `trace_diff.py --strict`. Flips INTERP_THRESHOLD=255 in-source for both builds and restores it |
| `armv7_gate.sh` | armv7-sacred byte-identity gate: per-block `(cpu, pc, thumb, len, xxh32)` stream over deterministic qemu boots vs a baseline — any shared-jit refactor must keep it identical (`armv7_gate_compare.py` masks host-pointer materializations) |

## Profiling / benchmarking (test box)

| script | what it does |
|---|---|
| `ab_measure.sh <bin> <rom>` | Launch uncapped, drive to gameplay, print mean emu-fps over 20 s |
| `ab_batch.sh [base] [opt] [list]` | Interleaved A/B fps of two builds (dropped in `$HOME` on the box, by name) over a rom list (relative to `$DSVITA_ROMS_DIR`; default `$HOME/ab_roms.txt`), thermal-drift-resistant |
| `prof_one2.sh <rom> <out> [boot_s] [prof_s]` | Unified per-game profile: DSVITA_DBG_TOUCH drive-in through menus, then a perf record over gameplay (`prof_one2_audio.sh` = same with `-a`) |
| `batch_prof2.sh [list]` / `batch_audio.sh [list]` | Sweep the profiler over a newline rom list (filenames relative to `$DSVITA_ROMS_DIR`; default `$HOME/prof_keep.txt` / `$HOME/prof_worst.txt`) |
| `gen_symbol_order.sh` / `match_symbol_order.sh` | Link-time hot/cold symbol ordering files from a perf report (see `DEVELOPMENT.md` §6) |

Decode any ilog to text with `tools/trace_decode.sh <path>` (x86-native — the standalone
`tools/trace-decode` crate; use this on the dev box, which has no dsvita binary) or, on an
arm/aarch64 box, the in-emulator `dsvita decode-inst-log <path>` (identical output). Record
lazily (start on `pkill -USR2 -x dsvita`) with `--inst-log-lazy <path>` to capture only the
final stretch of a long run — the tool of choice for hang steady-states. Tracing needs a
DEBUG_LOG build: plain `cargo build` (the dev profile IS the trace build; DEBUG_LOG keys off
the profile name).

## Audio triage (env valves, any build)

`DSVITA_AUDIO_DUMP=<path>` dumps every SPU sample event (8-byte frames: final L/R +
pre-capture mixer L/R, s16le 32768 Hz, pre-transport → guest-deterministic).
`DSVITA_SPU_LOG=1` logs channel/capture/main-cnt writes to stderr. Workflow and analysis
recipes in `DEVELOPMENT.md` §4 "Audio triage".
