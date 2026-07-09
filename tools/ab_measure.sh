#!/bin/bash
# ab_measure.sh <bin_name> <rom> — launch uncapped, drive to gameplay, print mean emu-fps over 20s.
set -u
NAME="$1"; ROM="$2"; BIN=~/"$1"
export DISPLAY=:0 LIBGL_ALWAYS_SOFTWARE=1 DSVITA_DBG_TOUCH=1 XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0
WT=~/tools/usr/bin/wtype; key(){ "$WT" -P "$1" -s "${2:-110}" -p "$1" 2>/dev/null; }
pkill -x "$NAME" 2>/dev/null; sleep 1
LOG=~/ab_run.log; : > "$LOG"
nohup "$BIN" -f 0 "$ROM" >"$LOG" 2>&1 &
sleep 3; PID=$(pgrep -x "$NAME"|head -1); [ -z "$PID" ] && { echo "FAIL_LAUNCH"; exit 1; }
sleep 16
for i in 1 2 3 4 5 6; do key b 120; sleep 0.5; key k 120; sleep 0.5; key o 120; sleep 0.4; done
key w 180; key d 180
( for i in $(seq 1 40); do key d 170; key w 150; key a 170; key s 150; key k 90; done ) >/dev/null 2>&1 &
INP=$!
N=$(wc -l < "$LOG"); sleep 20
kill "$INP" 2>/dev/null; pkill -x wtype 2>/dev/null
tail -n +$((N+1)) "$LOG" | grep -aE '^[0-9]+$' | awk '{s+=$1;n++} END{if(n)printf "%.0f\n",s/n; else print 0}'
pkill -x "$NAME" 2>/dev/null
