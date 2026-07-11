#!/bin/bash
# Build (cached) and run the standalone x86 trace decoder on a binary .ilog.
#
# Usage: trace_decode.sh <trace.ilog> [--cpu N] [--limit N] [--start N] [--index]
#
# The decoder is a small detached crate (tools/trace-decode) that reuses dsvita's REAL
# disassembler via #[path] but links no C/C++ (see its Cargo.toml), so it runs natively on the
# x86 dev box — unlike the in-emulator decoder, which only exists in the arm/aarch64 build.
# First build compiles std from source (the repo's build-std config) and takes ~30s; after that
# it is a cached no-op and the tool starts instantly.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/tools/trace-decode/Cargo.toml"
BIN="$ROOT/tools/trace-decode/target/release/trace_decode"

cargo build --release --manifest-path "$MANIFEST" >&2
exec "$BIN" "$@"
