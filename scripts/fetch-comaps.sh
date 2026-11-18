#!/usr/bin/env bash
# Fetch CoMaps at the pinned tag/commit for from-source Android builds.
# Network is only used here (and for submodules). The native compile step
# must not download binaries.
#
# Primary remote is Codeberg (comaps.pin.json "repo"). When that host is
# unreachable (DNS / outage), fall back to "mirror_github" so CI can still
# stage data assets and build the SDK.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN_JSON="${ROOT}/third_party/comaps.pin.json"
DEST="${NAVI_COMAPS_SRC:-${ROOT}/third_party/comaps}"

TAG="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["tag"])' "$PIN_JSON")"
SHA="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha"])' "$PIN_JSON")"
REPO="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["repo"])' "$PIN_JSON")"
MIRROR="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("mirror_github") or "")' "$PIN_JSON")"

clone_into() {
  local url="$1"
  git clone --filter=blob:none "$url" "$DEST"
}

ensure_clone() {
  mkdir -p "$(dirname "$DEST")"
  if [[ -d "$DEST/.git" ]]; then
    return 0
  fi
  rm -rf "$DEST"
  if clone_into "$REPO"; then
    return 0
  fi
  if [[ -n "$MIRROR" ]]; then
    echo "warn: primary clone failed ($REPO); trying mirror_github ($MIRROR)" >&2
    rm -rf "$DEST"
    clone_into "$MIRROR"
    return 0
  fi
  echo "error: clone failed and no mirror_github in $PIN_JSON" >&2
  return 1
}

fetch_tags() {
  if git fetch --tags origin; then
    return 0
  fi
  if [[ -n "$MIRROR" ]]; then
    echo "warn: git fetch from origin failed; switching origin to mirror_github ($MIRROR)" >&2
    git remote set-url origin "$MIRROR"
    git fetch --tags origin
    return 0
  fi
  echo "error: git fetch failed and no mirror_github in $PIN_JSON" >&2
  return 1
}

ensure_clone
cd "$DEST"
fetch_tags
# Pin verify runs after primary (Codeberg) or mirror_github clone/fetch.
git checkout --force "$SHA"
HEAD="$(git rev-parse HEAD)"
if [[ "$HEAD" != "$SHA" ]]; then
  echo "error: CoMaps HEAD ($HEAD) does not match pin sha ($SHA) from $PIN_JSON" >&2
  exit 1
fi
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
