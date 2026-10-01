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

stage_one() {
  local crate_dir="$1"
  local asset_name="$2"
  echo "==> build $crate_dir -> assets/plugins/$asset_name"
  cargo build --release --target wasm32-unknown-unknown \
    --manifest-path "$crate_dir/Cargo.toml"
  local pkg
  pkg="$(basename "$crate_dir" | tr '-' '_')"
  local release="$ROOT/target/wasm32-unknown-unknown/release"
  local wasm=""
  for candidate in \
    "$release/libnavi_plugin_${pkg}.wasm" \
    "$release/navi_plugin_${pkg}.wasm" \
    "$release/lib${pkg}.wasm"; do
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
stage_one plugins/busy-loop busy_loop
stage_one plugins/trap-guest trap_guest
stage_one plugins/memory-bomb memory_bomb

echo "OK: plugin wasm staged under $ASSETS"
