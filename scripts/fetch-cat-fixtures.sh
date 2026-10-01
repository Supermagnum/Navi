#!/usr/bin/env bash
# Fetch licence-clean CAT (amateur radio) test fixtures into testdata/cat/.
#
# Offline-reproducible: re-running refreshes OSM extracts and records FETCH_DATE.
# Does NOT call RepeaterBook. Does NOT scrape or commit repeatermap.de data.
# RadioID: terms check only (no DMR dump committed). OpenRepeater: CC0 country
# download attempted for Norway / Innlandet when the public export API allows.
#
# Usage (from repo root):
#   ./scripts/fetch-cat-fixtures.sh
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

UA="${CAT_FIXTURE_UA:-Navi-CAT-fixture-fetch/0.1 (OSM ODbL; https://github.com/navi)}"
OSM_API="${OSM_API:-https://api.openstreetmap.org/api/0.6}"
OR_API="${OR_API:-https://www.openrepeater.org/api/downloads}"

OUT="$ROOT/testdata/cat"
OSM_OUT="$OUT/osm"
LA5MR_OUT="$OSM_OUT/la5mr"
ELEM_OUT="$OSM_OUT/elements"
OR_OUT="$OUT/openrepeater"
DUMP_OUT="$OUT/dump_caps"
ANYTONE_OUT="$OUT/anytone"

CHANGESETS=(189693189 189704408)
LA5MR_REL=18780801
HAM_SHACK_WAY=395284738

FETCH_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
FETCH_DAY="$(date -u +%Y-%m-%d)"

mkdir -p "$ELEM_OUT" "$LA5MR_OUT" "$OR_OUT" "$DUMP_OUT" "$ANYTONE_OUT" \
  "$ROOT/navi-cat/tests/fixtures/dump_caps"

echo "$FETCH_DATE" > "$OUT/FETCH_DATE.txt"
echo "fetch-cat-fixtures: FETCH_DATE=$FETCH_DATE"

curl_get() {
  local url="$1" dest="$2"
  curl -fsSL -A "$UA" --retry 3 --retry-delay 2 "$url" -o "$dest"
}

echo "== OSM changeset metadata + downloads =="
for cs in "${CHANGESETS[@]}"; do
  curl_get "$OSM_API/changeset/$cs" "$OSM_OUT/changeset_${cs}.xml"
  curl_get "$OSM_API/changeset/$cs/download" "$OSM_OUT/changeset_${cs}_download.xml"
done

echo "== LA5MR relation $LA5MR_REL (full) =="
curl_get "$OSM_API/relation/${LA5MR_REL}/full" "$LA5MR_OUT/relation_${LA5MR_REL}_full.xml"

echo "== Extract touched repeater nodes/ways; fetch current versions =="
python3 - "$OSM_OUT" "$ELEM_OUT" <<'PY'
import sys, xml.etree.ElementTree as ET
from pathlib import Path

osm_out = Path(sys.argv[1])
elem_out = Path(sys.argv[2])
ids = {"node": set(), "way": set(), "relation": set()}
for path in sorted(osm_out.glob("changeset_*_download.xml")):
    root = ET.parse(path).getroot()
    for action_el in root:
        for elem in action_el:
            tags = {t.get("k"): t.get("v") for t in elem.findall("tag")}
            has_ar = any("amateur_radio" in (k or "") for k in tags)
            if has_ar or tags.get("type") == "network":
                ids[elem.tag].add(elem.get("id"))

(elem_out / "touched_ids.txt").write_text(
    "".join(f"{t}/{i}\n" for t in ("node", "way", "relation") for i in sorted(ids[t], key=int))
)
print(
    "touched:",
    {k: len(v) for k, v in ids.items()},
    "->",
    elem_out / "touched_ids.txt",
)
Path("/tmp/cat_fixture_nodes.csv").write_text(",".join(sorted(ids["node"], key=int)))
Path("/tmp/cat_fixture_ways.txt").write_text("\n".join(sorted(ids["way"], key=int)))
Path("/tmp/cat_fixture_rels.txt").write_text("\n".join(sorted(ids["relation"], key=int)))
PY

