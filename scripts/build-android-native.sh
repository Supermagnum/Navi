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

# ABIs shared with app/build.gradle.kts via gradle.properties `naviAbis`.
# Precedence for "all": NAVI_ABIS env > gradle.properties > default both 64-bit ABIs.
read_navi_abis() {
  if [[ -n "${NAVI_ABIS:-}" ]]; then
    echo "$NAVI_ABIS"
    return
  fi
  local props="$ROOT/gradle.properties"
  if [[ -f "$props" ]]; then
    local line
    line="$(grep -E '^[[:space:]]*naviAbis=' "$props" | tail -n 1 || true)"
    if [[ -n "$line" ]]; then
      echo "${line#*=}"
      return
    fi
  fi
  echo "arm64-v8a,x86_64"
}

normalize_to_abi() {
  case "$1" in
    arm64-v8a|aarch64-linux-android)
      echo "arm64-v8a"
      ;;
    x86_64|x86_64-linux-android)
      echo "x86_64"
      ;;
    *)
      return 1
      ;;
  esac
}

abi_to_triple() {
  case "$1" in
    arm64-v8a)
      echo "aarch64-linux-android"
      ;;
    x86_64)
      echo "x86_64-linux-android"
      ;;
    *)
      echo "error: unsupported ABI $1 (allowed: arm64-v8a, x86_64)" >&2
      exit 1
      ;;
  esac
}

resolve_abi_list() {
  local raw="$1"
  local -a out=()
  local part abi
  IFS=',' read -ra parts <<< "$raw"
  for part in "${parts[@]}"; do
    part="$(echo "$part" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
    [[ -z "$part" ]] && continue
    if ! abi="$(normalize_to_abi "$part")"; then
      echo "error: unsupported ABI/target '$part'" >&2
      echo "  Use: all | arm64-v8a | x86_64 | aarch64-linux-android | x86_64-linux-android" >&2
      echo "  Or set naviAbis / NAVI_ABIS to a comma-separated subset of arm64-v8a,x86_64" >&2
      exit 1
    fi
    out+=("$abi")
  done
  if [[ "${#out[@]}" -eq 0 ]]; then
    echo "error: no ABIs resolved from '$raw'" >&2
    exit 1
  fi
  printf '%s\n' "${out[@]}"
}

TARGET_ARG="${1:-all}"
PROFILE="${2:-release}"

if [[ "$TARGET_ARG" == "all" ]]; then
  mapfile -t ABI_LIST < <(resolve_abi_list "$(read_navi_abis)")
else
  mapfile -t ABI_LIST < <(resolve_abi_list "$TARGET_ARG")
fi

# Stage wasm guests from source into assets/ once (F-Droid / no committed binaries).
"$ROOT/scripts/build-plugin-wasm.sh"

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

build_one_abi() {
  local ABI_DIR="$1"
  local TARGET
  TARGET="$(abi_to_triple "$ABI_DIR")"

  echo "Building navi-ffi for $TARGET ($PROFILE) with NDK $ANDROID_NDK_HOME ($NDK_HOST_TAG)..."

  cargo build -p navi-ffi --target "$TARGET" "${CARGO_PROFILE_ARGS[@]}" --lib

  local LIB_SRC="$ROOT/target/$TARGET/$PROFILE/libnavi.so"
  local LIB_DST_DIR="$ROOT/app/src/main/jniLibs/$ABI_DIR"
  mkdir -p "$LIB_DST_DIR"

  local KOTLIN_OUT="$ROOT/app/src/main/java"
  mkdir -p "$KOTLIN_OUT"
  echo "Generating UniFFI Kotlin bindings..."
  # Bindgen must run before strip — cargo strip="symbols" removes UniFFI metadata.
  cargo run -p navi-ffi --bin uniffi-bindgen -- generate \
    --library "$LIB_SRC" \
    --language kotlin \
    --out-dir "$KOTLIN_OUT"

  # Strip after bindgen (workspace release leaves symbols so UniFFI metadata survives).
  if [[ "$PROFILE" == "release" ]]; then
    local STRIP_BIN=""
    if [[ -x "$NDK_BIN/llvm-strip" ]]; then
      STRIP_BIN="$NDK_BIN/llvm-strip"
    elif command -v llvm-strip >/dev/null 2>&1; then
      STRIP_BIN="$(command -v llvm-strip)"
    fi
    if [[ -n "$STRIP_BIN" ]]; then
      "$STRIP_BIN" --strip-unneeded "$LIB_SRC"
    else
      echo "warning: no llvm-strip found; shipping unstripped libnavi.so" >&2
    fi
  fi

  cp -f "$LIB_SRC" "$LIB_DST_DIR/libnavi.so"
  echo "Copied $LIB_SRC -> $LIB_DST_DIR/libnavi.so"
}

echo "Native ABIs: ${ABI_LIST[*]} (from naviAbis / NAVI_ABIS / CLI)"
for abi in "${ABI_LIST[@]}"; do
  build_one_abi "$abi"
done

echo "Done. Native library and Kotlin bindings are ready under app/."
