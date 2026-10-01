#!/usr/bin/env bash
# Cross-compile upstream Hamlib for Android (dynamic libhamlib.so).
# Resolves the latest stable release tag and records it in
# scripts/hamlib-android.lock together with the NDK version.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK="$ROOT/scripts/hamlib-android.lock"
OUT_JNI="$ROOT/app/src/main/jniLibs"
WORKDIR="${HAMLIB_BUILD_DIR:-$ROOT/target/hamlib-android-build}"

resolve_latest_stable_tag() {
  # Prefer GitHub API; fall back to git ls-remote.
  if command -v curl >/dev/null 2>&1; then
    local tag
    tag="$(curl -fsSL https://api.github.com/repos/Hamlib/Hamlib/releases/latest \
      | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)"
    if [[ -n "$tag" ]]; then
      echo "$tag"
      return 0
    fi
  fi
  git ls-remote --tags --refs https://github.com/Hamlib/Hamlib.git \
    | awk -F/ '{print $NF}' \
    | grep -E '^[0-9]+\.[0-9]+(\.[0-9]+)?$' \
    | sort -V \
    | tail -1
}

if [[ "${HAMLIB_LOCK_ONLY:-}" == "1" ]]; then
  TAG="$(resolve_latest_stable_tag)"
  NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
  NDK_VER="unset"
  if [[ -n "$NDK" && -d "$NDK" ]]; then NDK_VER="$(basename "$NDK")"; fi
  {
    echo "hamlib_tag=$TAG"
    echo "ndk_version=$NDK_VER"
    echo "resolved_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } >"$LOCK"
  echo "Wrote $LOCK (lock-only mode)"
  exit 0
fi

NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [[ -z "$NDK" || ! -d "$NDK" ]]; then
  echo "error: ANDROID_NDK_HOME / ANDROID_NDK_ROOT not set or missing" >&2
  echo "This script records the lock file tag even without NDK when HAMLIB_LOCK_ONLY=1." >&2
  if [[ "${HAMLIB_LOCK_ONLY:-}" == "1" ]]; then
    TAG="$(resolve_latest_stable_tag)"
    {
      echo "hamlib_tag=$TAG"
      echo "ndk_version=unset"
      echo "resolved_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } >"$LOCK"
    echo "Wrote $LOCK (lock-only mode)"
    exit 0
  fi
  exit 1
fi

TAG="$(resolve_latest_stable_tag)"
if [[ -z "$TAG" ]]; then
  echo "error: could not resolve latest Hamlib stable tag" >&2
  exit 1
fi

NDK_VER="$(basename "$NDK")"
{
  echo "hamlib_tag=$TAG"
  echo "ndk_version=$NDK_VER"
  echo "resolved_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} >"$LOCK"
echo "Resolved Hamlib $TAG (NDK $NDK_VER) → $LOCK"

mkdir -p "$WORKDIR"
SRC="$WORKDIR/Hamlib-$TAG"
if [[ ! -d "$SRC" ]]; then
  curl -fsSL "https://github.com/Hamlib/Hamlib/archive/refs/tags/${TAG}.tar.gz" \
    | tar -xz -C "$WORKDIR"
  # archive may unpack as Hamlib-<tag> or Hamlib-Hamlib-<tag>
  if [[ ! -d "$SRC" ]]; then
    SRC="$(find "$WORKDIR" -maxdepth 1 -type d -name "Hamlib*" | head -1)"
  fi
fi

build_abi() {
  local abi="$1"
  local triple="$2"
  local api=24
  local prefix="$WORKDIR/prefix-$abi"
  local build="$WORKDIR/build-$abi"
  mkdir -p "$build" "$prefix"
  local toolchain="$NDK/toolchains/llvm/prebuilt/linux-x86_64"
  export CC="$toolchain/bin/${triple}${api}-clang"
  export CXX="$toolchain/bin/${triple}${api}-clang++"
  export AR="$toolchain/bin/llvm-ar"
  export RANLIB="$toolchain/bin/llvm-ranlib"
  export STRIP="$toolchain/bin/llvm-strip"
  (
    cd "$build"
    "$SRC/configure" \
      --host="$triple" \
      --prefix="$prefix" \
      --disable-static \
      --enable-shared \
      --without-cxx-binding \
      --disable-perl-binding \
      --disable-python-binding \
      --disable-tcl-binding \
      --without-readline \
      --disable-libusb \
      || "$SRC/configure" --host="$triple" --prefix="$prefix" --disable-static --enable-shared
    make -j"$(nproc)"
    make install
  )
  local dest="$OUT_JNI/$abi"
  mkdir -p "$dest"
  cp -f "$prefix/lib/libhamlib.so" "$dest/libhamlib.so"
  echo "Installed $dest/libhamlib.so"
}

build_abi arm64-v8a aarch64-linux-android
build_abi armeabi-v7a armv7a-linux-androideabi
build_abi x86_64 x86_64-linux-android

echo "OK: Hamlib $TAG built for Android ABIs under $OUT_JNI"
echo "LGPL: ship dynamically linked .so; rebuild with this script + lock tag."
