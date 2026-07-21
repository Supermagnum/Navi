#!/usr/bin/env bash
# Prepare on-device corridor fixture (cut extract + ensure DEM tiles exist on host).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
FIX="$ROOT/core/target/integration-fixtures"
PBF="$FIX/espa-atnbrufossen-corridor.osm.pbf"
SRC="$FIX/ostlandet-latest.osm.pbf"

if [[ ! -f "$SRC" ]]; then
  echo "error: missing $SRC — run the host corridor integration once first" >&2
  exit 1
fi

if [[ ! -f "$PBF" ]] || [[ $(stat -c%s "$PBF") -lt 1000000 ]]; then
  echo "Cutting corridor extract..."
  python3 "$ROOT/scripts/cut-corridor-extract.py" --src "$SRC" --dst "$PBF"
fi

ls -lh "$PBF"
echo "Fixture ready: $PBF"
