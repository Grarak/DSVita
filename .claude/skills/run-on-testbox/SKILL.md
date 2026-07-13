---
name: run-on-testbox
description: Deploy and run DSVita on the remote ARM test box (raspberry pi class, native armhf speed) — use when asked to test a build on the pi/test box, run a game at real speed, take remote screenshots, or inject button presses remotely.
---

# Run DSVita on the remote ARM test box

Prerequisite: `.env` in the repo root with `DSVITA_PI_HOST` (ssh, key auth). Optional:
`DSVITA_PI_BIN` (default `~/claude/dsvita/dsvita`). Never use sudo on the box without asking.

**Working directory & roms.** The dev machine is x86 and can't run the armhf binary natively,
so the pi5 is where all roms/games run. Roms live on the box at `~/nds`. Do every box-side
thing — deployed binaries, logs, traces, savestates — under a `~/claude/dsvita/` working
directory so you don't pollute the home dir; keep `DSVITA_PI_BIN` pointed into it.

## Deploy

```bash
cargo build --profile release-debug --target thumbv7neon-unknown-linux-gnueabihf
ssh "$DSVITA_PI_HOST" 'mkdir -p ~/claude/dsvita'
scp target/thumbv7neon-unknown-linux-gnueabihf/release-debug/dsvita "$DSVITA_PI_HOST:~/claude/dsvita/dsvita"
```

When testing multiple build variants, give each binary a DISTINCT name on the box and
`md5sum` them locally first to prove they differ (a chained `sed && cargo build` can
silently skip the recompile — see DEVELOPMENT.md §4 pitfalls).

## Run / observe / drive

| action | command |
|---|---|
| Kill + relaunch with args | `tools/pi_run.sh '-e 2 -f 1 ~/nds/<rom>.nds'` |
| Screenshot (wayland grim) | `tools/pi_shot.sh <local.png>` |
| Press a key | `tools/pi_key.sh <xkb-key> [hold_ms]` |
| Check alive | `ssh $DSVITA_PI_HOST 'pgrep -x <binary-name>'` |

Keyboard map: WASD = dpad, K = A, J = B, I = X, U = Y, B = Start, V = Select, 8/9 = L/R.

**Debug command port** (debug/release-debug builds) — the headless control channel; needs no
wayland virtual keyboard (so it works even where `wtype`/`pi_key.sh` is absent). Launch with
`DSVITA_DBG_PORT=<port>` and send newline-delimited commands to `127.0.0.1:<port>` on the box,
e.g. `printf 'buttons a\n' | nc -q0 127.0.0.1 5555`:
- `press/release <btn>` | `buttons [<btn>...]` (exact held set) — btn: `a b x y up down left
  right start select l r`. A held button = held on the DS.
- `touch <x> <y>` (DS coords, x 0..256 y 0..192) | `touch off` — drives "touch to start" gates
  and touch menus (Zelda PH, HG title). No drag path, so rub/drag minigames can't be driven.
- `framelimit <0..9>` (0 = uncapped) | `savestate` (quick-save at vblank) | `inst-log` (arm a
  `--inst-log-lazy` capture) | `quit`.

Sequence timing in ONE ssh session (loop the `nc` sends with short sleeps); a command per ssh
round-trip is too slow and the game "heals" between them.

**Input-free verify loop**: the port's `savestate` (or the F11 key) quick-saves at vblank;
relaunch with `-s <savestate>` to resume — lets you A/B or re-test a scene deterministically
without re-driving inputs.

Rules that bite:
- ALWAYS launch with `LIBGL_ALWAYS_SOFTWARE=1` — the box's GPU driver renders incorrectly
  (garbled 2D layers); software GL is the reference. Never diagnose rendering from a
  GPU-driver run.
- ALWAYS pass a framelimit: `-f 1` for interaction, `-f 5`/`-f 9` to fast-forward through
  boot/loading. Uncapped (`-f 0`) is uninteractable.
- `pkill -f` / `pgrep -f` over ssh match the remote shell's own command line and kill your
  session — use `-x <exact-binary-name>` and name binaries distinctly.
- The box's `/tmp` is a small tmpfs: redirect big logs (DEBUG_LOG stdout, traces) to the
  home directory, never `/tmp`. A full `/tmp` kills the cpu thread with a StorageFull panic
  while the UI keeps rendering — looks exactly like an emulation hang.
- The box's V3D GL driver has a known cosmetic tile/glyph rendering offset; verify suspected
  rendering bugs against a pure-jit build or another renderer before blaming emulation.