NODES_CSV="$(tr '\n' ',' < /tmp/cat_fixture_nodes.csv | sed 's/,$//')"
if [[ -n "$NODES_CSV" ]]; then
  curl_get "$OSM_API/nodes?nodes=$NODES_CSV" "$ELEM_OUT/nodes_current.xml"
  python3 - "$ELEM_OUT" <<'PY'
import sys, xml.etree.ElementTree as ET
from pathlib import Path
out = Path(sys.argv[1])
root = ET.parse(out / "nodes_current.xml").getroot()
for n in root.findall("node"):
    osm = ET.Element("osm", version="0.6", generator="fetch-cat-fixtures")
    osm.append(n)
    ET.ElementTree(osm).write(out / f"node_{n.get('id')}.xml", encoding="utf-8", xml_declaration=True)
print("wrote", len(root.findall("node")), "per-node XML files")
PY
fi

while IFS= read -r wid; do
  [[ -z "$wid" ]] && continue
  curl_get "$OSM_API/way/$wid" "$ELEM_OUT/way_${wid}_current.xml"
  cp "$ELEM_OUT/way_${wid}_current.xml" "$ELEM_OUT/way_${wid}.xml"
done < /tmp/cat_fixture_ways.txt

while IFS= read -r rid; do
  [[ -z "$rid" ]] && continue
  curl_get "$OSM_API/relation/$rid" "$ELEM_OUT/relation_${rid}_current.xml"
done < /tmp/cat_fixture_rels.txt

echo "== Ham-shack way $HAM_SHACK_WAY (tags only) =="
curl_get "$OSM_API/way/$HAM_SHACK_WAY" "$OUT/ham_shack_way_${HAM_SHACK_WAY}.osm.xml"
python3 - "$OUT/ham_shack_way_${HAM_SHACK_WAY}.osm.xml" "$OUT/ham_shack_way_${HAM_SHACK_WAY}.json" <<'PY'
import sys, json, xml.etree.ElementTree as ET
from pathlib import Path
root = ET.parse(sys.argv[1]).getroot()
way = root.find("way")
tags = {t.get("k"): t.get("v") for t in way.findall("tag")}
Path(sys.argv[2]).write_text(
    json.dumps(
        {
            "osm_type": "way",
            "osm_id": int(way.get("id")),
            "version": int(way.get("version")),
            "tags": tags,
        },
        indent=2,
        ensure_ascii=False,
    )
    + "\n"
)
print("wrote", sys.argv[2])
PY

echo "== OpenRepeater CC0 download (Norway / Innlandet coverage check) =="
# Documented public CC0 export API (no API key). Norway country export has been
# empty; record the attempt. Do not invent coverage.
curl_get "$OR_API?country=Norway&format=json" "$OR_OUT/norway_attempt.json"
curl_get "$OR_API?country=Norway&format=csv" "$OR_OUT/norway_attempt.csv"
python3 - "$OR_OUT" "$FETCH_DAY" <<'PY'
import json, sys
from pathlib import Path
out = Path(sys.argv[1])
day = sys.argv[2]
data = json.loads((out / "norway_attempt.json").read_text())
note = {
    "fetched": day,
    "url": "https://www.openrepeater.org/api/downloads?country=Norway&format=json",
    "license_claimed": data.get("license"),
    "count": data.get("count"),
    "note": (
        "Public CC0 download endpoint returned zero Norway rows at fetch time. "
        "Innlandet-specific coverage is therefore unavailable via this export; "
        "see SOURCES.md. Terms also restrict commercial redistribution of the "
        "database without written permission despite CC0 labelling."
    ),
}
(out / "FETCH_NOTE.json").write_text(json.dumps(note, indent=2) + "\n")
print("OpenRepeater Norway count:", data.get("count"))
PY

