#!/usr/bin/env bash
# Fetch CoMaps at the pinned tag/commit for from-source Android builds.
# Network is only used here (and for submodules). The native compile step
# must not download binaries.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN_JSON="${ROOT}/third_party/comaps.pin.json"
DEST="${NAVI_COMAPS_SRC:-${ROOT}/third_party/comaps}"

TAG="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["tag"])' "$PIN_JSON")"
SHA="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha"])' "$PIN_JSON")"
REPO="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["repo"])' "$PIN_JSON")"

mkdir -p "$(dirname "$DEST")"
if [[ ! -d "$DEST/.git" ]]; then
  git clone --filter=blob:none "$REPO" "$DEST"
fi
cd "$DEST"
git fetch --tags origin
git checkout --force "$SHA"
test "$(git rev-parse HEAD)" = "$SHA"
git submodule update --init --recursive --depth 1

export PATH="${NAVI_HOST_TOOLS_BIN:-}:${PATH}"
export SKIP_MAP_DOWNLOAD="${SKIP_MAP_DOWNLOAD:-1}"
# The drape symbol atlases are generated from SVG (CoMaps' own and Navi's) by scripts/generate-comaps-symbols.py
# (rsvg-convert + Pillow) when Gradle builds :comaps-sdk; CoMaps' Qt skin_generator is not needed.
export SKIP_GENERATE_SYMBOLS="${SKIP_GENERATE_SYMBOLS:-1}"
./configure.sh

echo "CoMaps ready at $DEST ($TAG / $SHA)"
git rev-parse HEAD
git submodule status | head -40
