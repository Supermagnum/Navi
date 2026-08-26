#!/usr/bin/env bash
# Vendor a pinned Supermagnum/road-signs snapshot into Navi (copied tree).
# Usage: scripts/vendor-road-signs.sh [commit]
set -euo pipefail

COMMIT="${1:-be4dda9c6debe210a2a0d2fbbde5ed252714a7f4}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC_DIR="${TMPDIR:-/tmp}/road-signs-vendor-$$"
CORE_RS="${REPO_ROOT}/core/src/icons/road-signs"
ASSET_RS="${REPO_ROOT}/app/src/main/assets/icons/road-signs"

cleanup() { rm -rf "$SRC_DIR"; }
trap cleanup EXIT

git clone --depth 1 "https://github.com/Supermagnum/road-signs.git" "$SRC_DIR"
cd "$SRC_DIR"
git fetch --depth 1 origin "$COMMIT"
git checkout "$COMMIT"

rm -rf "$CORE_RS" "$ASSET_RS"
mkdir -p "$CORE_RS/database" "$ASSET_RS"

cp database/signs.json database/signs_en.json database/osm_tags.json "$CORE_RS/database/"

cat > "$CORE_RS/VENDOR.txt" <<EOF
Supermagnum/road-signs
commit: $(git rev-parse HEAD)
date: $(git log -1 --format=%ci)
license: NLOD 2.0 / Statens vegvesen / Kartverket (sign graphics)
upstream: https://github.com/Supermagnum/road-signs
EOF

python3 <<PY
import json
import re
import shutil
from pathlib import Path

src = Path(".")
core = Path("${CORE_RS}")
assets = Path("${ASSET_RS}")

with open(src / "database/osm_tags.json", encoding="utf-8") as f:
    data = json.load(f)

skipped_null = []
copied = 0
for entry in data["signs"]:
    svg_rel = entry.get("svg")
    if not svg_rel:
        skipped_null.append(entry["code"])
        continue
    svg_path = src / svg_rel
    if not svg_path.is_file():
        raise SystemExit(f"missing svg for {entry['code']}: {svg_path}")
    code = entry["code"]
    key = "no_sign_" + re.sub(r"[^A-Za-z0-9]+", "_", code).strip("_")
    dest_name = f"{key}.svg"
    shutil.copy2(svg_path, core / dest_name)
    shutil.copy2(svg_path, assets / dest_name)
    copied += 1

print(f"copied {copied} svgs; skipped null svg codes: {skipped_null}")
PY

echo "Vendored road-signs @ $(git rev-parse --short HEAD) -> $CORE_RS"
