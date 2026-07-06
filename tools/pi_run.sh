#!/bin/bash
# Usage: pi_run.sh <extra dsvita args...> — kill + relaunch the emulator on the remote test box.
. "$(dirname "$0")/env.sh"
require_env DSVITA_PI_HOST
BIN_NAME=$(basename "$DSVITA_PI_BIN")
ssh -o BatchMode=yes "$DSVITA_PI_HOST" "pkill -x $BIN_NAME; sleep 1; export DISPLAY=:0; export LIBGL_ALWAYS_SOFTWARE=1; cd ~; nohup $DSVITA_PI_BIN $* >/tmp/ds.log 2>&1 & sleep 2; pgrep -x $BIN_NAME >/dev/null && echo RUNNING || tail -5 /tmp/ds.log" 2>&1 | grep -v setlocale
