#!/bin/bash
# Launch the UI locally under qemu-arm, detached, log to /tmp/dsvita.log.
. "$(dirname "$0")/env.sh"
require_env DSVITA_SYSROOT DSVITA_ROMS_DIR
export DISPLAY="$DSVITA_DISPLAY"
[ -n "$DSVITA_XAUTHORITY" ] && export XAUTHORITY="$DSVITA_XAUTHORITY"
export LIBGL_ALWAYS_SOFTWARE=1
cd "$DSVITA_ROOT"
pkill -9 -f "release-debug/dsvita" 2>/dev/null
sleep 1
rm -f /tmp/dsvita.log
setsid qemu-arm -L "$DSVITA_SYSROOT" target/thumbv7neon-unknown-linux-gnueabihf/release-debug/dsvita --ui "$DSVITA_ROMS_DIR" >/tmp/dsvita.log 2>&1 &
disown
