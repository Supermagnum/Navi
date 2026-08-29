#!/usr/bin/env bash
# Gate step for navi-plugin-host on Android aarch64:
#
# 1) Cross-compile for aarch64-linux-android (NDK) — proves the Android ABI link.
# 2) Execute isolation smoke on aarch64 Cranelift codegen via either:
#      --qemu  : static aarch64-unknown-linux-musl binary under qemu-aarch64-static
#                (same Cranelift aarch64 backend; validates RUSTSEC-2026-0096 class)
#      --adb   : push the aarch64-linux-android binary to an arm64-v8a device/emulator
#
# Usage:
#   scripts/plugin-host-android-aarch64-smoke.sh           # build android + qemu exec
#   scripts/plugin-host-android-aarch64-smoke.sh --adb
#   scripts/plugin-host-android-aarch64-smoke.sh --build-only
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="qemu"
case "${1:-}" in
  --adb) MODE="adb" ;;
  --build-only) MODE="build-only" ;;
  --qemu|"") MODE="qemu" ;;
  -h|--help)
    sed -n '2,16p' "$0"
    exit 0
    ;;
  *)
    echo "error: unknown option: $1" >&2
    exit 2
    ;;
esac

# Resolve NDK (same rules as scripts/build-android-native.sh).
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  if [[ -n "${ANDROID_HOME:-}" && -d "${ANDROID_HOME}/ndk" ]]; then
    ANDROID_NDK_HOME="$(find "${ANDROID_HOME}/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -n 1 || true)"
  fi
fi
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-}"
if [[ -z "${ANDROID_NDK_HOME}" || ! -d "${ANDROID_NDK_HOME}" ]]; then
  echo "error: ANDROID_NDK_HOME is not set or not a directory." >&2
  exit 1
fi

detect_ndk_host_tag() {
  case "$(uname -s)" in
    Linux*) echo "linux-x86_64" ;;
    Darwin*)
      if [[ "$(uname -m)" == "arm64" && -d "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-arm64" ]]; then
        echo "darwin-arm64"
      else
        echo "darwin-x86_64"
      fi
      ;;
    MINGW*|MSYS*|CYGWIN*) echo "windows-x86_64" ;;
    *) echo "error: unsupported host OS" >&2; exit 1 ;;
  esac
}

NDK_HOST_TAG="$(detect_ndk_host_tag)"
NDK_PRE="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_HOST_TAG"
NDK_BIN="$NDK_PRE/bin"
CLANG="$NDK_BIN/aarch64-linux-android24-clang"
if [[ ! -x "$CLANG" ]]; then
  CLANG="$NDK_BIN/aarch64-linux-android34-clang"
fi
if [[ ! -x "$CLANG" ]]; then
  echo "error: aarch64 Android clang not found under $NDK_BIN" >&2
  exit 1
fi
export PATH="$NDK_BIN:$PATH"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CLANG"

ANDROID_TARGET=aarch64-linux-android
MUSL_TARGET=aarch64-unknown-linux-musl
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
export CARGO_TARGET_DIR

echo "==> rustup targets"
rustup target add "$ANDROID_TARGET" >/dev/null
rustup target add wasm32-unknown-unknown >/dev/null

echo "==> build wasm guests (host)"
cargo build --release --target wasm32-unknown-unknown \
  --manifest-path plugins/log-hello/Cargo.toml
cargo build --release --target wasm32-unknown-unknown \
  --manifest-path plugins/busy-loop/Cargo.toml

STAGE_ROOT="$CARGO_TARGET_DIR/plugin-fixtures/android-aarch64-smoke"
rm -rf "$STAGE_ROOT"
mkdir -p "$STAGE_ROOT/log-hello" "$STAGE_ROOT/busy-loop"

copy_guest() {
  local name="$1"
  local pkg="navi_plugin_${name//-/_}"
  local release="$CARGO_TARGET_DIR/wasm32-unknown-unknown/release"
  local wasm=""
  for c in "$release/lib${pkg}.wasm" "$release/${pkg}.wasm"; do
    if [[ -f "$c" ]]; then wasm="$c"; break; fi
  done
  if [[ -z "$wasm" ]]; then
    echo "error: compiled wasm not found for $name (looked under $release)" >&2
    exit 1
  fi
  cp "plugins/$name/plugin.json" "$STAGE_ROOT/$name/plugin.json"
  cp "$wasm" "$STAGE_ROOT/$name/plugin.wasm"
}
copy_guest log-hello
copy_guest busy-loop

echo "==> cross-compile android_isolation_smoke ($ANDROID_TARGET)"
cargo build -p navi-plugin-host --bin android_isolation_smoke \
  --target "$ANDROID_TARGET" --release

ANDROID_BIN="$CARGO_TARGET_DIR/$ANDROID_TARGET/release/android_isolation_smoke"
if [[ ! -f "$ANDROID_BIN" ]]; then
  echo "error: missing $ANDROID_BIN" >&2
  exit 1
