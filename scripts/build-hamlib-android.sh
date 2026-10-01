#!/usr/bin/env bash
# Cross-compile upstream Hamlib into Android jniLibs shared libraries.
#
# Steps:
#   1. Resolve / pin Hamlib release tag (scripts/hamlib-android.lock)
#   2. Fetch source for that tag
#   3. Configure with Android NDK clang for each ABI
#   4. Disable C++/Perl/Python/Tcl/readline/libusb (Android-unneeded)
#   5. Install libhamlib.so under out/hamlib-android/<abi>/
#
# Usage:
#   ./scripts/build-hamlib-android.sh
#   HAMLIB_TAG=4.6.5 ./scripts/build-hamlib-android.sh
#
# Requires: ANDROID_NDK_HOME (or ANDROID_HOME/ndk/<ver>), curl/git, autoconf/make.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK="$ROOT/scripts/hamlib-android.lock"
OUT_ROOT="$ROOT/out/hamlib-android"
SRC_ROOT="$OUT_ROOT/src"

resolve_ndk() {
  if [[ -n "${ANDROID_NDK_HOME:-}" && -d "${ANDROID_NDK_HOME}" ]]; then
    echo "$ANDROID_NDK_HOME"
    return
  fi
  if [[ -n "${ANDROID_HOME:-}" && -d "${ANDROID_HOME}/ndk" ]]; then
    local newest
    newest="$(find "${ANDROID_HOME}/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -n 1 || true)"
    if [[ -n "$newest" ]]; then
      echo "$newest"
      return
    fi
  fi
  echo ""
}

resolve_latest_tag() {
  # Prefer GitHub API; fall back to git ls-remote; then lock / 4.6.5.
  local tag=""
  if command -v curl >/dev/null 2>&1; then
    tag="$(curl -fsSL -H 'Accept: application/vnd.github+json' \
      -H 'User-Agent: navi-build-hamlib' \
      https://api.github.com/repos/Hamlib/Hamlib/releases/latest 2>/dev/null \
      | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
      | head -n 1 || true)"
  fi
  if [[ -z "$tag" ]] && command -v git >/dev/null 2>&1; then
    tag="$(git ls-remote --tags https://github.com/Hamlib/Hamlib.git 2>/dev/null \
      | awk '{print $2}' \
      | sed 's#refs/tags/##' \
      | grep -v '\^{}' \
      | grep -E '^[0-9]+\.[0-9]+' \
      | sort -V \
      | tail -n 1 || true)"
  fi
  if [[ -z "$tag" && -f "$LOCK" ]]; then
    # shellcheck disable=SC1090
    tag="$(grep '^HAMLIB_TAG=' "$LOCK" | cut -d= -f2-)"
  fi
  echo "${tag:-4.6.5}"
}

NDK="$(resolve_ndk)"
if [[ -z "$NDK" || ! -d "$NDK" ]]; then
  echo "error: Android NDK not found." >&2
  echo "  Set ANDROID_NDK_HOME to your NDK install, e.g.:" >&2
  echo "    export ANDROID_NDK_HOME=\"\$HOME/Android/Sdk/ndk/<version>\"" >&2
  echo "  Or set ANDROID_HOME so this script can pick ndk/<version>." >&2
  exit 1
fi

NDK_VER="$(basename "$NDK")"
HAMLIB_TAG="${HAMLIB_TAG:-$(resolve_latest_tag)}"

mkdir -p "$OUT_ROOT"
{
  echo "HAMLIB_TAG=${HAMLIB_TAG}"
  echo "NDK_VERSION=${NDK_VER}"
} >"$LOCK"
echo "Pinned Hamlib tag ${HAMLIB_TAG} with NDK ${NDK_VER} -> $LOCK"

detect_host_tag() {
  local prebuilt="$NDK/toolchains/llvm/prebuilt"
  case "$(uname -s)" in
    Linux*) echo "linux-x86_64" ;;
    Darwin*)
      if [[ "$(uname -m)" == "arm64" && -d "$prebuilt/darwin-arm64" ]]; then
        echo "darwin-arm64"
      else
        echo "darwin-x86_64"
      fi
      ;;
    *)
      echo "error: unsupported host $(uname -s)" >&2
      exit 1
      ;;
  esac
}

