#!/usr/bin/env bash
# A3: Gjendesheim → Leirvassbu (via) → Skjolden Hotel, hiking profile.
# Usage: scripts/emulator-hiking-a3.sh [--unlocked] [--networked] --out DIR
set -euo pipefail
PKG=no.navi.app
OUT=""
UNLOCKED=false
NETWORKED=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --unlocked) UNLOCKED=true; shift ;;
    --networked) NETWORKED=true; shift ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" ]] || { echo "--out DIR required" >&2; exit 2; }
mkdir -p "$OUT"
SERIAL="${ANDROID_SERIAL:-$(adb devices | awk '/\tdevice$/{print $1; exit}')}"
adb=(adb -s "$SERIAL")

# Place-index coords (alpine_hut / hotel).
FROM_LAT=61.4943459
FROM_LON=8.8126376
VIA_LAT=61.5485603
VIA_LON=8.2476038
TO_LAT=61.4908664
TO_LON=7.598792

# Fresh process so debug intent is applied and planning_done is not missed.
"${adb[@]}" shell am force-stop "$PKG" >/dev/null 2>&1 || true
sleep 1
"${adb[@]}" logcat -c
"${adb[@]}" shell am start -n "$PKG/.MainActivity" \
  --ez navi_long_trip true \
  --ez navi_auto_plan true \
  --ez navi_eco false \
  --ez navi_restore_settings false \
  --ez navi_use_unlocked_cabins "$UNLOCKED" \
  --ez navi_use_networked_cabins "$NETWORKED" \
  --es navi_profile hiking \
  --es navi_from_name Gjendesheim \
  --ed navi_from_lat "$FROM_LAT" \
  --ed navi_from_lon "$FROM_LON" \
  --es navi_via1_name Leirvassbu \
  --ed navi_via1_lat "$VIA_LAT" \
  --ed navi_via1_lon "$VIA_LON" \
  --es navi_to_name Skjolden_Hotel \
  --ed navi_to_lat "$TO_LAT" \
  --ed navi_to_lon "$TO_LON"

echo "planning unlocked=$UNLOCKED networked=$NETWORKED …"
deadline=$((SECONDS + 600))
status=""
# Stream-filter logcat; avoid holding the full dump in a bash variable.
while (( SECONDS < deadline )); do
  if "${adb[@]}" logcat -d -v time 2>/dev/null | grep -F -q 'planning_done'; then
    status=done
    break
  fi
  if "${adb[@]}" logcat -d -v time 2>/dev/null | grep -F -q 'planning_failed'; then
    status=failed
    break
  fi
  sleep 3
done
echo "status=$status"
"${adb[@]}" logcat -d -v time > "$OUT/logcat.txt" || true
grep -E 'planning_(start|done|failed|pois)|NaviDebugIntent' "$OUT/logcat.txt" > "$OUT/plan-summary.txt" 2>/dev/null || true
# If the waiter missed the line but artifacts show a successful hiking plan, accept done.
if [[ -z "$status" || "$status" == "failed" ]]; then
  if [[ -f "$OUT/route-result.json" ]] && grep -q 'PLAN_HIKING_ROUTE' "$OUT/route-result.json" 2>/dev/null; then
    if grep -F -q 'planning_done' "$OUT/logcat.txt" 2>/dev/null; then
      status=done
      echo "status=done (recovered from logcat/artifacts)"
    fi
  fi
fi

# Clear prior pull targets so stale car-trip hops/logs are not kept.
rm -f "$OUT/route-result.json" "$OUT/route-polyline.txt" "$OUT/routing-plan.log" "$OUT/hops.json"

# Pull route artifacts from known files dirs.
for base in \
  /storage/0000-0000/Android/data/$PKG/files \
  /storage/emulated/0/Android/data/$PKG/files \
  /data/user/0/$PKG/files; do
  for name in route-result.json route-polyline.txt routing-plan.log hops.json; do
    for sub in long-trip-ui-report ""; do
      src="$base/$sub/$name"
      [[ -n "$sub" ]] || src="$base/$name"
      if "${adb[@]}" pull "$src" "$OUT/$name" >/dev/null 2>&1; then
        break
      fi
    done
  done
done

# Hiking planner often does not rewrite car hop dumps; drop unrelated leftovers.
if [[ -f "$OUT/hops.json" ]] && ! grep -q "$FROM_LAT\|$VIA_LAT\|Gjendesheim\|Leirvassbu" "$OUT/hops.json" 2>/dev/null; then
  # Car-style hops use southern Norway coords; ignore if polyline/report is hiking.
  if [[ -f "$OUT/route-result.json" ]] && grep -q 'PLAN_HIKING_ROUTE\|Hiking' "$OUT/route-result.json" 2>/dev/null; then
    rm -f "$OUT/hops.json"
  fi
fi
if [[ -f "$OUT/routing-plan.log" ]] && grep -q 'profile=Car' "$OUT/routing-plan.log" 2>/dev/null; then
  if [[ -f "$OUT/route-result.json" ]] && grep -q 'PLAN_HIKING_ROUTE\|Hiking' "$OUT/route-result.json" 2>/dev/null; then
    rm -f "$OUT/routing-plan.log"
  fi
fi

# Screenshot current map
"${adb[@]}" shell screencap -p /data/local/tmp/a3_hiking.png
"${adb[@]}" pull /data/local/tmp/a3_hiking.png "$OUT/a3_hiking.png" >/dev/null 2>&1 || true

