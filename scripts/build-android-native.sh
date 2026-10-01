#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Use the workspace target/ tree so jniLibs copies are stable across environments.
unset CARGO_TARGET_DIR

# Resolve NDK: explicit ANDROID_NDK_HOME, else newest folder under ANDROID_HOME/ndk.
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  if [[ -n "${ANDROID_HOME:-}" && -d "${ANDROID_HOME}/ndk" ]]; then
    ANDROID_NDK_HOME="$(find "${ANDROID_HOME}/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -n 1 || true)"
  fi
fi
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-}"

if [[ -z "${ANDROID_NDK_HOME}" || ! -d "${ANDROID_NDK_HOME}" ]]; then
  echo "error: ANDROID_NDK_HOME is not set or not a directory." >&2
  echo "  Export ANDROID_NDK_HOME to your NDK install, e.g.:" >&2
  echo "    Linux:   export ANDROID_NDK_HOME=\"\$HOME/Android/Sdk/ndk/<version>\"" >&2
  echo "    macOS:   export ANDROID_NDK_HOME=\"\$HOME/Library/Android/sdk/ndk/<version>\"" >&2
  echo "    Windows (Git Bash): export ANDROID_NDK_HOME=\"\$LOCALAPPDATA/Android/Sdk/ndk/<version>\"" >&2
  echo "  Or set ANDROID_HOME so this script can pick the newest ndk/<version>." >&2
  exit 1
fi

# Host tag for NDK prebuilt clang (PATH). Prefer native; fall back where NDK ships only one.
detect_ndk_host_tag() {
  local prebuilt="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt"
  case "$(uname -s)" in
    Linux*)
      echo "linux-x86_64"
      ;;
    Darwin*)
      if [[ "$(uname -m)" == "arm64" ]]; then
        if [[ -d "$prebuilt/darwin-arm64" ]]; then
          echo "darwin-arm64"
        else
          echo "darwin-x86_64"
        fi
      else
        echo "darwin-x86_64"
      fi
      ;;
    MINGW*|MSYS*|CYGWIN*)
      echo "windows-x86_64"
      ;;
    *)
      echo "error: unsupported host OS for Android NDK PATH setup: $(uname -s)" >&2
      exit 1
      ;;
  esac
}

NDK_HOST_TAG="$(detect_ndk_host_tag)"
NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_HOST_TAG/bin"
if [[ ! -d "$NDK_BIN" ]]; then
  echo "error: NDK clang bin not found at $NDK_BIN" >&2
  echo "  Check ANDROID_NDK_HOME and that .cargo/config.toml linker paths use the same host tag ($NDK_HOST_TAG)." >&2
  exit 1
fi
export PATH="$NDK_BIN:$PATH"

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
    echo "  Use: x86_64-linux-android (emulator) or aarch64-linux-android (most phones/tablets)" >&2
    exit 1
    ;;
esac

echo "Building navi-ffi for $TARGET ($PROFILE) with NDK $ANDROID_NDK_HOME ($NDK_HOST_TAG)..."
CARGO_PROFILE_ARGS=()
case "$PROFILE" in
  release)
    CARGO_PROFILE_ARGS=(--release)
    ;;
  debug)
    # cargo has no --debug flag; debug is the default profile
    CARGO_PROFILE_ARGS=()
    ;;
  *)
    CARGO_PROFILE_ARGS=(--profile "$PROFILE")
    ;;
esac
cargo build -p navi-ffi --target "$TARGET" "${CARGO_PROFILE_ARGS[@]}" --lib

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
