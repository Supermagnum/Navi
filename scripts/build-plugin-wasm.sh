#!/usr/bin/env bash
# Build wasm32 guests from source and stage under app/src/main/assets/plugins/.
# No committed binaries — F-Droid / Gradle invoke this before assemble.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Use the workspace target/ tree (same as build-android-native.sh).
unset CARGO_TARGET_DIR

rustup target add wasm32-unknown-unknown >/dev/null

ASSETS="$ROOT/app/src/main/assets/plugins"
mkdir -p "$ASSETS"

# Remap absolute paths out of panic/debug strings so host vs CI/F-Droid wasm
# guests match (observed drift: /home/haaken/.cargo vs /home/runner/.cargo).
CARGO_HOME_DIR="${CARGO_HOME:-$HOME/.cargo}"
RUSTUP_HOME_DIR="${RUSTUP_HOME:-$HOME/.rustup}"
RUSTC_SYSROOT="$(rustc --print sysroot 2>/dev/null || true)"
WASM_RUSTFLAGS="${RUSTFLAGS:-}"
WASM_RUSTFLAGS+=" --remap-path-prefix=${CARGO_HOME_DIR}=/cargo"
WASM_RUSTFLAGS+=" --remap-path-prefix=${RUSTUP_HOME_DIR}=/rustup"
WASM_RUSTFLAGS+=" --remap-path-prefix=${ROOT}=/src"
if [[ -n "$RUSTC_SYSROOT" ]]; then
  WASM_RUSTFLAGS+=" --remap-path-prefix=${RUSTC_SYSROOT}=/rustc"
fi
export RUSTFLAGS="$WASM_RUSTFLAGS"
export CARGO_INCREMENTAL=0
echo "Reproducible wasm RUSTFLAGS enabled (remap-path-prefix)"

stage_one() {
  local crate_dir="$1"
  local asset_name="$2"
  echo "==> build $crate_dir -> assets/plugins/$asset_name"
  cargo build --release --target wasm32-unknown-unknown \
    --manifest-path "$crate_dir/Cargo.toml"
  local dir_pkg cargo_pkg
  dir_pkg="$(basename "$crate_dir" | tr '-' '_')"
  # Prefer [package].name from Cargo.toml (e.g. navi-plugin-cats under CATS-plugin/).
  cargo_pkg="$(
    sed -n 's/^name *= *"\(.*\)"/\1/p' "$crate_dir/Cargo.toml" | head -n1 | tr '-' '_'
  )"
  local release="$ROOT/target/wasm32-unknown-unknown/release"
  local wasm=""
  for candidate in \
    "$release/lib${cargo_pkg}.wasm" \
    "$release/${cargo_pkg}.wasm" \
    "$release/libnavi_plugin_${dir_pkg}.wasm" \
    "$release/navi_plugin_${dir_pkg}.wasm" \
    "$release/lib${dir_pkg}.wasm"; do
    if [[ -f "$candidate" ]]; then
      wasm="$candidate"
      break
    fi
  done
  if [[ -z "$wasm" ]]; then
    echo "error: wasm not found for $crate_dir" >&2
    ls -la "$release"/*.wasm 2>/dev/null || true
    exit 1
  fi
  local out="$ASSETS/$asset_name"
  mkdir -p "$out"
  cp -f "$crate_dir/plugin.json" "$out/plugin.json"
  cp -f "$wasm" "$out/plugin.wasm"
  echo "    staged $(wc -c < "$out/plugin.wasm") bytes"
}

stage_one plugins/right-to-roam-camping right_to_roam_camping
stage_one plugins/CATS-plugin cat
stage_one plugins/imu imu
stage_one plugins/busy-loop busy_loop
stage_one plugins/trap-guest trap_guest
stage_one plugins/memory-bomb memory_bomb

echo "OK: plugin wasm staged under $ASSETS"
