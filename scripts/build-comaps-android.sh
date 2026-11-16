#!/usr/bin/env bash
# Build CoMaps liborganicmaps.so from source for Navi (NDK r27c, arm64-v8a).
# Applies patches/comaps/*.patch. Does not commit or download prebuilt .so.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN_JSON="${ROOT}/third_party/comaps.pin.json"
SRC="${NAVI_COMAPS_SRC:-${ROOT}/third_party/comaps}"
BUILD="${NAVI_COMAPS_BUILD:-${ROOT}/out/comaps-build-arm64}"
ABI="${NAVI_COMAPS_ABI:-arm64-v8a}"
STAGE_DIR="${ROOT}/app/src/main/jniLibs/${ABI}"

NDK_VER="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["ndk_version"])' "$PIN_JSON")"
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  ANDROID_NDK_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}/ndk/${NDK_VER}"
fi
export ANDROID_NDK_HOME
[[ -d "$ANDROID_NDK_HOME" ]] || { echo "error: NDK not at $ANDROID_NDK_HOME" >&2; exit 1; }

if [[ ! -d "$SRC/.git" ]]; then
  "$ROOT/scripts/fetch-comaps.sh"
fi

# Reset to pin then apply quilt series (idempotent via git checkout of patched files).
SHA="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha"])' "$PIN_JSON")"
cd "$SRC"
git checkout --force "$SHA"
git clean -fd -- libs/routing/navi_wetland_cost.cpp libs/routing/navi_wetland_cost.hpp \
  android/sdk/src/main/cpp/navi 2>/dev/null || true
while IFS= read -r patch || [[ -n "$patch" ]]; do
  [[ -z "$patch" || "$patch" =~ ^# ]] && continue
  echo "Applying $patch"
  git apply --whitespace=nowarn "${ROOT}/patches/comaps/${patch}"
done < "${ROOT}/patches/comaps/series"

# The C ABI shim has one source of truth: native/comaps_shim. Patch 0002 only creates the file in the
# CoMaps tree (an older copy); the repository version (Phase 5 search additions plus the drape changes that
# patch 0004 also carries) is installed over it so a build from the pinned commit is the same as the one
# the Rust side (core/src/mwm/*.rs extern declarations) expects.
install -m 0644 "${ROOT}/native/comaps_shim/navi_comaps.cpp" "${ROOT}/native/comaps_shim/navi_comaps.h" \
  "${SRC}/android/sdk/src/main/cpp/navi/"

mkdir -p "$BUILD"
cmake -S "$SRC" -B "$BUILD" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_TOOLCHAIN_FILE="${ANDROID_NDK_HOME}/build/cmake/android.toolchain.cmake" \
  -DANDROID_ABI="$ABI" \
  -DANDROID_PLATFORM=android-24 \
  -DANDROID_STL=c++_static \
  -DANDROID_TOOLCHAIN=clang \
  -DSKIP_TESTS=ON \
  -DSKIP_TOOLS=ON \
  -DUSE_CCACHE=ON

cmake --build "$BUILD" --target organicmaps -j"$(nproc)"

SO="$(find "$BUILD" -name 'liborganicmaps.so' | head -n 1)"
[[ -n "$SO" && -f "$SO" ]] || { echo "error: liborganicmaps.so not produced" >&2; exit 1; }

# Stage for local debug installs only. F-Droid / release CI must build then
# package without committing this file (same rule as libnavi.so).
if [[ "${NAVI_COMAPS_STAGE:-1}" == "1" ]]; then
  mkdir -p "$STAGE_DIR"
  cp -f "$SO" "$STAGE_DIR/liborganicmaps.so"
  "${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip" \
    --strip-unneeded "$STAGE_DIR/liborganicmaps.so" || true
  ls -lh "$STAGE_DIR/liborganicmaps.so"
  sha256sum "$STAGE_DIR/liborganicmaps.so"
fi

echo "CoMaps organicmaps built for $ABI"
