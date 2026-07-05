#!/bin/bash
# Usage: extract.sh <in.ilog> <out.txt> [maxlines]
# Decode a binary inst log and keep only ARM7 "Executed" register-state lines.
set -e
. "$(dirname "$0")/env.sh"
require_env DSVITA_SYSROOT
cd "$DSVITA_ROOT"
qemu-arm -L "$DSVITA_SYSROOT" target/thumbv7neon-unknown-linux-gnueabihf/release-debug/dsvita decode-inst-log "$1" 2>/dev/null \
  | grep -a "^ARM7 Executed" | head -"${3:-3000000}" > "$2"
wc -l "$2"