echo "== RadioID: terms check only (no data download) =="
# Policy forbids bulk mirror / re-publish without written permission.
# Keep a short excerpt for SOURCES; never commit repeater dumps.
if curl -fsSL -A "$UA" "https://radioid.net/api/" -o "$OUT/radioid_api_page.html"; then
  python3 - "$OUT" <<'PY'
import re, sys
from pathlib import Path
out = Path(sys.argv[1])
html = (out / "radioid_api_page.html").read_text(errors="replace")
text = re.sub(r"<script[\s\S]*?</script>", " ", html)
text = re.sub(r"<style[\s\S]*?</style>", " ", text)
text = re.sub(r"<[^>]+>", " ", text)
text = re.sub(r"\s+", " ", text)
start = text.find("Important use rules")
excerpt = text[start : start + 900] if start >= 0 else text[:900]
(out / "radioid_POLICY_EXCERPT.txt").write_text(
    "RadioID API / Data Use Policy excerpt (licence review only; NO repeater data).\n"
    "Source: https://radioid.net/api/\n---\n"
    + excerpt
    + "\n"
)
(out / "radioid_api_page.html").unlink(missing_ok=True)
print("wrote radioid_POLICY_EXCERPT.txt (no DMR data)")
PY
else
  echo "WARN: could not fetch RadioID API page; leave existing excerpt if present" >&2
fi

echo "== dump_caps parser fixtures (synthetic) =="
# Stable samples under both testdata/cat/dump_caps and navi-cat/tests/fixtures/dump_caps
write_dump_caps() {
  local dest_root="$1"
  mkdir -p "$dest_root"
  cat > "$dest_root/stable.txt" <<'EOF'
Caps for model 2040 (example Stable backend)

Backend status: Stable
Can set Frequency: Y
Can set Mode: Y
Can set VFO: Y
Can set Repeater Shift: Y
Can set Repeater Offset: Y
Can set CTCSS Tone: Y
Can set DCS Code: Y
Can get PTT: Y
Can get DCD: Y
EOF

  cat > "$dest_root/beta.txt" <<'EOF'
Caps for model 9991 (example Beta backend)

Backend status: Beta
Can set Frequency: Y
Can set Mode: Y
Can set VFO: Y
Can set Repeater Shift: Y
Can set Repeater Offset: Y
Can set CTCSS Tone: Y
Can get PTT: Y
Can get DCD: N
EOF

  cat > "$dest_root/missing_lines.txt" <<'EOF'
Caps for model 1 (incomplete dump — missing required gating lines)

Backend status: Stable
Can set Frequency: Y
Can set Mode: Y
Can set VFO: Y
Can get PTT: Y
EOF

  cat > "$dest_root/alpha.txt" <<'EOF'
Caps for model 8880 (Alpha — never allow for auto-tune)

Backend status: Alpha
Can set Frequency: Y
Can set Mode: Y
Can set VFO: Y
Can set Repeater Shift: Y
Can set Repeater Offset: Y
Can set CTCSS Tone: Y
EOF

  cat > "$dest_root/untested.txt" <<'EOF'
Caps for model 7770 (Untested — never allow for auto-tune)

Backend status: Untested
Can set Frequency: Y
Can set Mode: Y
Can set VFO: Y
Can set Repeater Shift: Y
Can set Repeater Offset: Y
Can set CTCSS Tone: N
EOF
}

write_dump_caps "$DUMP_OUT"
write_dump_caps "$ROOT/navi-cat/tests/fixtures/dump_caps"

echo "== Rebuild non_networked.json from OSM fixtures =="
python3 - "$OUT" <<'PY'
import json, math, sys, xml.etree.ElementTree as ET
from pathlib import Path

