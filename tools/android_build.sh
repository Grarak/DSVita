#!/bin/bash
# Build libdsvita.so for arm64-v8a and stage it into the gradle jniLibs dir.
#
#   tools/android_build.sh [profile]     profile: dev (default) | release | release-debug
#
# Cross setup: host clang-21 + the NDK's sysroot/compiler-rt (DSVITA_ANDROID_NDK from
# .env) — the official NDK's own binaries are x86_64-only and useless on this aarch64
# box. The CC_/CFLAGS_ env below is for third-party build scripts (ring); our own C deps
# get their flags from vitabuild.
set -e
. "$(dirname "$0")/env.sh"
require_env DSVITA_ANDROID_NDK

PROFILE="${1:-dev}"
case "$PROFILE" in
    dev) PROFILE_FLAG="" ; OUT_DIR=debug ;;
    release) PROFILE_FLAG="--release" ; OUT_DIR=release ;;
    *) PROFILE_FLAG="--profile $PROFILE" ; OUT_DIR="$PROFILE" ;;
esac

SYSROOT="$DSVITA_ANDROID_NDK/toolchains/llvm/prebuilt/linux-x86_64/sysroot"
export CC_aarch64_linux_android=clang-21
export CXX_aarch64_linux_android=clang++-21
export AR_aarch64_linux_android=llvm-ar-21
export CFLAGS_aarch64_linux_android="--target=aarch64-linux-android30 --sysroot=$SYSROOT"
export CXXFLAGS_aarch64_linux_android="--target=aarch64-linux-android30 --sysroot=$SYSROOT"

cd "$DSVITA_ROOT"
cargo rustc --lib --crate-type cdylib --target aarch64-linux-android $PROFILE_FLAG

JNILIBS="$DSVITA_ROOT/android/app/src/main/jniLibs/arm64-v8a"
mkdir -p "$JNILIBS"
cp "target/aarch64-linux-android/$OUT_DIR/libdsvita.so" "$JNILIBS/"
echo "staged: $JNILIBS/libdsvita.so ($(du -h "$JNILIBS/libdsvita.so" | cut -f1))"
