#!/bin/bash
# Usage: shot.sh <out.png> — screenshot the local dsvita window (xwd, no ImageMagick needed).
. "$(dirname "$0")/env.sh"
export DISPLAY="$DSVITA_DISPLAY"
[ -n "$DSVITA_XAUTHORITY" ] && export XAUTHORITY="$DSVITA_XAUTHORITY"
WID=$(xwininfo -root -tree 2>/dev/null | grep '"dsvita" "dsvita"' | grep -o '0x[0-9a-f]*' | head -1)
xwd -id "$WID" -out /tmp/s.xwd 2>/dev/null && python3 "$DSVITA_TOOLS_DIR/xwd2png.py" /tmp/s.xwd "$1" >/dev/null
