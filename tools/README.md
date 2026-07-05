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
| `extract.sh <ilog> <out.txt> [maxlines]` | Decode an ilog and keep only `^ARM7 Executed` register lines |
| `sync.sh <ilog> <out.txt> [lines]` | Decode and grep the IPCSYNC (0x4000180) read/write traffic |
| `gen_thumb_table.py` | Regenerate `src/jit/interpreter/thumb_table.rs` from the thumb disassembler layout |
| `trace_diff.py <a.ilog> <b.ilog> [W]` | Streaming BINARY ilog differ — finds the first divergence (same-pc-diff-regs or unresolvable control-flow split) between two traces without decoding to text. Resyncs around HLE gaps (interp interprets a cold SDK function the jit HLE-replaces), same-block `bl` parity gaps, and spin-loop iteration-count skew. `W` = resync window (default 100000; two-phase 2000 then W). Pair the interp build (threshold 100) against the jit build (threshold 0), both DEBUG_LOG=true. See `DEVELOPMENT.md` §4 for the workflow and parity caveats. |

Decode any ilog to text with `dsvita decode-inst-log <path>`. Record lazily (start on `pkill
-USR2 -x dsvita`) with `--inst-log-lazy <path>` to capture only the final stretch of a long run.
Both need a `DEBUG_LOG = true` build (src/main.rs).
