#!/bin/bash
# Usage: sync.sh <in.ilog> <out.txt> [lines]
# Pull the IPCSYNC handshake (both cpus) plus surrounding executed-record lines from a decoded ilog.
set -e
. "$(dirname "$0")/env.sh"
require_env DSVITA_SYSROOT
cd "$DSVITA_ROOT"
qemu-arm -L "$DSVITA_SYSROOT" target/thumbv7neon-unknown-linux-gnueabihf/release-debug/dsvita decode-inst-log "$1" 2>/dev/null \
  | grep -an "memory write at 4000180\|memory read at 4000180 with" | head -"${3:-400}" > "$2"
wc -l "$2"
