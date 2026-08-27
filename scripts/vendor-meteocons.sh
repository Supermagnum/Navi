#!/usr/bin/env bash
# Vendor Meteocons SVG static + animated sets into plugins/weather/.
# Downloads npm tarballs only — does not add npm dependencies to the repo.
# Usage: scripts/vendor-meteocons.sh [svg-static-version] [svg-version]
set -euo pipefail

STATIC_VER="${1:-0.1.0}"
ANIM_VER="${2:-0.1.0}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRATCH="${TMPDIR:-/tmp}/meteocons-vendor-$$"
STATIC_PKG="@meteocons/svg-static"
ANIM_PKG="@meteocons/svg"
DEST="${REPO_ROOT}/plugins/weather"

cleanup() { rm -rf "$SCRATCH"; }
trap cleanup EXIT

mkdir -p "$SCRATCH"
cd "$SCRATCH"

fetch_and_extract() {
  local pkg="$1" ver="$2" out="$3"
  local url="https://registry.npmjs.org/${pkg}/-/$(basename "$pkg")-${ver}.tgz"
  curl -fsSL -o pkg.tgz "$url"
  mkdir -p "$out"
  tar -xzf pkg.tgz -C "$out"
}

fetch_and_extract "$STATIC_PKG" "$STATIC_VER" static
fetch_and_extract "$ANIM_PKG" "$ANIM_VER" animated

rm -rf "${DEST}/icons" "${DEST}/animated-icons"
mkdir -p "${DEST}/icons" "${DEST}/animated-icons"

copy_tree() {
  local src="$1" dst="$2"
  for style in fill flat line monochrome; do
    mkdir -p "${dst}/${style}"
    cp "${src}/package/${style}"/*.svg "${dst}/${style}/"
  done
  cp "${src}/package/manifest.json" "${dst}/manifest.json"
}

copy_tree "$SCRATCH/static" "${DEST}/icons"
copy_tree "$SCRATCH/animated" "${DEST}/animated-icons"

# Animated npm package omits LICENSE; static package includes MIT text.
cp "${SCRATCH}/static/package/LICENSE" "${DEST}/icons/LICENSE"
cp "${SCRATCH}/static/package/LICENSE" "${DEST}/animated-icons/LICENSE"

cat > "${DEST}/VENDOR.txt" <<EOF
Meteocons weather icon sets (Bas Milius)
static: ${STATIC_PKG}@${STATIC_VER}
animated: ${ANIM_PKG}@${ANIM_VER}
license: MIT (see icons/LICENSE and animated-icons/LICENSE)
upstream: https://github.com/basmilius/meteocons
refresh: scripts/vendor-meteocons.sh ${STATIC_VER} ${ANIM_VER}
EOF

echo "Vendored Meteocons into ${DEST}"
du -sh "${DEST}/icons" "${DEST}/animated-icons" "${DEST}"

python3 "${REPO_ROOT}/scripts/generate-weather-icons-reference.py"
