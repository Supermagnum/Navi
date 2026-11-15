#!/usr/bin/env bash
# Stage CoMaps resource data into APK assets (generated at build time from pin).
# Does not commit World.mwm / WorldCoasts.mwm — those are Phase 1 downloads.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN_JSON="${ROOT}/third_party/comaps.pin.json"
SRC="${NAVI_COMAPS_SRC:-${ROOT}/third_party/comaps}"
# Prefer a warm local-artifacts cache when the gitignored clone is absent.
if [[ ! -d "$SRC/data" ]]; then
  SHA="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha"][:8])' "$PIN_JSON")"
  ALT="/mnt/2e9a1e9f-2097-408c-ab9a-a01b32f11d28/github-projects/Navi-local-artifacts/comaps-${SHA}"
  if [[ -d "$ALT/data" ]]; then
    SRC="$ALT"
  fi
fi
if [[ ! -d "$SRC/data" ]]; then
  "$ROOT/scripts/fetch-comaps.sh"
  SRC="${NAVI_COMAPS_SRC:-${ROOT}/third_party/comaps}"
fi

OUT="${ROOT}/app/src/main/assets/comaps-data"
mkdir -p "$OUT"

# Routing / classificator / country-info minimum (no drape styles required for
# RoutesBuilder headless routing). Keep the list explicit for F-Droid review.
FILES=(
  classificator.txt
  types.txt
  visibility.txt
  countries.txt
  packed_polygons.bin
  categories.txt
  colors.txt
  patterns.txt
)

MANIFEST="$OUT/MANIFEST.txt"
: >"$MANIFEST"
echo "# CoMaps resource assets staged from $SRC" >>"$MANIFEST"
echo "# pin=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha"])' "$PIN_JSON")" >>"$MANIFEST"
echo "# generated=$(date -u +%Y-%m-%dT%H:%M:%SZ)" >>"$MANIFEST"

total=0
stage_one() {
  local f="$1"
  local src="$SRC/data/$f"
  [[ -f "$src" ]] || { echo "error: missing $src" >&2; exit 1; }
  cp -f "$src" "$OUT/$f"
  local sz
  sz=$(stat -c%s "$OUT/$f")
  total=$((total + sz))
  printf '%s\t%s\n' "$f" "$sz" | tee -a "$MANIFEST"
}

for f in "${FILES[@]}"; do
  stage_one "$f"
done

# RoutesBuilder / map style reader pulls vehicle-specific drules at init
# (e.g. drules_proto_walking_light.bin). Bundle the full proto set from pin.
shopt -s nullglob
for src in "$SRC/data"/drules_proto*.bin; do
  stage_one "$(basename "$src")"
done
shopt -u nullglob

printf 'TOTAL_BYTES\t%s\n' "$total" | tee -a "$MANIFEST"
echo "Staged CoMaps data assets under $OUT ($total bytes)"