fi
echo "built $ANDROID_BIN"
file "$ANDROID_BIN" || true

if [[ "$MODE" == "build-only" ]]; then
  echo "build-only: Android aarch64 cross-compile OK; skipping execution"
  exit 0
fi

ensure_musl_cross() {
  local musl_root="$CARGO_TARGET_DIR/aarch64-linux-musl-cross"
  if [[ ! -x "$musl_root/bin/aarch64-linux-musl-gcc" ]]; then
    echo "==> fetch aarch64-linux-musl-cross toolchain (musl.cc)"
    local tmp
    tmp="$(mktemp -d)"
    curl -fsSL -o "$tmp/aarch64-linux-musl-cross.tgz" \
      https://musl.cc/aarch64-linux-musl-cross.tgz
    tar -xzf "$tmp/aarch64-linux-musl-cross.tgz" -C "$tmp"
    rm -rf "$musl_root"
    mv "$tmp/aarch64-linux-musl-cross" "$musl_root"
    rm -rf "$tmp"
  fi
  export PATH="$musl_root/bin:$PATH"
  export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-musl-gcc
}

run_qemu() {
  local qemu=""
  if command -v qemu-aarch64-static >/dev/null 2>&1; then
    qemu=qemu-aarch64-static
  elif command -v qemu-aarch64 >/dev/null 2>&1; then
    qemu=qemu-aarch64
  else
    echo "error: qemu-aarch64(-static) not found; install qemu-user-static or use --adb" >&2
    exit 1
  fi

  # Bionic user-mode needs /system/bin/linker64 (not shipped in the NDK). Modern
  # x86_64 Android emulators also refuse arm64 system images. Execute the same
  # isolation checks on a static aarch64 musl binary under QEMU so Cranelift's
  # aarch64 backend (RUSTSEC-2026-0096 class) is actually run — not host x86_64.
  ensure_musl_cross
  rustup target add "$MUSL_TARGET" >/dev/null
  echo "==> build android_isolation_smoke ($MUSL_TARGET) for QEMU aarch64 exec"
  cargo build -p navi-plugin-host --bin android_isolation_smoke \
    --target "$MUSL_TARGET" --release
  local musl_bin="$CARGO_TARGET_DIR/$MUSL_TARGET/release/android_isolation_smoke"
  echo "==> run under $qemu (aarch64 Cranelift codegen)"
  file "$musl_bin" || true
  "$qemu" "$musl_bin" \
    "$STAGE_ROOT/log-hello" \
    "$STAGE_ROOT/busy-loop"
  echo "note: Android ABI link was verified via $ANDROID_TARGET cross-compile;"
  echo "      execution used $MUSL_TARGET under QEMU (same Cranelift aarch64 ISA)."
  echo "      Use --adb on an arm64-v8a device for full Bionic process execution."
}

run_adb() {
  if ! command -v adb >/dev/null 2>&1; then
    echo "error: adb not found" >&2
    exit 1
  fi
  local serial
  serial="$(adb devices | awk '/\tdevice$/{print $1; exit}')"
  if [[ -z "$serial" ]]; then
    echo "error: no adb device online" >&2
    exit 1
  fi
  # Require arm64 device/emulator — x86_64 AVD does not validate aarch64 Cranelift.
  local abi
  abi="$(adb -s "$serial" shell getprop ro.product.cpu.abi | tr -d '\r')"
  if [[ "$abi" != "arm64-v8a" ]]; then
    echo "error: device abi=$abi; need arm64-v8a for aarch64 Cranelift smoke" >&2
    exit 1
  fi
  local remote="/data/local/tmp/navi-plugin-smoke"
  echo "==> adb push to $serial ($abi) $remote"
  adb -s "$serial" shell "rm -rf $remote && mkdir -p $remote/log-hello $remote/busy-loop"
  adb -s "$serial" push "$ANDROID_BIN" "$remote/android_isolation_smoke"
  adb -s "$serial" push "$STAGE_ROOT/log-hello/plugin.json" "$remote/log-hello/plugin.json"
  adb -s "$serial" push "$STAGE_ROOT/log-hello/plugin.wasm" "$remote/log-hello/plugin.wasm"
  adb -s "$serial" push "$STAGE_ROOT/busy-loop/plugin.json" "$remote/busy-loop/plugin.json"
  adb -s "$serial" push "$STAGE_ROOT/busy-loop/plugin.wasm" "$remote/busy-loop/plugin.wasm"
  adb -s "$serial" shell "chmod 755 $remote/android_isolation_smoke"
  echo "==> adb shell run (aarch64-linux-android binary on Bionic)"
  adb -s "$serial" shell "$remote/android_isolation_smoke $remote/log-hello $remote/busy-loop"
}

case "$MODE" in
  qemu) run_qemu ;;
  adb) run_adb ;;
esac

echo "OK: aarch64 plugin-host isolation smoke passed ($MODE)"