export A3_OUT="$OUT" A3_STATUS="$status" A3_UNLOCKED="$UNLOCKED" A3_NETWORKED="$NETWORKED"
export A3_VIA_LAT="$VIA_LAT" A3_VIA_LON="$VIA_LON"
python3 - <<'PY'
import json, math, os, re, pathlib
out = pathlib.Path(os.environ["A3_OUT"])
unlocked = os.environ.get("A3_UNLOCKED", "false") == "true"
networked = os.environ.get("A3_NETWORKED", "false") == "true"
rec = {"status": os.environ.get("A3_STATUS", ""), "unlocked": unlocked, "networked": networked}
rr = out / "route-result.json"
if rr.is_file():
    d = json.loads(rr.read_text())
    rec["distance_km"] = d.get("distance_km")
    rec["eta_minutes"] = d.get("eta_minutes")
    rep = d.get("report") or ""
    rec["report_head"] = "\n".join(rep.splitlines()[:40])
    # Trail vs road signals
    segs = d.get("route_segments_json") or d.get("routeSegmentsJson") or "[]"
    try:
        arr = json.loads(segs) if isinstance(segs, str) else segs
    except Exception:
        arr = []
    on = sum(1 for s in arr if (s.get("kind") or "") == "on_trail")
    off = sum(1 for s in arr if (s.get("kind") or "") == "off_trail")
    rec["on_trail_segments"] = on
    rec["off_trail_segments"] = off
    rec["off_trail_advisory"] = d.get("off_trail_advisory") or d.get("offTrailAdvisory")
    m = re.search(r"route_mode=([^;\n]+);\s*off_trail_m=([0-9.]+)", rep)
    if m:
        rec["route_mode"] = m.group(1).strip()
        rec["off_trail_m"] = float(m.group(2))
        if off == 0 and rec["off_trail_m"] > 0:
            rec["off_trail_segments"] = 1
            rec["on_trail_segments"] = 1  # hybrid implies both
    if "ADVISORY: Off-trail" in rep and not rec["off_trail_advisory"]:
        for line in rep.splitlines():
            if line.startswith("ADVISORY:"):
                rec["off_trail_advisory"] = line[len("ADVISORY:"):].strip()
                break
    m = re.search(r"terrain_gap_densify_added=([0-9]+)", rep)
    if m:
        rec["terrain_gap_densify_added"] = int(m.group(1))
    m = re.search(r"auto_vias=([0-9]+);\s*names=([^\n]+)", rep)
    if m:
        rec["auto_vias"] = int(m.group(1))
        rec["auto_via_names"] = [x.strip() for x in m.group(2).split("|") if x.strip()]
    # Via honour: distance from via to polyline
    poly = (out / "route-polyline.txt").read_text().strip() if (out / "route-polyline.txt").is_file() else d.get("route_polyline") or ""
    via = (float(os.environ["A3_VIA_LAT"]), float(os.environ["A3_VIA_LON"]))
    best = 1e18
    pts = []
    for part in poly.split(";"):
        if not part.strip():
            continue
        a = part.split(",")
        if len(a) >= 2:
            try:
                lon, lat = float(a[0]), float(a[1])
                pts.append((lat, lon))
            except ValueError:
                pass
    def hav(a, b):
        r = 6371008.8
        p1, p2 = map(math.radians, (a[0], b[0]))
        dp = p2 - p1
        dl = math.radians(b[1] - a[1])
        h = math.sin(dp/2)**2 + math.cos(p1)*math.cos(p2)*math.sin(dl/2)**2
        return 2*r*math.asin(math.sqrt(h))
    for p in pts:
        best = min(best, hav(via, p))
    rec["via_distance_m"] = None if best > 1e17 else best
    rec["polyline_points"] = len(pts)
    # Hut/cabin POIs: break_pois_json, else enumerations + days_json overnight
    bp = d.get("break_pois_json") or d.get("breakPoisJson") or "[]"
    try:
        pois = json.loads(bp) if isinstance(bp, str) else bp
    except Exception:
        pois = []
    break_pois = [
        {"name": p.get("name"), "kind": p.get("kind"), "lat": p.get("lat"), "lon": p.get("lon")}
        for p in pois
    ]
    if not break_pois:
        enums = d.get("enumerations") or {}
        names = enums.get("rest_place_names") or []
        # Prefer kinds from logcat planning_pois if present
        kinds = []
        logcat = out / "logcat.txt"
        if logcat.is_file():
            for line in logcat.read_text(errors="replace").splitlines():
                if "planning_pois" in line and "names=" in line:
                    mk = re.search(r"names=(.+?)\s+kinds=(\S+)", line)
                    if mk:
                        names = mk.group(1).split("|")
                        kinds = mk.group(2).split("|")
                    break
        for i, name in enumerate(names):
            kind = kinds[i] if i < len(kinds) else None
            break_pois.append({"name": name, "kind": kind, "lat": None, "lon": None})
    rec["break_pois"] = break_pois
    # Overnight huts from days_json
    overnight = []
    try:
        days = json.loads(d.get("days_json") or "[]")
    except Exception:
        days = []
    for day in days:
        if not day.get("overnight_found"):
            continue
        overnight.append({
            "day_index": day.get("day_index"),
            "name": day.get("overnight_name"),
            "cabin_class": day.get("cabin_class"),
            "category": day.get("category"),
            "is_network": day.get("is_network"),
            "osm_id": day.get("osm_id"),
            "lat": day.get("lat"),
            "lon": day.get("lon"),
            "distance_km": day.get("distance_km"),
        })
    rec["overnight_huts"] = overnight
    # Highway class share from report if present
    for key in ("priority_path_share_pct", "footway_share", "path_share", "track_share"):
        m = re.search(rf"{key}=([0-9.]+)", rep)
        if m:
            rec[key] = float(m.group(1))
(out / "result.json").write_text(json.dumps(rec, indent=2) + "\n")
print(json.dumps(rec, indent=2))
PY

[[ "$status" == "done" ]]
