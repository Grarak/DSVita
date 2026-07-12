#!/bin/bash
# prof_one2.sh <rom_path> <outname> [boot_s] [prof_s]
# Corrected unified profiler: touch build + DSVITA_DBG_TOUCH, longer looping drive-in that
# navigates save/difficulty/language menus and touch gates, and a gameplay profiling loop
# with NO Start/Select (so the game never sits paused during sampling).
set -u
. "$(dirname "$0")/env.sh"
ROM="$1"; OUT="$2"; BOOT="${3:-16}"; PROF="${4:-25}"
BIN="${DSVITA_PI_BIN/#\~/$HOME}"; NAME=$(basename "$BIN")
D="$HOME/prof"
mkdir -p "$D"
export DISPLAY="$DSVITA_DISPLAY" LIBGL_ALWAYS_SOFTWARE=1 DSVITA_DBG_TOUCH=1
export XDG_RUNTIME_DIR="$DSVITA_PI_RUNTIME_DIR" WAYLAND_DISPLAY="$DSVITA_PI_WAYLAND_DISPLAY"
WT="$DSVITA_PI_WTYPE"
key(){ "$WT" -P "$1" -s "${2:-110}" -p "$1" 2>/dev/null; }

pkill -x "$NAME" 2>/dev/null; sleep 1
: > "$D/$OUT.log"
nohup "$BIN" -f 0 "$ROM" >"$D/$OUT.log" 2>&1 &
sleep 3
PID=$(pgrep -x "$NAME" | head -1)
[ -z "$PID" ] && { echo "STATUS=LAUNCH_FAIL"; tail -3 "$D/$OUT.log"; exit 1; }

sleep "$BOOT"

# Drive-in (gentle, pass-1 style + center touch): advance logos/titles and confirm the DEFAULT
# menu item (usually Continue/Start). No d-pad nav — moving off the default lands on New Game /
# wrong options and pulls save games into name-entry. Center tap 'o' clears touch-to-start gates.
for i in 1 2 3 4 5 6; do
  key b 120; sleep 0.5     # DS Start (title -> menu)
  key k 120; sleep 0.5     # A (confirm default: Continue / Start)
  key o 120; sleep 0.4     # center touch (touch-to-start gates)
done
key w 180; key d 180       # nudge into motion

grim -t png "$D/$OUT.pre.png" 2>/dev/null
kill -0 "$PID" 2>/dev/null || { echo "STATUS=DIED_BEFORE_PROFILE"; tail -3 "$D/$OUT.log"; exit 2; }

# Gameplay input during profiling: movement + A/X ONLY. No Start/Select (pause), no B (back/cancel
# can exit gameplay into menus). Keeps the game rendering real gameplay throughout the sample.
( for i in $(seq 1 60); do
    key d 170; key w 150; key a 170; key s 150; key k 90; key d 170; key w 150; key i 80
  done ) >/dev/null 2>&1 &
INP=$!
perf record -F 997 -p "$PID" -o "$D/$OUT.data" -- sleep "$PROF" 2>/dev/null
kill "$INP" 2>/dev/null; pkill -x wtype 2>/dev/null
grim -t png "$D/$OUT.post.png" 2>/dev/null

alive=DEAD; kill -0 "$PID" 2>/dev/null && alive=ALIVE
FPS=$(grep -aE '^[0-9]+$' "$D/$OUT.log" | tail -3 | tr '\n' ' ')
if [ -s "$D/$OUT.data" ]; then
  perf report -i "$D/$OUT.data" --comms cpu --no-children -s symbol -g none --percent-limit 0.3 --stdio 2>/dev/null > "$D/$OUT.report.txt"
  perf report -i "$D/$OUT.data" --no-children -s comm -g none --stdio 2>/dev/null | grep -aE '%' | head -8 > "$D/$OUT.comms.txt"
  rm -f "$D/$OUT.data"
fi
pkill -x "$NAME" 2>/dev/null
echo "STATUS=OK pid=$PID final=$alive fps=[$FPS]"
