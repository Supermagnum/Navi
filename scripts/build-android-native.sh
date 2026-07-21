#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Use the workspace target/ tree so jniLibs copies are stable across environments.
unset CARGO_TARGET_DIR

ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-/home/haaken/Android/Sdk/ndk/27.3.13750724}"
export ANDROID_NDK_HOME

if [[ ! -d "$ANDROID_NDK_HOME" ]]; then
  echo "error: ANDROID_NDK_HOME not found at $ANDROID_NDK_HOME" >&2
  exit 1
fi

TARGET="${1:-x86_64-linux-android}"
PROFILE="${2:-release}"

case "$TARGET" in
  x86_64-linux-android)
    ABI_DIR="x86_64"
    ;;
  aarch64-linux-android)
    ABI_DIR="arm64-v8a"
    ;;
  *)
    echo "error: unsupported target $TARGET" >&2
    exit 1
    ;;
esac

export PATH="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH"

if [[ -z "${RUSTUP_TOOLCHAIN:-}" ]]; then
  :
fi

echo "Building navi-ffi for $TARGET ($PROFILE)..."
cargo build -p navi-ffi --target "$TARGET" --"$PROFILE" --lib

LIB_SRC="$ROOT/target/$TARGET/$PROFILE/libnavi.so"
LIB_DST_DIR="$ROOT/app/src/main/jniLibs/$ABI_DIR"
mkdir -p "$LIB_DST_DIR"
cp -f "$LIB_SRC" "$LIB_DST_DIR/libnavi.so"
echo "Copied $LIB_SRC -> $LIB_DST_DIR/libnavi.so"

KOTLIN_OUT="$ROOT/app/src/main/java"
mkdir -p "$KOTLIN_OUT"
echo "Generating UniFFI Kotlin bindings..."
cargo run -p navi-ffi --bin uniffi-bindgen -- generate \
  --library "$LIB_SRC" \
  --language kotlin \
  --out-dir "$KOTLIN_OUT"

echo "Done. Native library and Kotlin bindings are ready under app/."
