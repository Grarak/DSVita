#!/bin/bash
# Build libdsvita.so for arm64-v8a. The gradle app module (or a caller) copies it
# out of target/; this script only cross-builds.
#
#   tools/android_build.sh [profile]     profile: dev (default) | release | release-debug
#
# Cross setup: host clang-21 + the NDK's sysroot/compiler-rt (ANDROID_NDK_HOME) — the
# official NDK binaries are x86_64-only, useless on an aarch64 box. The CC_/CFLAGS_ env
# below is for third-party build scripts (ring); our own C deps get flags from vitabuild.
# The gradle app module calls this to cross-build the cdylib; see DEVELOPMENT.md Â§9.
set -e
. "$(dirname "$0")/env.sh"
require_env ANDROID_NDK_HOME

PROFILE="${1:-dev}"
case "$PROFILE" in
    dev) PROFILE_FLAG="" ; OUT_DIR=debug ;;
    release) PROFILE_FLAG="--release" ; OUT_DIR=release ;;
    *) PROFILE_FLAG="--profile $PROFILE" ; OUT_DIR="$PROFILE" ;;
esac

SYSROOT="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot"
export CC_aarch64_linux_android=clang-21
export CXX_aarch64_linux_android=clang++-21
export AR_aarch64_linux_android=llvm-ar-21
export CFLAGS_aarch64_linux_android="--target=aarch64-linux-android30 --sysroot=$SYSROOT"
export CXXFLAGS_aarch64_linux_android="--target=aarch64-linux-android30 --sysroot=$SYSROOT"

cd "$DSVITA_ROOT"
cargo rustc --lib --crate-type cdylib --target aarch64-linux-android $PROFILE_FLAG
echo "built: target/aarch64-linux-android/$OUT_DIR/libdsvita.so ($(du -h "target/aarch64-linux-android/$OUT_DIR/libdsvita.so" | cut -f1))"