HOST_TAG="$(detect_host_tag)"
TOOLCHAIN="$NDK/toolchains/llvm/prebuilt/$HOST_TAG"
if [[ ! -d "$TOOLCHAIN/bin" ]]; then
  echo "error: NDK toolchain not found at $TOOLCHAIN/bin" >&2
  exit 1
fi
export PATH="$TOOLCHAIN/bin:$PATH"

API_LEVEL="${ANDROID_API_LEVEL:-24}"
ABIS=(arm64-v8a armeabi-v7a x86_64)

fetch_source() {
  local dest="$SRC_ROOT/Hamlib-${HAMLIB_TAG}"
  if [[ -d "$dest" && ( -f "$dest/configure" || -f "$dest/configure.ac" ) ]]; then
    echo "Using existing source at $dest" >&2
    printf '%s' "$dest"
    return
  fi
  mkdir -p "$SRC_ROOT"
  local url="https://github.com/Hamlib/Hamlib/archive/refs/tags/${HAMLIB_TAG}.tar.gz"
  echo "Fetching $url" >&2
  curl -fsSL "$url" | tar -xz -C "$SRC_ROOT"
  if [[ ! -d "$dest" ]]; then
    local found
    found="$(find "$SRC_ROOT" -maxdepth 1 -type d -name 'Hamlib-*' | head -n 1)"
    if [[ -n "$found" && "$found" != "$dest" ]]; then
      mv "$found" "$dest"
    fi
  fi
  printf '%s' "$dest"
}

SRC="$(fetch_source)"
if [[ ! -f "$SRC/configure.ac" && ! -f "$SRC/configure" ]]; then
  echo "error: Hamlib source incomplete at $SRC" >&2
  exit 1
fi

if [[ ! -x "$SRC/configure" ]]; then
  echo "Bootstrapping autotools in $SRC"
  (cd "$SRC" && ./bootstrap || autoreconf -fi)
fi

build_abi() {
  local abi="$1"
  local triple target_flag
  case "$abi" in
    arm64-v8a)
      triple="aarch64-linux-android"
      ;;
    armeabi-v7a)
      triple="armv7a-linux-androideabi"
      ;;
    x86_64)
      triple="x86_64-linux-android"
      ;;
    *)
      echo "error: unknown ABI $abi" >&2
      exit 1
      ;;
  esac

  local cc="${triple}${API_LEVEL}-clang"
  local cxx="${triple}${API_LEVEL}-clang++"
  if [[ ! -x "$TOOLCHAIN/bin/$cc" ]]; then
    echo "error: missing compiler $TOOLCHAIN/bin/$cc" >&2
    exit 1
  fi

  local build_dir="$OUT_ROOT/build-$abi"
  local prefix="$OUT_ROOT/$abi"
  rm -rf "$build_dir"
  mkdir -p "$build_dir" "$prefix"

  echo "=== Building Hamlib ${HAMLIB_TAG} for ${abi} (API ${API_LEVEL}) ==="
  pushd "$build_dir" >/dev/null
  # Disable Android-unneeded bindings and libusb-dependent backends.
  "$SRC/configure" \
    --host="$triple" \
    --prefix="$prefix" \
    --enable-shared \
    --disable-static \
    --without-cxx-binding \
    --without-perl-binding \
    --without-python-binding \
    --without-tcl-binding \
    --without-readline \
    --without-libusb \
    CC="$cc" \
    CXX="$cxx" \
    CFLAGS="-O2 -fPIC" \
    LDFLAGS="-llog"
  make -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
  make install
  popd >/dev/null

  local so
  so="$(find "$prefix" -name 'libhamlib.so*' -type f | head -n 1 || true)"
  if [[ -z "$so" ]]; then
    echo "error: libhamlib.so not produced for $abi" >&2
    exit 1
  fi
  # Flatten a copy named libhamlib.so for jniLibs packaging.
  mkdir -p "$OUT_ROOT/jniLibs/$abi"
  cp -L "$so" "$OUT_ROOT/jniLibs/$abi/libhamlib.so"
  echo "Installed $OUT_ROOT/jniLibs/$abi/libhamlib.so"
}

for abi in "${ABIS[@]}"; do
  build_abi "$abi"
done

echo "Done. Artifacts under $OUT_ROOT/jniLibs/{arm64-v8a,armeabi-v7a,x86_64}/libhamlib.so"
echo "Lock file: $LOCK (tag=${HAMLIB_TAG}, ndk=${NDK_VER})"
