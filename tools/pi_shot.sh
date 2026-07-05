#!/bin/bash
# Usage: pi_shot.sh <local.png> — grab the remote test box's wayland screen.
. "$(dirname "$0")/env.sh"
require_env DSVITA_PI_HOST
ssh -o BatchMode=yes "$DSVITA_PI_HOST" "XDG_RUNTIME_DIR=$DSVITA_PI_RUNTIME_DIR WAYLAND_DISPLAY=$DSVITA_PI_WAYLAND_DISPLAY grim -t png /tmp/shot.png" 2>/dev/null
scp -q "$DSVITA_PI_HOST:/tmp/shot.png" "$1"
