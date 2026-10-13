#!/usr/bin/env python3
"""Plan a trip via debug intent, pull polyline, run spike detector. Evidence only."""
from __future__ import annotations

import argparse
import json
import math
import re
import subprocess
import sys
import time
from pathlib import Path

UTURN_DEG = 165.0
BACKTRACK_MIN_M = 50.0
VIA_TOLERANCE_M = 75.0
OFFSET_ABS_M = 200.0
OFFSET_REL = 0.15
OFFSET_MIN_CHORD_M = 100.0
OFFSET_MAX_LEG_M = 800.0


def hav(a, b):
    r = 6371008.8
    p1, p2 = map(math.radians, (a[0], b[0]))
    dp = p2 - p1
    dl = math.radians(b[1] - a[1])
    h = math.sin(dp / 2) ** 2 + math.cos(p1) * math.cos(p2) * math.sin(dl / 2) ** 2
    return 2 * r * math.asin(math.sqrt(h))


def bearing(a, b):
    lat1, lon1 = map(math.radians, a)
    lat2, lon2 = map(math.radians, b)
    dl = lon2 - lon1
    y = math.sin(dl) * math.cos(lat2)
    x = math.cos(lat1) * math.sin(lat2) - math.sin(lat1) * math.cos(lat2) * math.cos(dl)
    return math.degrees(math.atan2(y, x)) % 360


def detect(coords, vias):
    hits = []
    for i in range(1, len(coords) - 1):
        prev, cur, nxt = coords[i - 1], coords[i], coords[i + 1]
        d1, d2 = hav(prev, cur), hav(cur, nxt)
        turn = abs((bearing(cur, nxt) - bearing(prev, cur) + 180) % 360 - 180)
        near = any(hav(cur, v) <= VIA_TOLERANCE_M for v in vias)
        if turn >= UTURN_DEG and d1 > BACKTRACK_MIN_M and d2 > BACKTRACK_MIN_M and not near:
            hits.append(
                {
                    "kind": "out_and_back",
                    "i": i,
                    "lat": cur[0],
                    "lon": cur[1],
                    "turn": turn,
                    "d1": d1,
                    "d2": d2,
                }
            )
        chord = hav(prev, nxt)
        if (
            chord >= OFFSET_MIN_CHORD_M
            and d1 <= OFFSET_MAX_LEG_M
            and d2 <= OFFSET_MAX_LEG_M
        ):
            s = (d1 + d2 + chord) * 0.5
            area = max(0.0, s * (s - d1) * (s - d2) * (s - chord)) ** 0.5
            xt = 2 * area / chord
            if xt >= OFFSET_ABS_M and xt / chord >= OFFSET_REL:
                hits.append(
                    {
                        "kind": "offset",
                        "i": i,
                        "lat": cur[0],
                        "lon": cur[1],
                        "xt": xt,
                        "chord": chord,
                    }
                )
    return hits


def _shell_quote(s: str) -> str:
    """Quote for remote `adb shell` (args are joined unquoted by adb)."""
    if re.fullmatch(r"[A-Za-z0-9_./:=+-]+", s):
        return s
    return "'" + s.replace("'", "'\\''") + "'"