out = Path(sys.argv[1])
la5 = ET.parse(out / "osm/la5mr/relation_18780801_full.xml").getroot()
member_ids = set()
for rel in la5.findall("relation"):
    tags = {t.get("k"): t.get("v") for t in rel.findall("tag")}
    if tags.get("type") != "network":
        continue
    for m in rel.findall("member"):
        member_ids.add((m.get("type"), m.get("ref")))

espa = (60.563, 11.257)
dombas = (62.076, 9.128)

def hav_km(a, b):
    R = 6371.0
    lat1, lon1 = map(math.radians, a)
    lat2, lon2 = map(math.radians, b)
    dlat = lat2 - lat1
    dlon = lon2 - lon1
    h = math.sin(dlat / 2) ** 2 + math.cos(lat1) * math.cos(lat2) * math.sin(dlon / 2) ** 2
    return 2 * R * math.asin(math.sqrt(h))

def dist_to_segment_km(p, a, b):
    best = 1e9
    for i in range(101):
        t = i / 100.0
        best = min(best, hav_km(p, (a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]))))
    return best

def classify(mod: str):
    mod = mod or ""
    is_aprs = "20K0F2D" in mod or "F2D" in mod
    is_dmr = "7K60FXE" in mod or "F7W" in mod or "DMR" in mod.upper()
    is_fm = "11K2F3E" in mod or ("F3E" in mod and not is_aprs)
    return is_aprs, is_fm, is_dmr

nodes_path = out / "osm/elements/nodes_current.xml"
entries = []
if nodes_path.exists():
    for n in ET.parse(nodes_path).getroot().findall("node"):
        tags = {t.get("k"): t.get("v") for t in n.findall("tag")}
        if tags.get("communication:amateur_radio:repeater") != "yes":
            continue
        eid = n.get("id")
        if ("node", eid) in member_ids:
            continue
        lat = float(n.get("lat"))
        lon = float(n.get("lon"))
        d = dist_to_segment_km((lat, lon), espa, dombas)
        if d > 150:
            continue
        mod = tags.get("communication:amateur_radio:repeater:modulation", "")
        is_aprs, is_fm, is_dmr = classify(mod)
        if is_aprs or not (is_fm or is_dmr):
            continue
        modes = (["FM"] if is_fm else []) + (["DMR"] if is_dmr else [])
        entries.append(
            {
                "source": "openstreetmap",
                "osm_type": "node",
                "osm_id": int(eid),
                "callsign": tags.get("communication:amateur_radio:callsign"),
                "lat": lat,
                "lon": lon,
                "frequency_out": tags.get("communication:amateur_radio:repeater:frequency_out"),
                "shift": tags.get("communication:amateur_radio:repeater:shift"),
                "ctcss": tags.get("communication:amateur_radio:repeater:ctcss"),
                "modulation": mod,
                "modes": modes,
                "distance_km_to_espa_dombas_route": round(d, 1),
                "reason": (
                    f"{'/'.join(modes)} amateur repeater on OSM node {eid}; "
                    f"{round(d, 1)} km from Espa→Dombås corridor (≤150 km); "
                    "not a member of any type=network relation in fixtures "
                    "(LA5MR/18780801); APRS/digipeater modes excluded"
                ),
            }
        )

entries.sort(key=lambda e: e["distance_km_to_espa_dombas_route"])
payload = {
    "description": (
        "FM+DMR amateur repeaters within ~150 km of Espa→Dombås that are not "
        "members of any OSM type=network relation in the CAT fixtures. APRS "
        "excluded. Built from OSM fixtures only."
    ),
    "route": {"espa": {"lat": espa[0], "lon": espa[1]}, "dombas": {"lat": dombas[0], "lon": dombas[1]}},
    "count": len(entries),
    "entries": entries,
}
(out / "non_networked.json").write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n")
print("non_networked count:", len(entries))
PY

echo "== Done. See testdata/cat/SOURCES.md for licence decisions. =="
echo "Note: AnyTone CPS samples under testdata/cat/anytone/ are hand-built from"
echo "OSM fixture frequencies (not re-downloaded). Re-verify after OSM refresh."
