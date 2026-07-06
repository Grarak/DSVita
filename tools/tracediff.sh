#!/bin/bash
# Usage: tracediff.sh <rom-basename.nds> [records=15000000] [arm7_emu=0]
#
# Stage-2 strict cross-arch trace diff: armhf interpreter (INTERP_THRESHOLD=255, on the
# test box) vs aarch64 interpreter (this machine), identical settings, no input, then
# `trace_diff.py --strict` — the traces must match record-for-record.
#
# Both variants are built as the debug profile (opt-level 3 since the profile bump; the
# profile NAME is what turns DEBUG_LOG on — no source flip needed) with a temporary
# INTERP_THRESHOLD=255 source flip (restored on exit), and run with --hle-irq 0 so the
# os irq handler is interpreted raw on both sides (the HLE substitution only exists in
# compiled code). Both sides capture exactly <records> instruction records via
# DSVITA_INST_LOG_MAX (the logger closes itself on a record boundary — signal-based stops
# tear records). The rom's .sav is taken from the test box and restored on both sides
# afterwards, so repeated runs see identical inputs.
set -e
. "$(dirname "$0")/env.sh"
require_env DSVITA_PI_HOST DSVITA_ROMS_DIR

ROM="$1"
RECORDS="${2:-15000000}"
EMU_N="${3:-0}"
[ -n "$ROM" ] || { echo "usage: tracediff.sh <rom.nds> [records] [arm7_emu=0]" >&2; exit 2; }

cd "$DSVITA_ROOT"
INTERP_RS=src/jit/interpreter/mod.rs
trap 'git checkout -- '"$INTERP_RS"' 2>/dev/null' EXIT

sed -i 's/^pub const INTERP_THRESHOLD: u8 = .*/pub const INTERP_THRESHOLD: u8 = 255;/' $INTERP_RS
# Defeat the same-mtime-tick fingerprint race (a sed+build chain can silently skip the
# recompile — see DEVELOPMENT.md pitfalls): bump mtimes, then require the compile line.
sleep 1.1; touch $INTERP_RS

build_checked() {
    local target="$1"
    local out
    out=$(cargo build --target "$target" 2>&1) || { echo "$out" | tail -20; exit 1; }
    echo "$out" | grep -q "Compiling dsvita" || { echo "STALE BUILD for $target (no 'Compiling dsvita')" >&2; exit 1; }
    echo "$out" | grep -E "Compiling dsvita|Finished"
}

echo "== building armhf reference (threshold 255, DEBUG_LOG) =="
build_checked thumbv7neon-unknown-linux-gnueabihf
echo "== building aarch64 (threshold 255, DEBUG_LOG) =="
build_checked aarch64-unknown-linux-gnu

ARMHF=target/thumbv7neon-unknown-linux-gnueabihf/debug/dsvita
A64=target/aarch64-unknown-linux-gnu/debug/dsvita
md5sum $ARMHF $A64

echo "== deploying reference to the test box =="
scp -q $ARMHF "$DSVITA_PI_HOST:~/dsvita_ref"

# Identical save inputs: the box's .sav is the master copy on both sides (absent = absent).
SAV="$ROM.sav"
mkdir -p /tmp/tracediff.$$
if ssh -o BatchMode=yes "$DSVITA_PI_HOST" "test -f '$HOME/nds/$SAV'" 2>/dev/null; then
    scp -q "$DSVITA_PI_HOST:~/nds/$SAV" "/tmp/tracediff.$$/master.sav"
    cp "/tmp/tracediff.$$/master.sav" "$DSVITA_ROMS_DIR/$SAV"
else
    rm -f "$DSVITA_ROMS_DIR/$SAV"
fi

REF_LOG="\$HOME/ref.ilog"
A64_LOG=/tmp/tracediff.$$/a64.ilog

echo "== running both sides ($RECORDS records each) =="
# Each side runs until its logger reports the record budget exhausted (or a hard timeout),
# then is killed — the log file is already complete and flushed at that point.
ssh -o BatchMode=yes "$DSVITA_PI_HOST" "pkill -x dsvita_ref 2>/dev/null; sleep 1; export DISPLAY=:0 LIBGL_ALWAYS_SOFTWARE=1 DSVITA_INST_LOG_MAX=$RECORDS DSVITA_INST_LOG_TEXT=0; cd ~; rm -f ref.ilog; nohup ./dsvita_ref \"\$HOME/nds/$ROM\" -e $EMU_N -f 0 --hle-irq 0 --inst-log $REF_LOG > ~/ref_run.log 2>&1 & for i in \$(seq 600); do grep -q 'record budget exhausted' ~/ref_run.log 2>/dev/null && break; pgrep -x dsvita_ref >/dev/null || break; sleep 2; done; pkill -x dsvita_ref 2>/dev/null; tail -2 ~/ref_run.log; ls -la ref.ilog" 2>&1 | grep -v setlocale &
PI_PID=$!

# No LIBGL_ALWAYS_SOFTWARE here: this box's EGL rejects forced software rendering
# (segfault at init) and nobody looks at the local window during a trace run.
LOCAL_RUN_LOG=/tmp/tracediff.$$/a64_run.log
DSVITA_INST_LOG_MAX=$RECORDS DSVITA_INST_LOG_TEXT=0 "$A64" "$DSVITA_ROMS_DIR/$ROM" -e $EMU_N -f 0 --hle-irq 0 --inst-log "$A64_LOG" > "$LOCAL_RUN_LOG" 2>&1 &
LOCAL_PID=$!
for i in $(seq 600); do
    grep -q 'record budget exhausted' "$LOCAL_RUN_LOG" 2>/dev/null && break
    kill -0 $LOCAL_PID 2>/dev/null || break
    sleep 2
done
kill $LOCAL_PID 2>/dev/null || true
tail -2 "$LOCAL_RUN_LOG" || true
wait $PI_PID || true

# Restore the master sav on both sides (a run may have written it).
if [ -f "/tmp/tracediff.$$/master.sav" ]; then
    cp "/tmp/tracediff.$$/master.sav" "$DSVITA_ROMS_DIR/$SAV"
    scp -q "/tmp/tracediff.$$/master.sav" "$DSVITA_PI_HOST:~/nds/$SAV"
fi

echo "== pulling reference trace =="
scp -q "$DSVITA_PI_HOST:~/ref.ilog" "/tmp/tracediff.$$/ref.ilog"
ssh -o BatchMode=yes "$DSVITA_PI_HOST" "rm -f ~/ref.ilog" 2>/dev/null
ls -la /tmp/tracediff.$$/

echo "== strict diff =="
set +e
python3 "$DSVITA_TOOLS_DIR/trace_diff.py" "/tmp/tracediff.$$/ref.ilog" "$A64_LOG" --strict
RC=$?
echo "(artifacts in /tmp/tracediff.$$ — delete when done)"
exit $RC