def adb(*args):
    # `adb shell a b c` joins with spaces on the device; quote each token so
    # waypoint names with spaces (e.g. "Bad Bevensen") do not break am extras.
    if args and args[0] == "shell" and len(args) > 1:
        remote = " ".join(_shell_quote(a) for a in args[1:])
        return subprocess.run(
            ["adb", "shell", remote], check=False, capture_output=True, text=True
        )
    return subprocess.run(["adb", *args], check=False, capture_output=True, text=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--profile", default="car")
    ap.add_argument("--eco", action="store_true")
    ap.add_argument("--from-name", required=True)
    ap.add_argument("--from-lat", type=float, required=True)
    ap.add_argument("--from-lon", type=float, required=True)
    ap.add_argument("--to-name", required=True)
    ap.add_argument("--to-lat", type=float, required=True)
    ap.add_argument("--to-lon", type=float, required=True)
    ap.add_argument("--via-lat", type=float)
    ap.add_argument("--via-lon", type=float)
    ap.add_argument("--via-name", default="Via")
    ap.add_argument("--timeout", type=int, default=600)
    ap.add_argument("--keep-alive", action="store_true", help="do not force-stop (warm cache)")
    ap.add_argument(
        "--stay-in-country",
        action="store_true",
        help="enable stay-in-country (writes prefs + debug extra if supported)",
    )
    ap.add_argument(
        "--graph",
        default="pack",
        choices=("pack", "pbf"),
        help="navi_graph: pack-hit (default) or force local PBF cold-build",
    )
    ap.add_argument(
        "--camping",
        action="store_true",
        help="enable wild-camping plugin (navi_camping)",
    )
    ap.add_argument(
        "--attractions",
        action="store_true",
        help="enable nearby attractions / POI look-ahead (navi_attractions)",
    )
    ap.add_argument(
        "--datex",
        default=None,
        help="datex_plan_mode (e.g. live); omit to leave unset",
    )
    ap.add_argument("--avoid-motorways", action="store_true")
    ap.add_argument("--avoid-tunnels", action="store_true")
    ap.add_argument("--avoid-tolls", action="store_true")
    ap.add_argument("--avoid-ferries", action="store_true")
    ap.add_argument(
        "--total-weight-kg",
        type=float,
        default=None,
        help="debug overlay navi_total_weight_kg (e.g. 3020.4 for T6)",
    )
    ap.add_argument("--axle-weight-kg", type=float, default=None)
    ap.add_argument("--height-m", type=float, default=None)
    ap.add_argument("--width-m", type=float, default=None)
    ap.add_argument("--length-m", type=float, default=None)
    ap.add_argument(
        "--enum-wait-s",
        type=int,
        default=180,
        help="after planning_done, wait up to N s for camping/attractions enumerations",
    )
    args = ap.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    if not args.keep_alive:
        adb("shell", "am", "force-stop", "no.navi.app")
    # Wipe stale plan artifacts so a timeout cannot pull a previous trip's
    # route-result (e.g. Raufoss→Bergen leftover after a Bevensen timeout).
    pkg = "no.navi.app"
    for base in (
        f"/storage/emulated/0/Android/data/{pkg}/files/long-trip-ui-report",
        f"/storage/0000-0000/Android/data/{pkg}/files/long-trip-ui-report",
        f"/data/user/0/{pkg}/files/long-trip-ui-report",
    ):
        adb("shell", "rm", "-f", f"{base}/route-result.json", f"{base}/route-polyline.txt")
    adb("logcat", "-c")
    cmd = [
        "shell",
        "am",
        "start",
        "-n",
        "no.navi.app/.MainActivity",
        "--ez",
        "navi_long_trip",
        "true",
        "--ez",
        "navi_auto_plan",
        "true",
        "--ez",
        "navi_restore_settings",
        "false",
        "--ez",
        "navi_eco",
        "true" if args.eco else "false",
        "--es",
        "navi_profile",
        args.profile,
        "--es",
        "navi_from_name",
        args.from_name,
        "--ed",
        "navi_from_lat",
        str(args.from_lat),
        "--ed",
        "navi_from_lon",
        str(args.from_lon),
        "--es",
        "navi_to_name",
        args.to_name,
        "--ed",
        "navi_to_lat",
        str(args.to_lat),
        "--ed",
        "navi_to_lon",
        str(args.to_lon),
    ]
    if args.via_lat is not None and args.via_lon is not None:
        cmd += [
            "--es",
            "navi_via1_name",
            args.via_name,
            "--ed",
            "navi_via1_lat",
            str(args.via_lat),
            "--ed",
            "navi_via1_lon",
            str(args.via_lon),
        ]
    # Always set the extra: a sticky UI pref left ON (e.g. after Lakselv)
    # would otherwise force origin-country-only and break DE→NO long trips
    # with opaque "inf m from a road" snaps at the destination.
    cmd += [
        "--ez",
        "navi_stay_in_country",
        "true" if args.stay_in_country else "false",
    ]
    if args.camping:
        cmd += ["--ez", "navi_camping", "true"]
    if args.attractions:
        cmd += ["--ez", "navi_attractions", "true"]
    if args.datex:
        cmd += ["--es", "navi_datex", args.datex]
    if args.avoid_motorways:
        cmd += ["--ez", "navi_avoid_motorways", "true"]
    if args.avoid_tunnels:
        cmd += ["--ez", "navi_avoid_tunnels", "true"]
    if args.avoid_tolls:
        cmd += ["--ez", "navi_avoid_tolls", "true"]
    if args.avoid_ferries:
        cmd += ["--ez", "navi_avoid_ferries", "true"]
    if args.graph != "pack":
        cmd += ["--es", "navi_graph", args.graph]
    if args.total_weight_kg is not None:
        cmd += ["--ef", "navi_total_weight_kg", str(args.total_weight_kg)]
    if args.axle_weight_kg is not None:
        cmd += ["--ef", "navi_axle_weight_kg", str(args.axle_weight_kg)]
    if args.height_m is not None:
        cmd += ["--ef", "navi_height_m", str(args.height_m)]
    if args.width_m is not None:
        cmd += ["--ef", "navi_width_m", str(args.width_m)]
    if args.length_m is not None:
        cmd += ["--ef", "navi_length_m", str(args.length_m)]
    adb(*cmd)
    t0 = time.time()
    status = "timeout"
    preparing_retries = 0
    # Prefer matching THIS trip's planning_start / pendingTripPlan so a
    # concurrent sticky plan cannot steal the run. Fall back to bare
    # planning_done only after trip_grace_s if older APKs omit waypoint tokens.
    trip_token = f"waypoints={args.from_name}|{args.to_name}"
    trip_seen = False
    trip_grace_s = 45.0
    while time.time() - t0 < args.timeout:
        log = adb("logcat", "-d", "-v", "time").stdout or ""
        if (
            trip_token in log
            or f"from={args.from_name} to={args.to_name}" in log
            or (
                args.from_name in log
                and args.to_name in log
                and ("pendingTripPlan" in log or "planning_start" in log)
            )
        ):
            trip_seen = True
        accept_done = trip_seen or (time.time() - t0 >= trip_grace_s)
        if accept_done and "planning_done" in log:
            status = "done"
            break
        if trip_seen and "planning_failed" in log:
            # Ignore failures that belong to a different concurrent trip.
            failed_ours = any(
                (
                    trip_token in line
                    or (
                        args.from_name in line
                        and args.to_name in line
                        and "planning_start" in line
                    )
                    or (
                        f"{args.from_lat:.5f}" in line
                        and "planning_failed" in line
                    )
                )
                for line in log.splitlines()
                if "planning_failed" in line
                or "planning_start" in line
                or "hop_fail" in line
            )
            # Correlate: last planning_start for our trip precedes a failure.
            our_start = log.rfind(trip_token)
            if our_start < 0:
                our_start = log.rfind(f"from={args.from_name} to={args.to_name}")
            fail_at = log.rfind("planning_failed")
            failed_ours = failed_ours and our_start >= 0 and fail_at > our_start
            if not failed_ours:
                time.sleep(2)
                continue
            # Skeleton / ferry overlay builds are soft. Wait for the idle
            # builder without re-issuing the intent (that aborts the build).
            if (
                preparing_retries < 40
                and (
                    "terminate=skeleton_preparing" in log
                    or "terminate=ferry_preparing" in log
                    or "skeleton_preparing" in log
                )
            ):
                preparing_retries += 1
                time.sleep(15)
                # Re-plan only every ~2 min once skeletons may be ready.
                if preparing_retries % 8 == 0:
                    trip_seen = False
                    adb("logcat", "-c")
                    adb(*cmd)
                continue
            status = "failed"
            break
        time.sleep(2)
    wall = time.time() - t0
    bases = (
        f"/storage/emulated/0/Android/data/{pkg}/files/long-trip-ui-report",
        f"/storage/0000-0000/Android/data/{pkg}/files/long-trip-ui-report",
        f"/data/user/0/{pkg}/files/long-trip-ui-report",
    )

    def pull_artifacts():
        for base in bases:
            for name in ("route-result.json", "route-polyline.txt"):
                adb("pull", f"{base}/{name}", str(out / name))

    def route_matches_trip(path: Path) -> bool:
        """Reject leftover route-result.json from a different OD."""
        if not path.is_file():
            return False
        try:
            d = json.loads(path.read_text())
        except Exception:
            return False
        start_name = str(d.get("start") or "")
        end_name = str(d.get("end") or "")
        if start_name and end_name:
            if (
                args.from_name.lower() in start_name.lower()
                and args.to_name.lower() in end_name.lower()
            ):
                return True
        report = d.get("report") or ""
        # First chunk leg starts at the trip origin.
        m = re.search(r"chunk_leg1=([0-9.+-]+),([0-9.+-]+)\s*->", report)
        if m:
            slat, slon = float(m.group(1)), float(m.group(2))
            if hav((slat, slon), (args.from_lat, args.from_lon)) <= 2500.0:
                return True
        m = re.search(
            r"plan_inputs[^\n]*start=([0-9.+-]+),([0-9.+-]+)\s+end=([0-9.+-]+),([0-9.+-]+)",
            report,
        )
        if not m:
            m = re.search(
                r"start=([0-9.+-]+),([0-9.+-]+);\s*end=([0-9.+-]+),([0-9.+-]+)",
                report,
            )
        if m:
            slat, slon, elat, elon = map(float, m.groups())
            return (
                hav((slat, slon), (args.from_lat, args.from_lon)) <= 2500.0
                and hav((elat, elon), (args.to_lat, args.to_lon)) <= 2500.0
            )
        return False

    pull_artifacts()
    if (out / "route-result.json").is_file() and not route_matches_trip(
        out / "route-result.json"
    ):
        (out / "route-result.json").unlink(missing_ok=True)
        (out / "route-polyline.txt").unlink(missing_ok=True)
        if status == "done":
            status = "stale_result"

    # Camping / attractions fill enumerations after planning_done; wait until
    # status kind / attraction tally lands (or enum-wait timeout).
    enum_meta = {}
    if status == "done" and (args.camping or args.attractions):
        enum_t0 = time.time()
        last_key = None
        stable = 0
        while time.time() - enum_t0 < args.enum_wait_s:
            pull_artifacts()
            e = {}
            if (out / "route-result.json").is_file():
                try:
                    e = json.loads((out / "route-result.json").read_text()).get(
                        "enumerations"
                    ) or {}
                except Exception:
                    e = {}
            camp_kind = (e.get("wild_camping_status_kind") or "").strip()
            camp_n = int(e.get("wild_camping_site_count") or 0)
            attr_n = int(e.get("attraction_count") or 0)
            log = adb("logcat", "-d", "-v", "time").stdout or ""
            camp_logged = "NaviCamping" in log and "suggestAlongRoute" in log
            attr_logged = "NaviAttractions" in log and "lookahead tally" in log
            camp_ready = (not args.camping) or camp_kind or camp_logged
            attr_ready = (not args.attractions) or attr_logged or attr_n > 0
            key = (camp_n, camp_kind, attr_n, camp_ready, attr_ready)
            if camp_ready and attr_ready:
                if key == last_key:
                    stable += 1
                    if stable >= 2:
                        enum_meta = {
                            "wild_camping_site_count": camp_n,
                            "wild_camping_status_kind": camp_kind,
                            "attraction_count": attr_n,
                            "enum_wait_s": round(time.time() - enum_t0, 2),
                        }
                        break
                else:
                    stable = 0
                    last_key = key
            time.sleep(3)
        else:
            e = {}
            if (out / "route-result.json").is_file():
                try:
                    e = json.loads((out / "route-result.json").read_text()).get(
                        "enumerations"
                    ) or {}
                except Exception:
                    e = {}
            enum_meta = {
                "wild_camping_site_count": int(e.get("wild_camping_site_count") or 0),
                "wild_camping_status_kind": (e.get("wild_camping_status_kind") or ""),
                "attraction_count": int(e.get("attraction_count") or 0),
                "enum_wait_s": args.enum_wait_s,
                "enum_wait_timeout": True,
            }

    (out / "logcat.txt").write_text(adb("logcat", "-d", "-v", "time").stdout or "")
    pull_artifacts()
    adb("shell", "screencap", "-p", "/data/local/tmp/reg.png")
    adb("pull", "/data/local/tmp/reg.png", str(out / "route.png"))
    pts = []
    poly = out / "route-polyline.txt"
    if poly.is_file():
        for part in poly.read_text().split(";"):
            part = part.strip()
            if not part:
                continue
            a = part.split(",")
            if len(a) >= 2:
                pts.append((float(a[1]), float(a[0])))
    vias = [(args.from_lat, args.from_lon), (args.to_lat, args.to_lon)]
    if args.via_lat is not None:
        vias.insert(1, (args.via_lat, args.via_lon))
    spikes = detect(pts, vias) if pts else []
    distance_km = None
    enumerations = None
    if (out / "route-result.json").is_file():
        d = json.loads((out / "route-result.json").read_text())
        distance_km = d.get("distance_km")
        enumerations = d.get("enumerations")
    duration_ms = None
    for line in (out / "logcat.txt").read_text().splitlines():
        if "planning_done" in line or "planning_failed" in line:
            m = re.search(r"duration_ms=(\d+)", line)
            if m:
                duration_ms = int(m.group(1))
    rec = {
        "status": status,
        "profile": args.profile,
        "eco": args.eco,
        "stay_in_country": args.stay_in_country,
        "graph": args.graph,
        "wall_s": round(wall, 2),
        "duration_ms": duration_ms,
        "distance_km": distance_km,
        "polyline_points": len(pts),
        "spike_count": len(spikes),
        "spikes": spikes[:20],
        "from": [args.from_name, args.from_lat, args.from_lon],
        "to": [args.to_name, args.to_lat, args.to_lon],
        "enumerations": enumerations,
        "enum_meta": enum_meta or None,
    }
    (out / "result.json").write_text(json.dumps(rec, indent=2) + "\n")
    print(json.dumps(rec, indent=2))
    return 0 if status == "done" and not spikes else 1


if __name__ == "__main__":
    sys.exit(main())
