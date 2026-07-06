#!/bin/bash
# Usage: trace.sh <inst-log-path>
# Boot DSVITA_TEST_ROM locally under qemu-arm with --inst-log, detached.
. "$(dirname "$0")/env.sh"
require_env DSVITA_SYSROOT DSVITA_TEST_ROM
export DISPLAY="$DSVITA_DISPLAY"
[ -n "$DSVITA_XAUTHORITY" ] && export XAUTHORITY="$DSVITA_XAUTHORITY"
export LIBGL_ALWAYS_SOFTWARE=1
cd "$DSVITA_ROOT"
pkill -9 -f "debug/dsvita" 2>/dev/null
sleep 1
rm -f /tmp/dsvita_trace.log
# The debug profile IS the tracing build (DEBUG_LOG keys off the profile name; opt-level 3).
setsid qemu-arm -L "$DSVITA_SYSROOT" target/thumbv7neon-unknown-linux-gnueabihf/debug/dsvita --inst-log "$1" "$DSVITA_TEST_ROM" >/tmp/dsvita_trace.log 2>&1 &
disown
