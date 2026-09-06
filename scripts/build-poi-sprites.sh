#!/usr/bin/env bash
# Download OpenStreetMap Carto POI symbols (CC0) and merge into the bundled
# Protomaps light sprite atlas. Requires: curl, python3, spreet (cargo install spreet).
#
# Usage: scripts/build-poi-sprites.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CARTO_REF="${CARTO_REF:-master}"
CARTO_BASE="https://raw.githubusercontent.com/gravitystorm/openstreetmap-carto/${CARTO_REF}/symbols"
SPRITE_DIR="${REPO_ROOT}/app/src/main/assets/map-styles/protomaps-light/sprites"
WORK="${TMPDIR:-/tmp}/navi-poi-sprites-$$"
ICONS="${WORK}/icons"
NEW_PACK="${WORK}/new"

cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

mkdir -p "$ICONS" "$NEW_PACK"

if ! command -v spreet >/dev/null 2>&1; then
  echo "error: spreet not found; install with: cargo install spreet" >&2
  exit 1
fi

# kind -> Carto symbol path (relative to symbols/)
declare -A CARTO=(
  [alcohol]="shop/alcohol.svg"
  [fuel]="amenity/fuel.svg"
  [charging_station]="amenity/charging_station.svg"
  [parking]="amenity/parking.svg"
  [hospital]="amenity/hospital.svg"
  [pharmacy]="amenity/pharmacy.svg"
  [hotel]="tourism/hotel.svg"
  [townhall]="amenity/town_hall.svg"
  [car_repair]="shop/car_repair.svg"
  [motorcycle]="shop/motorcycle.svg"
  [motorcycle_repair]="shop/motorcycle_repair.svg"
  [bicycle]="shop/bicycle.svg"
  # Carto has no shop/bicycle_repair.svg; reuse shop/bicycle.svg (see CREDITS).
  [bicycle_repair]="shop/bicycle.svg"
  [bicycle_repair_station]="shop/bicycle.svg"
)

echo "=== Download Carto POI SVGs (CC0) ==="
for kind in "${!CARTO[@]}"; do
  rel="${CARTO[$kind]}"
  url="${CARTO_BASE}/${rel}"
  dest="${ICONS}/${kind}.svg"
  if ! curl -fsSL "$url" -o "$dest"; then
    echo "error: failed to download $url for kind=$kind" >&2
    exit 1
  fi
  echo "  $kind <- $rel"
done

echo "=== Pack new icons with spreet ==="
spreet "$ICONS" "$NEW_PACK/light"
spreet --retina "$ICONS" "$NEW_PACK/light@2x"

echo "=== Merge into bundled atlas ==="
python3 "${REPO_ROOT}/scripts/merge_sprite_atlas.py" \
  --base-json "${SPRITE_DIR}/light.json" \
  --base-png "${SPRITE_DIR}/light.png" \
  --add-json "${NEW_PACK}/light.json" \
  --add-png "${NEW_PACK}/light.png" \
  --out-json "${SPRITE_DIR}/light.json" \
  --out-png "${SPRITE_DIR}/light.png"

python3 "${REPO_ROOT}/scripts/merge_sprite_atlas.py" \
  --base-json "${SPRITE_DIR}/light@2x.json" \
  --base-png "${SPRITE_DIR}/light@2x.png" \
  --add-json "${NEW_PACK}/light@2x.json" \
  --add-png "${NEW_PACK}/light@2x.png" \
  --out-json "${SPRITE_DIR}/light@2x.json" \
  --out-png "${SPRITE_DIR}/light@2x.png"

echo "Done. Updated ${SPRITE_DIR}/light*.json/png — bump BasemapStyleResolver assetEpoch and style.template.json icon-image mappings."
echo "Note: railway station (kind=station) uses Navit rail_station.svg packed separately into the atlas — not from Carto (see CREDITS.md)."
