---
name: run-on-testbox
description: Deploy and run DSVita on the remote ARM test box (raspberry pi class, native armhf speed) — use when asked to test a build on the pi/test box, run a game at real speed, take remote screenshots, or inject button presses remotely.
---

# Run DSVita on the remote ARM test box

Prerequisite: `.env` in the repo root with `DSVITA_PI_HOST` (ssh, key auth). Optional:
`DSVITA_PI_BIN` (default `~/dsvita`). Never use sudo on the box without asking.

## Deploy

```bash
cargo build --profile release-debug --target thumbv7neon-unknown-linux-gnueabihf
scp target/thumbv7neon-unknown-linux-gnueabihf/release-debug/dsvita "$DSVITA_PI_HOST:~/dsvita"
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
Mouse click on the bottom-screen area = touch (needs local display; remotely there is no
touch injection — only keys).

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
