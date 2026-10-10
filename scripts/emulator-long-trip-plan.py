#!/usr/bin/env python3
"""Plan one long trip on a running emulator through MainActivity debug extras.

Never force-stops the app. Sends vias as navi_via{i}_lat / navi_via{i}_lon /
navi_via{i}_name, then reads back from the plan log what the plan received:

- planner `plan_inputs` line (profile, eco, avoid flags, DATEX mode, vias and
  their coordinates),
- app `plan_settings` line (eco, camping plugin, avoid flags),
- app `planning_start` line (profile, waypoint count).

The run is rejected (exit 2) when any of these differ from what was sent.
Results (km, ferries, via distance, hops, plan peak memory) go to
`<out>/result.json` beside the pulled plan artifacts, together with the place
index on the pack volume (path, size, quick_check, rows per region), read
before the plan with `sqlite3 -readonly`.

Usage:
  scripts/emulator-long-trip-plan.py bevensen --datex none --out DIR
"""

import argparse
import json
import math
import os
import pathlib
import re
import subprocess
import sys
import time

PKG = "no.navi.app"
FILES_DIRS = (
    "/storage/0000-0000/Android/data/no.navi.app/files",
    "/storage/emulated/0/Android/data/no.navi.app/files",
)

TRIPS = {
    "bevensen": {
        "from": (53.079686, 10.587198, "Bevensen"),
        "vias": [(61.8691419, 9.1055130, "Vagavegen 80")],
        "to": (61.4433766, 7.4614016, "Dalsoren Camping"),
    },
    "aga": {
        "from": (60.82718, 11.30278, "Brenneriroa"),
        "vias": [],
        "to": (60.29870, 6.60322, "Aga"),
    },
    "floro": {
        "from": (60.82712, 11.30249, "Brenneriroa"),
        "vias": [(62.013569, 7.630359, "Grotli")],
        "to": (61.60145, 5.02658, "Floro"),
    },
    "elsa": {
        "from": (69.9742, 29.63342, "Elsa"),
        "vias": [],
        "to": (59.80326, 9.39866, "Sjuvass"),
    },
    # Place-index Østlandet: Hamar place:city, Stange place:town.
    "hamar_stange": {
        "from": (60.794721, 11.068055, "Hamar"),
        "vias": [],
        "to": (60.717675, 11.192379, "Stange"),
    },
    # Place-index Østlandet: Hamar place:city, Lillehammer place:town.
    "hamar_lillehammer": {
        "from": (60.794721, 11.068055, "Hamar"),
        "vias": [],
        "to": (61.114545, 10.467007, "Lillehammer"),
    },
    # Place-index Østlandet: Oslo place:city, Lillestrøm place:town.
    "oslo_lillestrom": {
        "from": (59.913330, 10.738970, "Oslo"),
        "vias": [],
        "to": (59.955924, 11.049112, "Lillestrom"),
    },
    # Hamburg place:city to Bergedorf, ~18 km across the city-state pack.
    "hamburg_bergedorf": {
        "from": (53.550341, 9.993682, "Hamburg"),
        "vias": [],
        "to": (53.48611, 10.23278, "Bergedorf"),
    },
    # Copenhagen place:city to Taastrup, ~19 km across the dense Zealand tile.
    "copenhagen_taastrup": {
        "from": (55.676098, 12.568337, "Copenhagen"),
        "vias": [],
        "to": (55.6517, 12.2922, "Taastrup"),
    },
    "k_60_276_10_816": {
        "from": (60.27656, 10.81650, "From"),
        "vias": [],
        "to": (59.80326, 9.39866, "To"),
    },
    "raufoss_bergen": {
        "from": (60.7277483, 10.6109403, "Raufoss"),
        "vias": [],
        "to": (60.388114, 5.333857, "Bergen"),
    },
}

DATEX_MODES = {"none": "None", "saved": "Saved", "live": "Live"}

# Fixed list for the gate emulator search check. Each query must return this
# place name as the top hit (case-insensitive; å/ä/ö fold to a/o).
SEARCH_EXPECT = [
    ("Oslo", "Oslo"),
    ("Hamar", "Hamar"),
    ("Lillehammer", "Lillehammer"),
    ("Luleå", "Luleå"),
    ("Kiruna", "Kiruna"),
    ("Piteå", "Piteå"),
    ("Falun", "Falun"),
    ("Mora", "Mora"),
    ("Aga", "Aga"),
    ("Bergen", "Bergen"),
    ("Raufoss", "Raufoss"),
    ("Utne", "Utne"),
]

env = os.environ.copy()
sdk = pathlib.Path.home() / "Android/Sdk/platform-tools"
env["PATH"] = f"{sdk}:{env.get('PATH', '')}"


def serial():
    want = os.environ.get("ANDROID_SERIAL") or os.environ.get("NAVI_ADB_SERIAL")
    r = subprocess.run(["adb", "devices"], env=env, capture_output=True, text=True, timeout=30)
    found = []
    for ln in (r.stdout or "").splitlines():
        parts = ln.split("\t")
        if len(parts) == 2 and parts[1] == "device":
            found.append(parts[0])
    if want:
        if want in found:
            return want
        sys.exit(f"adb device {want} not ready (have {found})")
    if found:
        return found[0]
    sys.exit("no adb device")


SERIAL = None


def adb(*args, timeout=180):
    return subprocess.run(
        ["adb", "-s", SERIAL, *args], env=env, capture_output=True, text=True, timeout=timeout
    )


def log(msg):
    print(f"{time.strftime('%H:%M:%S')} {msg}", flush=True)


def pid():
    return (adb("shell", "pidof", PKG).stdout or "").strip()


def ensure_running():
    if pid():
        return
    adb("shell", "am", "start", "-n", f"{PKG}/.MainActivity")
    for _ in range(30):
        time.sleep(1)
        if pid():
            return
    sys.exit("app did not start")


def proc_kb(key):
    p = pid()
    if not p:
        return None
    out = adb("shell", f"grep {key} /proc/{p}/status").stdout or ""
    m = re.search(rf"{key}:\s+(\d+)", out)
    return int(m.group(1)) if m else None


def reset_peak():
    """Reset VmHWM to current RSS (clear_refs 4). Value 5 is file-backed refs."""
    p = pid()
    r = adb("shell", f"echo 4 > /proc/{p}/clear_refs && echo ok")
    if "ok" not in (r.stdout or ""):
        r = adb("shell", f"su 0 sh -c 'echo 4 > /proc/{p}/clear_refs' && echo ok")
    return "ok" in (r.stdout or "")


def haversine_m(a, b):
    r = 6371008.8
    la1, lo1, la2, lo2 = map(math.radians, (a[0], a[1], b[0], b[1]))
    h = math.sin((la2 - la1) / 2) ** 2 + math.cos(la1) * math.cos(la2) * math.sin((lo2 - lo1) / 2) ** 2
    return 2 * r * math.asin(math.sqrt(h))


def point_segment_m(p, a, b):
    k = math.cos(math.radians(p[0]))
    m = 111195.0
    ax, ay = (a[1] - p[1]) * k * m, (a[0] - p[0]) * m
    bx, by = (b[1] - p[1]) * k * m, (b[0] - p[0]) * m
    dx, dy = bx - ax, by - ay
    l2 = dx * dx + dy * dy
    t = max(0.0, min(1.0, -(ax * dx + ay * dy) / l2)) if l2 > 0 else 0.0
    return math.hypot(ax + t * dx, ay + t * dy)


def clear_previous_plan():
    for base in FILES_DIRS:
        adb(
            "shell",
            f"rm -f {base}/routing-hops-partial.json {base}/routing-hops.json "
            f"{base}/long-trip-ui-report/route-result.json "
            f"{base}/long-trip-ui-report/route-polyline.txt "
            f"{base}/long-trip-ui-report/hops.json 2>/dev/null; true",
        )


def start_plan(trip, avoid_ferries, datex, long_trip=True):
    f_lat, f_lon, f_name = trip["from"]
    t_lat, t_lon, t_name = trip["to"]
    args = [
        "shell", "am", "start", "-n", f"{PKG}/.MainActivity",
        "--ez", "navi_long_trip", "true" if long_trip else "false",
        "--ez", "navi_auto_plan", "true",
        "--ez", "navi_eco", "false",
        "--ez", "navi_restore_settings", "false",
        "--ez", "navi_avoid_ferries", "true" if avoid_ferries else "false",
        "--es", "navi_datex", datex,
        "--es", "navi_profile", "car",
        "--es", "navi_from_name", f_name.replace(" ", "_"),
        "--ed", "navi_from_lat", repr(f_lat),
        "--ed", "navi_from_lon", repr(f_lon),
        "--es", "navi_to_name", t_name.replace(" ", "_"),
        "--ed", "navi_to_lat", repr(t_lat),
        "--ed", "navi_to_lon", repr(t_lon),
    ]
    for i, (lat, lon, name) in enumerate(trip["vias"], start=1):
        args += [
            "--ed", f"navi_via{i}_lat", repr(lat),
            "--ed", f"navi_via{i}_lon", repr(lon),
            "--es", f"navi_via{i}_name", name.replace(" ", "_"),
        ]
    r = adb(*args)
    log(f"start: {(r.stdout or '').strip()} {(r.stderr or '').strip()}")


def logcat():
    return adb("logcat", "-d", "-v", "time", timeout=120).stdout or ""


def logcat_recent(n=200):
    return adb("logcat", "-d", "-v", "time", "-t", str(n), timeout=30).stdout or ""


def fold_name(s):
    return (
        (s or "")
        .casefold()
        .replace("å", "a")
        .replace("ä", "a")
        .replace("ö", "o")
        .replace("æ", "ae")
        .replace("ø", "o")
    )


def run_search(query):
    """Search through the app's real path (PlaceIndexStorage / searchPlaces)."""
    adb("logcat", "-c")
    adb(
        "shell",
        "am",
        "start",
        "-n",
        f"{PKG}/.MainActivity",
        "--es",
        "navi_search_q",
        query,
    )
    line = ""
    for _ in range(20):
        time.sleep(0.5)
        text = logcat()
        for ln in text.splitlines():
            if "NaviSearch" in ln and f"app_search q={query}" in ln:
                line = ln
        if line:
            break
    top = field(line, "top") if line else ""
    region = field(line, "region") if line else ""
    n = field(line, "n") if line else "0"
    return {
        "q": query,
        "n": int(n) if (n or "").isdigit() else 0,
        "top": top,
        "region": region,
        "line": line,
    }


def set_airplane(on):
    mode = "enable" if on else "disable"
    want = "1" if on else "0"
    adb("shell", "cmd", "connectivity", "airplane-mode", mode)
    adb("shell", "settings", "put", "global", "airplane_mode_on", want)
    adb(
        "shell",
        "am",
        "broadcast",
        "-a",
        "android.intent.action.AIRPLANE_MODE",
        "--ez",
        "state",
        "true" if on else "false",
    )
    for _ in range(20):
        time.sleep(0.25)
        got = (adb("shell", "settings", "get", "global", "airplane_mode_on").stdout or "").strip()
        if got == want:
            return
    log(f"airplane_mode_on stayed {got!r} want={want}")


def clear_forced_basemap():
    """Clear forced-online / forced-source and verify the hooks log they are off."""
    adb("logcat", "-c")
    adb(
        "shell",
        "am",
        "start",
        "-n",
        f"{PKG}/.MainActivity",
        "--ez",
        "navi_clear_basemap_test_hooks",
        "true",
        "--ez",
        "navi_force_online_basemap",
        "false",
    )
    line = ""
    for _ in range(40):
        time.sleep(0.25)
        text = adb("logcat", "-d", "-v", "time", "-s", "NaviMapHooks:I", timeout=30).stdout or ""
        for ln in text.splitlines():
            if "NaviMapHooks" in ln and "force_online=" in ln:
                line = ln
        if line:
            break
    online = (field(line, "force_online") or "").lower()
    source = field(line, "force_source") or ""
    if source.lower() == "null":
        source = ""
    cleared = (field(line, "cleared") or "").lower()
    ok = online in ("false", "0") and source in ("", "null") and cleared in ("true", "1")
    rec = {"ok": ok, "line": line, "force_online": online, "force_source": source}
    log(f"clear_forced_basemap ok={ok} line={line[-160:]}")
    return rec


def camera_to(lat, lon, zoom=12):
    camera_to_fu49(
        lat,
        lon,
        zoom,
        extra=["--ez", "navi_clear_basemap_test_hooks", "true"],
    )


FU44_POSITIONS = [
    ("elsa_overview", 64.88873, 19.51604),
    ("oslo", 59.91333, 10.73897),
    ("hamar", 60.79472, 11.06806),
    ("hallingdal_bromma", 60.50, 9.17),
    ("ostersund", 63.18, 14.64),
    ("ostlandet_varmland", 59.92, 12.29),
    ("hamburg", 53.55034, 9.99368),
]
FU44_ZOOMS = (3, 5, 7, 9, 11, 13, 15)
BLANK_BG = (248, 244, 240)


def _is_blank_bg(r, g, b, tol=6):
    br, bg, bb = BLANK_BG
    cream = abs(r - br) <= tol and abs(g - bg) <= tol and abs(b - bb) <= tol
    empty_gray = max(r, g, b) >= 215 and max(r, g, b) - min(r, g, b) <= 16
    cutoff = abs(r - 218) <= 8 and abs(g - 214) <= 8 and abs(b - 211) <= 8
    return cream or empty_gray or cutoff


def blank_share_png(path, tol=6):
    """Share of the map that is a solid blank background, in percent.

    A coarse grid so a straight half-screen cutoff counts and a drawn city
    (roads and water through the cream) does not.
    """
    try:
        from PIL import Image
    except ImportError:
        return None
    im = Image.open(path).convert("RGB")
    w, h = im.size
    if w <= 0 or h <= 0:
        return 100.0
    cols, rows = 8, 16
    top = max(1, int(h * 0.0375))
    bot = max(1, int(h * 0.02))
    y0, y1 = top, max(top + 1, h - bot)
    cw, ch = w // cols, (y1 - y0) // rows
    if cw <= 0 or ch <= 0:
        return 100.0
    blank_cells = 0
    for i in range(rows):
        for j in range(cols):
            crop = im.crop((j * cw, y0 + i * ch, j * cw + cw, y0 + i * ch + ch))
            px = list(crop.getdata())
            if not px:
                blank_cells += 1
                continue
            empty = sum(1 for r, g, b in px if _is_blank_bg(r, g, b, tol))
            if empty / len(px) >= 0.92:
                blank_cells += 1
    return 100.0 * blank_cells / (cols * rows)


def screencap(dest):
    dest = pathlib.Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    remote = "/data/local/tmp/navi_fu44.png"
    adb("shell", "screencap", "-p", remote)
    adb("pull", remote, str(dest), timeout=60)
    return dest


def pixel_nudge_deg(zoom, pixels=4):
    return pixels * 360.0 / (256.0 * (2.0 ** float(zoom)))


def run_fu44_map_matrix(out_dir, plan_elsa=True):
    """Idle then nudge screenshots at seven positions and seven zooms, net on/off."""
    out = pathlib.Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    rows = []
    before_poly = os.environ.get(
        "NAVI_FU44_BEFORE_POLY",
        "/mnt/2e9a1e9f-2097-408c-ab9a-a01b32f11d28/navi-gate/work-fu43/e_elsa_sjuvass.polyline.txt",
    )
    if pathlib.Path(before_poly).is_file():
        remote_poly = "/data/local/tmp/elsa_before.polyline.txt"
        adb("push", before_poly, remote_poly, timeout=60)
        clear_forced_basemap()
        adb(
            "shell",
            "am",
            "start",
            "-n",
            f"{PKG}/.MainActivity",
            "--ez",
            "navi_hide_chrome",
            "true",
            "--es",
            "navi_overlay_polyline_file",
            remote_poly,
        )
        time.sleep(2)
        camera_to(60.54, 9.14, 11)
        wait_tiles(16)
        screencap(out / "bromma_before.png")
        log("fu44-map bromma_before.png")
    if plan_elsa and os.environ.get("NAVI_FU44_SKIP_PLAN") != "1":
        log("fu44-map: plan Elsa for the route-overview camera")
        clear_previous_plan()
        adb("logcat", "-c")
        start_plan(TRIPS["elsa"], False, "none")
        status, end, wall, _peak = wait_plan(240)
        log(f"fu44-map elsa plan {status} after {wall:.0f}s: {end[-160:]}")
        camera_to(60.54, 9.14, 11)
        wait_tiles(16)
        screencap(out / "bromma_after.png")
        log("fu44-map bromma_after.png")
    only_net = os.environ.get("NAVI_FU44_NET", "").strip().lower()
    nets = ((False, "on"), (True, "off"))
    if only_net == "off":
        nets = ((True, "off"),)
    elif only_net == "on":
        nets = ((False, "on"),)
    for airplane, net in nets:
        set_airplane(airplane)
        time.sleep(2)
        try:
            for name, lat, lon in FU44_POSITIONS:
                for z in FU44_ZOOMS:
                    ensure_running()
                    clear_forced_basemap()
                    adb("logcat", "-c")
                    camera_to(lat, lon, z)
                    idle = wait_tiles(16)
                    idle_path = out / f"{name}_z{z}_{net}_idle.png"
                    screencap(idle_path)
                    idle_blank = blank_share_png(idle_path)
                    dlat = pixel_nudge_deg(z)
                    adb("logcat", "-c")
                    camera_to(lat + dlat, lon, z)
                    nudged = wait_tiles(16)
                    nudge_path = out / f"{name}_z{z}_{net}_nudge.png"
                    screencap(nudge_path)
                    nudge_blank = blank_share_png(nudge_path)
                    before = idle_blank if idle_blank is not None else idle.get("blank_pct")
                    after = nudge_blank if nudge_blank is not None else nudged.get("blank_pct")
                    pair_ok = (
                        before is not None
                        and after is not None
                        and abs(before - after) <= 1.0
                    )
                    rec = {
                        "position": name,
                        "lat": lat,
                        "lon": lon,
                        "zoom": z,
                        "network": net,
                        "idle_file": idle_path.name,
                        "nudge_file": nudge_path.name,
                        "blank_idle": None if before is None else round(before, 2),
                        "blank_nudge": None if after is None else round(after, 2),
                        "pair_ok": pair_ok,
                        "log_idle": idle.get("blank_pct"),
                        "log_nudge": nudged.get("blank_pct"),
                    }
                    rows.append(rec)
                    log(
                        f"fu44-map {name} z{z} net={net} idle={rec['blank_idle']} "
                        f"nudge={rec['blank_nudge']} pair_ok={pair_ok}"
                    )
        finally:
            if airplane:
                set_airplane(False)
                time.sleep(1)
    (out / "blank-share.json").write_text(json.dumps(rows, indent=2) + "\n")
    return rows


def parse_camera_line(line):
    try:
        lat = float(field(line, "lat") or "nan")
        lon = float(field(line, "lon") or "nan")
        zoom = float(field(line, "zoom") or "nan")
    except ValueError:
        return None
    if any(math.isnan(v) for v in (lat, lon, zoom)):
        return None
    return lat, lon, zoom


def camera_close(got, want_lat, want_lon, want_zoom):
    lat, lon, zoom = got
    lat_tol = max(0.03, pixel_nudge_deg(want_zoom) * 12)
    lon_tol = lat_tol / max(0.2, math.cos(math.radians(want_lat)))
    return (
        abs(lat - want_lat) <= lat_tol
        and abs(lon - want_lon) <= lon_tol
        and abs(zoom - want_zoom) <= 0.4
    )


def wait_camera(lat, lon, zoom, seconds=16):
    last = ""
    got = None
    for _ in range(int(seconds * 2)):
        time.sleep(0.5)
        text = logcat()
        for ln in text.splitlines():
            if "NaviMapCamera" not in ln or "lat=" not in ln:
                continue
            parsed = parse_camera_line(ln)
            if parsed is None:
                continue
            last = ln
            got = parsed
            if camera_close(parsed, lat, lon, zoom):
                return {"ok": True, "line": ln, "lat": parsed[0], "lon": parsed[1], "zoom": parsed[2]}
    return {
        "ok": False,
        "line": last,
        "lat": None if got is None else got[0],
        "lon": None if got is None else got[1],
        "zoom": None if got is None else got[2],
    }


def parse_feature_grid(text):
    cells = {}
    for ln in text.splitlines():
        if "NaviMapGrid" not in ln or "roads=" not in ln:
            continue
        try:
            col = int(field(ln, "c") or "-1")
            row = int(field(ln, "r") or "-1")
            roads = int(field(ln, "roads") or "0")
            water = int(field(ln, "water") or "0")
            labels = int(field(ln, "labels") or "0")
            blank = int(field(ln, "blank") or "0")
        except ValueError:
            continue
        if col < 0 or row < 0:
            continue
        cells[f"{col},{row}"] = {
            "roads": roads,
            "water": water,
            "labels": labels,
            "blank": blank,
        }
    return cells


def grid_agree(a, b):
    """Legacy exact-count compare. Follow-up 47 uses [grid_presence_agree]."""
    if not a or not b:
        return False
    if set(a) != set(b):
        return False
    for key in a:
        if (
            a[key]["roads"] != b[key]["roads"]
            or a[key]["water"] != b[key]["water"]
            or a[key]["labels"] != b[key]["labels"]
        ):
            return False
    return True


def _present(cell, kind):
    return int(cell.get(kind) or 0) > 0


def grid_presence_agree(a, b):
    """Per-cell presence of roads/water/labels; whole-view totals within 10%."""
    mismatches = []
    if not a or not b:
        return False, ["empty-grid"], {}
    keys = set(a) | set(b)
    totals = {"roads": [0, 0], "water": [0, 0], "labels": [0, 0]}
    for key in sorted(keys):
        ca = a.get(key) or {"roads": 0, "water": 0, "labels": 0}
        cb = b.get(key) or {"roads": 0, "water": 0, "labels": 0}
        for kind in ("roads", "water", "labels"):
            totals[kind][0] += int(ca.get(kind) or 0)
            totals[kind][1] += int(cb.get(kind) or 0)
            if _present(ca, kind) != _present(cb, kind):
                mismatches.append(f"{key}:{kind}")
    tot_ok = True
    for kind, (ia, ib) in totals.items():
        hi = max(ia, ib)
        lo = min(ia, ib)
        if hi > 0 and lo < 0.9 * hi:
            tot_ok = False
            mismatches.append(f"total:{kind}:{ia}/{ib}")
    return (not mismatches) and tot_ok, mismatches, totals


def parse_archive_features(text):
    """Layers present in mounted archive tiles at the camera (NaviMapArchive)."""
    roads = water = labels = False
    files = []
    for ln in text.splitlines():
        if "NaviMapArchive" not in ln or "layers=" not in ln:
            continue
        files.append(ln)
        if (field(ln, "roads") or "0") not in ("0", ""):
            roads = True
        if (field(ln, "water") or "0") not in ("0", ""):
            water = True
        if (field(ln, "labels") or "0") not in ("0", ""):
            labels = True
    return {"roads": roads, "water": water, "labels": labels, "lines": files}


def shoot_verified(dest, lat, lon, zoom, seconds=18):
    """Move camera, require a matching NaviMapCamera log, then screenshot."""
    last_cam = {"ok": False}
    last_tiles = {}
    last_grid = {}
    last_archive = {}
    for attempt in range(3):
        adb("logcat", "-c")
        camera_to(lat, lon, zoom)
        last_cam = wait_camera(lat, lon, zoom, seconds)
        last_tiles = wait_tiles(seconds)
        cat = logcat()
        last_grid = parse_feature_grid(cat)
        last_archive = parse_archive_features(cat)
        if last_cam.get("ok"):
            screencap(dest)
            last_tiles["camera_ok"] = True
            last_tiles["grid"] = last_grid
            last_tiles["archive"] = last_archive
            last_tiles["attempts"] = attempt + 1
            return last_tiles
        log(
            f"discard wrong camera {pathlib.Path(dest).name} "
            f"want={lat:.5f},{lon:.5f},z{zoom} got={last_cam}"
        )
    screencap(dest)
    last_tiles["camera_ok"] = False
    last_tiles["grid"] = last_grid
    last_tiles["archive"] = last_archive
    last_tiles["attempts"] = 3
    last_tiles["ok"] = False
    return last_tiles


def run_fu46_map_matrix(out_dir, extra_shots=True):
    """Idle then nudge on one APK; camera verified from the app log."""
    out = pathlib.Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    rows = []
    only_net = os.environ.get("NAVI_FU46_NET", os.environ.get("NAVI_FU44_NET", "")).strip().lower()
    nets = ((False, "on"), (True, "off"))
    if only_net == "off":
        nets = ((True, "off"),)
    elif only_net == "on":
        nets = ((False, "on"),)
    for airplane, net in nets:
        set_airplane(airplane)
        time.sleep(2)
        try:
            for name, lat, lon in FU44_POSITIONS:
                for z in FU44_ZOOMS:
                    ensure_running()
                    clear_forced_basemap()
                    idle_path = out / f"{name}_z{z}_{net}_idle.png"
                    idle = shoot_verified(idle_path, lat, lon, z)
                    dlat = pixel_nudge_deg(z)
                    nudge_path = out / f"{name}_z{z}_{net}_nudge.png"
                    nudged = shoot_verified(nudge_path, lat + dlat, lon, z)
                    idle_blank = blank_share_png(idle_path)
                    nudge_blank = blank_share_png(nudge_path)
                    agree = grid_agree(idle.get("grid") or {}, nudged.get("grid") or {})
                    rec = {
                        "position": name,
                        "lat": lat,
                        "lon": lon,
                        "zoom": z,
                        "network": net,
                        "idle_file": idle_path.name,
                        "nudge_file": nudge_path.name,
                        "blank_idle": None if idle_blank is None else round(idle_blank, 2),
                        "blank_nudge": None if nudge_blank is None else round(nudge_blank, 2),
                        "camera_idle_ok": bool(idle.get("camera_ok")),
                        "camera_nudge_ok": bool(nudged.get("camera_ok")),
                        "grid_agree": agree,
                        "idle_grid": idle.get("grid") or {},
                        "nudge_grid": nudged.get("grid") or {},
                    }
                    rows.append(rec)
                    log(
                        f"fu46-map {name} z{z} net={net} cam={rec['camera_idle_ok']}/{rec['camera_nudge_ok']} "
                        f"grid_agree={agree} blank={rec['blank_idle']}/{rec['blank_nudge']}"
                    )
        finally:
            if airplane:
                set_airplane(False)
                time.sleep(1)
    extras = []
    if extra_shots:
        extras.extend(run_fu46_extra_shots(out))
    (out / "blank-share.json").write_text(json.dumps({"rows": rows, "extras": extras}, indent=2) + "\n")
    return rows, extras


def run_fu46_extra_shots(out):
    extras = []
    clear_forced_basemap()
    hamar = next(p for p in FU44_POSITIONS if p[0] == "hamar")
    _, lat, lon = hamar
    prev = None
    for z in FU44_ZOOMS:
        path = out / f"hamar_zoom_step_z{z}.png"
        rec = shoot_verified(path, lat, lon, z)
        extras.append(
            {
                "kind": "zoom_step",
                "file": path.name,
                "zoom": z,
                "camera_ok": rec.get("camera_ok"),
                "blank": blank_share_png(path),
                "grid": rec.get("grid") or {},
                "blank_frame": (blank_share_png(path) or 0) >= 80,
            }
        )
    border = next(p for p in FU44_POSITIONS if p[0] == "ostlandet_varmland")
    _, blat, blon = border
    for i, dlon in enumerate((-0.35, -0.15, 0.0, 0.15, 0.35)):
        path = out / f"border_pan_z11_{i}.png"
        rec = shoot_verified(path, blat, blon + dlon, 11)
        extras.append(
            {
                "kind": "border_pan",
                "file": path.name,
                "lon": blon + dlon,
                "camera_ok": rec.get("camera_ok"),
                "blank": blank_share_png(path),
                "grid": rec.get("grid") or {},
            }
        )
    if os.environ.get("NAVI_FU46_SKIP_PLAN") != "1":
        clear_previous_plan()
        adb("logcat", "-c")
        start_plan(TRIPS["elsa"], False, "none")
        status, end, wall, _peak = wait_plan(240)
        log(f"fu46-map elsa plan {status} after {wall:.0f}s: {end[-160:]}")
        path = out / "elsa_after_plan.png"
        rec = shoot_verified(path, 64.88873, 19.51604, 5)
        extras.append(
            {
                "kind": "elsa_after_plan",
                "file": path.name,
                "plan_status": status,
                "camera_ok": rec.get("camera_ok"),
                "blank": blank_share_png(path),
            }
        )
    set_airplane(False)
    camera_to(59.91333, 10.73897, 11)
    wait_camera(59.91333, 10.73897, 11, 12)
    wait_tiles(8)
    adb("shell", "settings", "put", "system", "accelerometer_rotation", "0")
    adb("shell", "settings", "put", "system", "user_rotation", "1")
    time.sleep(3)
    rotate_path = out / "oslo_z11_rotated.png"
    screencap(rotate_path)
    extras.append({"kind": "rotate", "file": rotate_path.name, "blank": blank_share_png(rotate_path)})
    adb("shell", "settings", "put", "system", "user_rotation", "0")
    time.sleep(2)
    return extras


def run_fu46_gate_map_check():
    """Network off: Oslo, Hamar, Hallingdal, border at z7/11/14."""
    points = [
        ("oslo", 59.91333, 10.73897),
        ("hamar", 60.79472, 11.06806),
        ("hallingdal_bromma", 60.50, 9.17),
        ("ostlandet_varmland", 59.92, 12.29),
    ]
    set_airplane(True)
    time.sleep(2)
    out = []
    try:
        for name, lat, lon in points:
            for z in (7, 11, 14):
                clear_forced_basemap()
                rec = shoot_verified(f"/tmp/navi_fu46_gate_{name}_z{z}.png", lat, lon, z, seconds=14)
                rec.update({"position": name, "zoom": z, "network": "off"})
                rec["camera_ok"] = bool(rec.get("camera_ok"))
                rec["ok"] = bool(rec.get("camera_ok") and rec.get("ok"))
                out.append(rec)
                log(
                    f"fu46-gate {name} z{z} cam={rec.get('camera_ok')} "
                    f"blank={rec.get('blank_pct')} ok={rec.get('ok')}"
                )
    finally:
        set_airplane(False)
        time.sleep(1)
    return out


FU47_ZOOMS = (5, 11, 15)
FU47_OUTSIDE = (
    ("paris", 48.8566, 2.3522),
    ("stockholm", 59.3293, 18.0686),
)
# Recorded in the gate until follow-up 48 fixes selection and coarse fill.
KNOWN_MAP_FAILURES = (
    {
        "id": "elsa_overview_z15",
        "reason": (
            "Elsa overview z15 is blank because a neighbouring archive is ranked "
            "first by bounding-box overlap (Finland over Vasterbotten at 64.889, 19.516)."
        ),
    },
    {
        "id": "low_zoom_mint_fill",
        "reason": (
            "At low zoom a regional archive paints flat land fill outside its real "
            "coverage (Sweden from Oslo/Hamar z5, Poland from Hamburg z5) because "
            "header bounds are a rectangle and coarse world tiles fill the rest."
        ),
    },
)


def _fu47_row(name, lat, lon, z, net, idle, nudged, idle_path, nudge_path):
    idle_grid = idle.get("grid") or {}
    nudge_grid = nudged.get("grid") or {}
    agree, mismatches, totals = grid_presence_agree(idle_grid, nudge_grid)
    idle_arch = idle.get("archive") or {}
    nudge_arch = nudged.get("archive") or {}
    archive = {
        "roads": bool(idle_arch.get("roads") or nudge_arch.get("roads")),
        "water": bool(idle_arch.get("water") or nudge_arch.get("water")),
        "labels": bool(idle_arch.get("labels") or nudge_arch.get("labels")),
        "lines": (idle_arch.get("lines") or []) + (nudge_arch.get("lines") or []),
    }
    idle_tot = {k: sum(int((c or {}).get(k) or 0) for c in idle_grid.values()) for k in ("roads", "water", "labels")}
    return {
        "position": name,
        "lat": lat,
        "lon": lon,
        "zoom": z,
        "network": net,
        "idle_file": idle_path.name,
        "nudge_file": nudge_path.name,
        "camera_idle_ok": bool(idle.get("camera_ok")),
        "camera_nudge_ok": bool(nudged.get("camera_ok")),
        "grid_presence_agree": agree,
        "presence_mismatch": mismatches,
        "totals": totals,
        "idle_totals": idle_tot,
        "archive": {k: archive[k] for k in ("roads", "water", "labels")},
        "blank_idle": None if blank_share_png(idle_path) is None else round(blank_share_png(idle_path), 2),
        "blank_nudge": None if blank_share_png(nudge_path) is None else round(blank_share_png(nudge_path), 2),
        "ok": bool(idle.get("camera_ok"))
        and bool(nudged.get("camera_ok"))
        and not any(m.startswith("total:") for m in mismatches),
    }


def run_fu47_map_matrix(out_dir, extra_shots=True):
    """Corrected idle/nudge matrix: z5/11/15 plus named extras."""
    out = pathlib.Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    rows = []
    only_net = os.environ.get("NAVI_FU47_NET", os.environ.get("NAVI_FU46_NET", "")).strip().lower()
    nets = ((False, "on"), (True, "off"))
    if only_net == "off":
        nets = ((True, "off"),)
    elif only_net == "on":
        nets = ((False, "on"),)
    for airplane, net in nets:
        set_airplane(airplane)
        time.sleep(2)
        try:
            for name, lat, lon in FU44_POSITIONS:
                for z in FU47_ZOOMS:
                    ensure_running()
                    clear_forced_basemap()
                    idle_path = out / f"{name}_z{z}_{net}_idle.png"
                    idle = shoot_verified(idle_path, lat, lon, z)
                    dlat = pixel_nudge_deg(z)
                    nudge_path = out / f"{name}_z{z}_{net}_nudge.png"
                    nudged = shoot_verified(nudge_path, lat + dlat, lon, z)
                    rec = _fu47_row(name, lat, lon, z, net, idle, nudged, idle_path, nudge_path)
                    rows.append(rec)
                    log(
                        f"fu47-map {name} z{z} net={net} cam={rec['camera_idle_ok']}/{rec['camera_nudge_ok']} "
                        f"presence={rec['grid_presence_agree']} mismatch={rec['presence_mismatch'][:6]}"
                    )
        finally:
            if airplane:
                set_airplane(False)
                time.sleep(1)
    extras = []
    if extra_shots:
        extras.extend(run_fu47_extra_shots(out))
    (out / "blank-share.json").write_text(json.dumps({"rows": rows, "extras": extras}, indent=2) + "\n")
    return rows, extras


def run_fu47_extra_shots(out):
    extras = []
    extras.extend(run_fu46_extra_shots(out))
    border = next(p for p in FU44_POSITIONS if p[0] == "ostlandet_varmland")
    _, blat, blon = border
    for airplane, net in ((False, "on"), (True, "off")):
        set_airplane(airplane)
        time.sleep(2)
        try:
            for z in (13, 15):
                path = out / f"border_overlap_z{z}_{net}_idle.png"
                rec = shoot_verified(path, blat, blon, z)
                extras.append(
                    {
                        "kind": "border_overlap",
                        "file": path.name,
                        "zoom": z,
                        "network": net,
                        "camera_ok": rec.get("camera_ok"),
                        "blank": blank_share_png(path),
                        "grid": rec.get("grid") or {},
                        "archive": rec.get("archive") or {},
                    }
                )
                log(f"fu47-border-overlap z{z} net={net} cam={rec.get('camera_ok')}")
        finally:
            if airplane:
                set_airplane(False)
                time.sleep(1)
    for airplane, net in ((False, "on"), (True, "off")):
        set_airplane(airplane)
        time.sleep(2)
        try:
            for name, lat, lon in FU47_OUTSIDE:
                path = out / f"{name}_z11_{net}_idle.png"
                rec = shoot_verified(path, lat, lon, 11)
                extras.append(
                    {
                        "kind": "outside_region",
                        "file": path.name,
                        "position": name,
                        "zoom": 11,
                        "network": net,
                        "camera_ok": rec.get("camera_ok"),
                        "blank": blank_share_png(path),
                        "grid": rec.get("grid") or {},
                        "archive": rec.get("archive") or {},
                    }
                )
                log(f"fu47-outside {name} z11 net={net} cam={rec.get('camera_ok')}")
        finally:
            if airplane:
                set_airplane(False)
                time.sleep(1)
    return extras


def run_fu47_gate_map_check():
    """Network off: Oslo, Hamar, Hallingdal, border at z7/11/14 with presence rules."""
    points = [
        ("oslo", 59.91333, 10.73897),
        ("hamar", 60.79472, 11.06806),
        ("hallingdal_bromma", 60.50, 9.17),
        ("ostlandet_varmland", 59.92, 12.29),
    ]
    set_airplane(True)
    time.sleep(2)
    out = []
    try:
        for name, lat, lon in points:
            for z in (7, 11, 14):
                clear_forced_basemap()
                rec = shoot_verified(f"/tmp/navi_fu47_gate_{name}_z{z}.png", lat, lon, z, seconds=14)
                grid = rec.get("grid") or {}
                roads = sum(int((c or {}).get("roads") or 0) for c in grid.values())
                water = sum(int((c or {}).get("water") or 0) for c in grid.values())
                rec.update({"position": name, "zoom": z, "network": "off", "roads": roads, "water": water})
                rec["camera_ok"] = bool(rec.get("camera_ok"))
                rec["ok"] = bool(rec.get("camera_ok") and (roads > 0 or water > 0))
                out.append(rec)
                log(
                    f"fu47-gate {name} z{z} cam={rec.get('camera_ok')} "
                    f"roads={roads} water={water} ok={rec.get('ok')}"
                )
    finally:
        set_airplane(False)
        time.sleep(1)
    return out


def wait_tiles(seconds=12):
    """Wait until the map logs a blank-area share. Feature count is not used."""
    visible = None
    blank = None
    kind = ""
    line = ""
    min_visible = None
    min_blank = None
    for _ in range(int(seconds * 2)):
        time.sleep(0.5)
        text = logcat()
        for ln in text.splitlines():
            if "NaviMapTiles" in ln and "blank_pct=" in ln:
                line = ln
                raw = field(ln, "visible") or "0"
                try:
                    visible = int(raw)
                except ValueError:
                    visible = 0
                braw = field(ln, "blank_pct") or ""
                try:
                    blank = float(braw)
                except ValueError:
                    blank = None
                kind = field(ln, "kind") or ""
                if min_visible is None or visible < min_visible:
                    min_visible = visible
                if blank is not None and (min_blank is None or blank < min_blank):
                    min_blank = blank
        if blank is not None:
            break
    ok = blank is not None and blank <= 25.0
    return {
        "visible": visible or 0,
        "min_visible": min_visible if min_visible is not None else 0,
        "blank_pct": blank,
        "min_blank_pct": min_blank,
        "kind": kind,
        "line": line,
        "ok": ok,
    }


def run_offline_map_check():
    """Network off, then on: tiles at each gate-route start, via and destination."""
    out = []
    for offline, net in ((True, "off"), (False, "on")):
        hooks = clear_forced_basemap()
        if not hooks.get("ok"):
            out.append(
                {
                    "trip": "hooks",
                    "role": "clear",
                    "network": net,
                    "visible": 0,
                    "ok": False,
                    "kind": "hooks",
                    "line": hooks.get("line") or "NaviMapHooks missing",
                }
            )
        extra = ["--ez", "navi_fu49_force_offline", "true" if offline else "false"]
        set_true_offline(offline)
        time.sleep(2)
        try:
            for name, spec in TRIPS.items():
                points = [("from", spec["from"])]
                points += [("via", v) for v in spec["vias"]]
                points.append(("to", spec["to"]))
                for role, (lat, lon, label) in points:
                    hooks = clear_forced_basemap()
                    adb("logcat", "-c")
                    camera_to_fu49(lat, lon, 12, extra)
                    settled = wait_fu49_settle(lat, lon, 12, extra, 20)
                    tot = settled.get("totals") or {}
                    visible = (
                        int(tot.get("roads") or 0)
                        + int(tot.get("water") or 0)
                        + int(tot.get("labels") or 0)
                    )
                    rec = {
                        "visible": visible,
                        "min_visible": visible,
                        "blank_pct": tot.get("blank_cells"),
                        "min_blank_pct": tot.get("blank_cells"),
                        "kind": "settled",
                        "line": tot.get("line") or "",
                        "ok": visible > 0,
                        "totals": tot,
                        "trip": name,
                        "role": role,
                        "label": label,
                        "lat": lat,
                        "lon": lon,
                        "network": net,
                        "hooks_cleared": hooks.get("ok", False),
                    }
                    if not hooks.get("ok"):
                        rec["ok"] = False
                    out.append(rec)
                    log(
                        f"map_{net} {name} {role} {label} visible={rec['visible']} "
                        f"kind={rec['kind']} ok={rec['ok']}"
                    )
        finally:
            if offline:
                set_true_offline(False)
                time.sleep(1)
    return out


def run_display_checks():
    """Hop, border pan, and plan-must-not-blank. Hooks cleared before each."""
    hops = [
        (53.551, 9.993, "hamburg"),
        (57.708, 11.974, "vastra_gotaland"),
        (60.674, 17.141, "gavleborg"),
    ]
    hooks_ok = True
    hooks = clear_forced_basemap()
    hooks_ok = hooks_ok and hooks.get("ok", False)
    adb("logcat", "-c")
    t0 = time.time()
    for lat, lon, _name in hops:
        camera_to(lat, lon, 10)
    hop_s = time.time() - t0
    last = wait_tiles(8)
    hop = {
        "ok": hop_s < 1.0 and last["ok"],
        "elapsed_s": round(hop_s, 3),
        "last_kind": last["kind"],
        "last_visible": last["visible"],
        "blank_pct": last.get("blank_pct"),
        "line": last["line"],
    }
    log(
        f"hop_three_regions elapsed={hop['elapsed_s']}s "
        f"blank_pct={last.get('blank_pct')} ok={hop['ok']}"
    )

    clear_forced_basemap()
    adb("logcat", "-c")
    # Halland / Västra Götaland border (FU40 blank-map site).
    empties = 0
    frames = []
    for lon in (12.2, 12.35, 12.5, 12.65):
        adb("logcat", "-c")
        camera_to(57.5, lon, 9)
        rec = wait_tiles(6)
        frames.append(rec)
        if not rec["ok"]:
            empties += 1
    pan = {
        "ok": empties == 0,
        "empty_frames": empties,
        "frames": frames,
    }
    log(f"border_pan empty_frames={empties} ok={pan['ok']}")

    clear_forced_basemap()
    adb("logcat", "-c")
    camera_to(53.551, 9.993, 11)
    before = wait_tiles(12)
    if not before["ok"]:
        adb("logcat", "-c")
        camera_to(53.551, 9.993, 11)
        before = wait_tiles(12)
    start_plan(TRIPS["bevensen"], False, "none")
    time.sleep(3)
    during = wait_tiles(8)
    plan = {
        "ok": before["ok"] and during["ok"],
        "before": before.get("blank_pct"),
        "during": during.get("blank_pct"),
        "before_visible": before["visible"],
        "during_visible": during["visible"],
    }
    log(
        f"plan_no_blank before_blank={plan['before']} "
        f"during_blank={plan['during']} ok={plan['ok']}"
    )
    # This check starts a real plan; wait it out so the gate trip is not busy.
    status, end, wall, _peak = wait_plan(180)
    log(f"plan_no_blank drain {status} after {wall:.0f}s: {end[-160:]}")
    return {
        "hooks_cleared": hooks_ok,
        "hop": hop,
        "border_pan": pan,
        "plan_no_blank": plan,
    }


def run_search_check():
    out = []
    for q, want in SEARCH_EXPECT:
        rec = run_search(q)
        rec["want"] = want
        rec["ok"] = rec["n"] > 0 and fold_name(want) in fold_name(rec["top"])
        out.append(rec)
        log(f"search q={q} n={rec['n']} top={rec['top']} region={rec['region']} ok={rec['ok']}")
    return out


def clear_place_index_region(region_id):
    adb("logcat", "-c")
    adb(
        "shell",
        "am",
        "start",
        "-n",
        f"{PKG}/.MainActivity",
        "--es",
        "navi_place_index_clear_region",
        region_id,
    )
    line = ""
    for _ in range(180):
        time.sleep(1)
        text = logcat()
        for ln in text.splitlines():
            if "harness_clear region=" in ln or "refuse clear region=" in ln:
                line = ln
        if line:
            break
    log(f"clear {region_id}: {line[-200:]}")
    return line


# Tiny fixture + scratch DB for the emulator pause test. Never the product index.
TINY_PBF = pathlib.Path(__file__).resolve().parents[1] / "core/tests/fixtures/place-source-tiny.osm.pbf"
PAUSE_SCRATCH = "/storage/0000-0000/Android/data/no.navi.app/files/fu39-pause-scratch"
INDEX_PHASES = ("Migrate", "ReadSource", "SortPrepare", "Insert", "Fts", "Commit")


def write_v4_scratch_db(host_path):
    """Legacy v4 rows so open() runs migrate on the scratch file only."""
    import sqlite3

    host_path.parent.mkdir(parents=True, exist_ok=True)
    if host_path.exists():
        host_path.unlink()
    conn = sqlite3.connect(host_path)
    conn.execute(
        """
        CREATE TABLE name_entries (
            osm_id INTEGER PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            lat REAL NOT NULL,
            lon REAL NOT NULL,
            sub_area TEXT NOT NULL DEFAULT '',
            municipality TEXT NOT NULL DEFAULT '',
            region_id TEXT NOT NULL DEFAULT '',
            search_doc TEXT NOT NULL DEFAULT ''
        )
        """
    )
    conn.execute("PRAGMA user_version = 4")
    conn.executemany(
        "INSERT INTO name_entries(osm_id, name, kind, lat, lon, region_id, search_doc) "
        "VALUES (?,?,?,?,?,?,?)",
        [
            (i, f"n{i}", "place:hamlet", 60.0, 10.0, "europe/norway/ostlandet", f"n{i}")
            for i in range(40_000)
        ],
    )
    conn.commit()
    conn.close()


def logcat_pause_tags(n=120):
    return (
        adb(
            "logcat",
            "-d",
            "-v",
            "time",
            "-t",
            str(n),
            "NaviRouting:I",
            "IdlePackJobs:I",
            "NaviNative:I",
            "PlaceIndexBg:I",
            "*:S",
            timeout=20,
        ).stdout
        or ""
    )


def wait_planning_start(deadline_s):
    """Wait until planning_start (planner acquired), not until the route finishes."""
    t0 = time.time()
    while time.time() - t0 < deadline_s:
        text = logcat_pause_tags(150)
        st = last_line(text, "planning_start ")
        pause = last_line(text, "idle_job_pause ") or last_line(text, "plan_idle_pause ")
        already = last_line(text, "plan already running")
        if st:
            return {
                "ok": True,
                "wait_s": time.time() - t0,
                "planning_start": st[-200:],
                "idle_job_pause": pause[-200:] if pause else "",
                "already": already[-200:] if already else "",
            }
        if already:
            return {
                "ok": False,
                "wait_s": time.time() - t0,
                "error": "already_planning",
                "already": already[-200:],
                "idle_job_pause": pause[-200:] if pause else "",
            }
        time.sleep(0.1)
    return {"ok": False, "wait_s": time.time() - t0, "error": "timeout_planning_start"}


def run_pause_plan_phases(remote_pbf, remote_db):
    """Start Oslo→Lillestrøm in each index-build phase; planner must start in 2 s."""
    trip = TRIPS["oslo_lillestrom"]
    results = []
    seen = set()
    deadline = time.time() + 240
    idle_after_built = 0
    while len(seen) < len(INDEX_PHASES) and time.time() < deadline:
        text = logcat_pause_tags(200)
        hit = None
        for phase in INDEX_PHASES:
            if phase in seen:
                continue
            if f"place_index_build phase={phase}" in text:
                hit = phase
                break
        if hit is None:
            if "action=built" in text and "place_index_build" in text:
                idle_after_built += 1
                if idle_after_built >= 8:
                    break
            time.sleep(0.1)
            continue
        idle_after_built = 0
        clear_previous_plan()
        t0 = time.time()
        start_plan(trip, False, "none", long_trip=False)
        rec = wait_planning_start(5)
        rec["phase"] = hit
        rec["to_planner_s"] = rec.get("wait_s", time.time() - t0)
        pause_line = rec.get("idle_job_pause") or ""
        if not pause_line:
            pause_line = last_line(logcat_pause_tags(200), "idle_job_pause ") or last_line(
                logcat_pause_tags(200), "plan_idle_pause "
            )
            rec["idle_job_pause"] = pause_line[-200:] if pause_line else ""
        dm = re.search(r"duration_ms=(\d+)", rec.get("idle_job_pause") or "")
        rec["pause_duration_ms"] = int(dm.group(1)) if dm else None
        pause_ok = rec["pause_duration_ms"] is not None and rec["pause_duration_ms"] <= 2000
        rec["ok"] = bool(rec.get("ok")) and (pause_ok or rec["to_planner_s"] <= 2.0)
        status, end, wall, _peak = wait_plan(90)
        rec["plan_status"] = status
        rec["plan_wall_s"] = wall
        rec["plan_end"] = (end or "")[-160:]
        seen.add(hit)
        results.append(rec)
        log(
            f"pause-phase {hit} planner={rec['to_planner_s']:.2f}s "
            f"duration_ms={rec['pause_duration_ms']} ok={rec['ok']} plan={status}"
        )
    missing = [p for p in INDEX_PHASES if p not in seen]
    ok = not missing and all(r.get("ok") for r in results)
    return {"ok": ok, "phases": results, "missing": missing}


def run_pause_test():
    """Index a small fixture into a scratch database, then remove both.

    The product place index is not opened for write. While the scratch index
    runs, a short plan is started in each build phase.
    """
    if not TINY_PBF.is_file():
        return {"ok": False, "error": f"missing {TINY_PBF}"}
    remote_pbf = f"{PAUSE_SCRATCH}/tiny.navi-place-source.osm.pbf"
    remote_db = f"{PAUSE_SCRATCH}/place_index.db"
    adb("shell", f"rm -rf {PAUSE_SCRATCH} && mkdir -p {PAUSE_SCRATCH}")
    pushed = adb("push", str(TINY_PBF), remote_pbf)
    if pushed.returncode != 0:
        return {"ok": False, "error": (pushed.stderr or pushed.stdout or "push failed").strip()}
    host_v4 = pathlib.Path("/tmp/navi-fu56-v4-scratch.db")
    write_v4_scratch_db(host_v4)
    pushed_db = adb("push", str(host_v4), remote_db)
    if pushed_db.returncode != 0:
        return {"ok": False, "error": (pushed_db.stderr or pushed_db.stdout or "push db failed").strip()}
    before = {"rows": {}}
    adb("logcat", "-c")
    adb(
        "shell",
        "am",
        "start",
        "-n",
        f"{PKG}/.MainActivity",
        "--ei",
        "navi_index_phase_sleep_ms",
        "1200",
        "--es",
        "navi_place_index_pbf",
        remote_pbf,
        "--es",
        "navi_place_index_region",
        "test/fu38-pause",
        "--es",
        "navi_place_index_db",
        remote_db,
    )
    line = ""
    for _ in range(40):
        time.sleep(0.15)
        text = logcat_pause_tags(80)
        for ln in text.splitlines():
            if (
                "debug place-index queued" in ln
                or "refusing PLACE_INDEX" in ln
                or "place_index_build" in ln
                or "idle_job_pause" in ln
                or "index_phase_sleep_ms=" in ln
            ):
                line = ln
        if line and (
            "queued" in line
            or "action=built" in line
            or "refusing" in line
            or "PAUSED" in line
            or "idle_job_pause" in line
            or "phase=" in line
            or "index_phase_sleep_ms=" in line
        ):
            break
    phases = run_pause_plan_phases(remote_pbf, remote_db)
    after = {"rows": {}}
    product_rows_before = (before.get("rows") or {}).get("test/fu38-pause", 0)
    product_rows_after = (after.get("rows") or {}).get("test/fu38-pause", 0)
    scratch_ls = (adb("shell", f"ls -l {remote_db} {remote_db}-wal 2>/dev/null").stdout or "").strip()
    finished = False
    for _ in range(90):
        text = logcat_recent(80)
        if any(
            s in text
            for s in (
                "PlaceIndexBg: finished",
                "action=built",
            )
        ):
            finished = True
            break
        time.sleep(1)
    if not finished:
        log("pause-test: index job did not finish in 90s; removing scratch anyway")
    adb(
        "shell",
        "am",
        "start",
        "-n",
        f"{PKG}/.MainActivity",
        "--ei",
        "navi_index_phase_sleep_ms",
        "0",
    )
    adb("shell", f"rm -rf {PAUSE_SCRATCH}")
    gone = (adb("shell", f"ls {PAUSE_SCRATCH} 2>/dev/null").stdout or "").strip() == ""
    queued = "queued" in line or "action=built" in line or "phase=" in line
    refused_product = "product=true" in line
    ok = (
        queued
        and not refused_product
        and product_rows_after == 0
        and product_rows_before == 0
        and gone
        and phases.get("ok")
    )
    rec = {
        "ok": ok,
        "line": line,
        "scratch_db": remote_db,
        "scratch_listing": scratch_ls,
        "scratch_removed": gone,
        "product_test_fu38_pause_before": product_rows_before,
        "product_test_fu38_pause_after": product_rows_after,
        "phases": phases,
    }
    log(f"pause-test ok={ok} product_rows={product_rows_after} scratch_removed={gone} phases={phases.get('ok')} {line[-160:]}")
    return rec


def plan_stage_peaks(text):
    """Per-stage RSS from plan_mem / hop_mem / stage_b lines (not process HWM)."""
    stages = {}
    plan_peak = 0
    for ln in text.splitlines():
        if "plan_mem " in ln or "hop_mem " in ln or "stage_b_timing " in ln:
            stage = field(ln, "stage") or field(ln, "hop") or "stage"
            rss = field(ln, "peak_rss_mb") or field(ln, "rss_mb") or field(ln, "corridor_peak_rss_mb")
            if rss and rss.replace(".", "", 1).isdigit():
                mb = float(rss)
                stages[stage] = mb
                plan_peak = max(plan_peak, mb)
    return stages, plan_peak


def last_line(text, needle):
    hits = [ln for ln in text.splitlines() if needle in ln]
    return hits[-1] if hits else ""


def field(line, key):
    m = re.search(rf"(?:^|[ ;]){re.escape(key)}=(\S*)", line)
    return m.group(1).rstrip(";") if m else None


def wait_plan(deadline_s):
    """Wait for planning_done / planning_failed of a plan that is not preparing."""
    t0 = time.time()
    peak = 0
    while time.time() - t0 < deadline_s:
        rss = proc_kb("VmRSS") or 0
        peak = max(peak, rss)
        text = logcat()
        done = last_line(text, "planning_done")
        failed = last_line(text, "planning_failed")
        end = done or failed
        if end:
            if "skeleton_preparing" in end or "ferry_preparing" in end:
                return "preparing", end, time.time() - t0, peak
            return ("done" if done else "failed"), end, time.time() - t0, peak
        time.sleep(3)
    return "timeout", "", time.time() - t0, peak


def check_inputs(text, trip, avoid_ferries, datex, long_trip=True):
    """Compare what the plan received with what was sent; return mismatches."""
    bad = []
    pi = last_line(text, "plan_inputs ")
    ps = last_line(text, "plan_settings ")
    st = last_line(text, "planning_start ")
    if not pi:
        bad.append("no plan_inputs line")
    if not ps:
        bad.append("no plan_settings line")
    if not st:
        bad.append("no planning_start line")
    want_pi = {
        "profile": "Car",
        "eco": "false",
        "avoid_motorways": "false",
        "avoid_ferries": str(avoid_ferries).lower(),
        "avoid_tunnels": "false",
        "toll": "Allow",
        "long_trip": "true" if long_trip else "false",
        "datex": DATEX_MODES[datex],
        "vias": str(len(trip["vias"])),
    }
    for k, v in want_pi.items():
        got = field(pi, k)
        if got != v:
            bad.append(f"plan_inputs {k}={got} (sent {v})")
    coords = [c for c in (field(pi, "via_coords") or "").split(";") if c]
    got_vias = [tuple(map(float, c.split(","))) for c in coords]
    if len(got_vias) != len(trip["vias"]) or any(
        abs(g[0] - w[0]) > 1e-5 or abs(g[1] - w[1]) > 1e-5 for g, w in zip(got_vias, trip["vias"])
    ):
        bad.append(f"plan_inputs via_coords={coords} (sent {[v[:2] for v in trip['vias']]})")
    want_ps = {
        "eco": "false",
        "camping_plugin": "false",
        "avoid_motorways": "false",
        "avoid_tolls": "false",
        "avoid_ferries": str(avoid_ferries).lower(),
        "avoid_tunnels": "false",
    }
    for k, v in want_ps.items():
        got = field(ps, k)
        if got != v:
            bad.append(f"plan_settings {k}={got} (sent {v})")
    if field(st, "profile") != "car":
        bad.append(f"planning_start profile={field(st, 'profile')} (sent car)")
    if field(st, "legs") != str(len(trip["vias"]) + 1):
        bad.append(f"planning_start legs={field(st, 'legs')} (sent {len(trip['vias']) + 1})")
    return bad, {"plan_inputs": pi, "plan_settings": ps, "planning_start": st}


def pull(out):
    got = {}
    for base in FILES_DIRS:
        for name in ("route-result.json", "route-polyline.txt", "hops.json", "routing-plan.log"):
            if name in got:
                continue
            for sub in ("long-trip-ui-report", ""):
                src = f"{base}/{sub}/{name}" if sub else f"{base}/{name}"
                dest = out / name
                r = adb("pull", src, str(dest))
                if r.returncode == 0 and dest.is_file():
                    got[name] = dest
                    break
    return got


def place_index_facts():
    """Place index on the pack volume, as InstalledMaps names it: path, size,
    quick_check and rows per region. Read-only; never creates a file."""
    snap = (
        adb("shell", "run-as", PKG, "cat", "files/installed-maps-snapshot.txt").stdout
        or adb("shell", "cat", f"/data/user/0/{PKG}/files/installed-maps-snapshot.txt").stdout
        or ""
    )
    m = re.search(r"^place_index vol=(\S+) path=(\S+)", snap, re.M)
    if not m:
        unavail = re.search(r"^place_index UNAVAILABLE.*$", snap, re.M)
        return {"error": unavail.group(0) if unavail else "no place_index line in InstalledMaps snapshot"}
    vol, path = m.group(1), m.group(2)
    rec = {"volume": vol, "path": path}
    size = (adb("shell", f"stat -c %s {path} 2>/dev/null").stdout or "").strip()
    rec["bytes"] = int(size) if size.isdigit() else None
    if not rec["bytes"]:
        return rec
    uri = f"'file:{path}?mode=ro'"
    qc = adb("shell", f"sqlite3 -readonly {uri} 'PRAGMA quick_check;'", timeout=900)
    rec["quick_check"] = (qc.stdout or qc.stderr or "").strip()
    # After an APK replace the app may still hold the WAL. GROUP BY 1 on a 2 GB
    # file then often returns empty stdout (locked / busy) while quick_check
    # already succeeded. Retry, group by the column name, and fall back to
    # per-region counts so the stored ten-region check is not empty.
    sql = "SELECT region_id, COUNT(*) FROM name_entries GROUP BY region_id;"
    counts = {}
    err = ""
    for _ in range(4):
        rows = adb("shell", f"sqlite3 -readonly {uri} '{sql}'", timeout=900)
        err = (rows.stderr or "").strip()
        counts = {}
        for ln in (rows.stdout or "").splitlines():
            rid, _, n = ln.rpartition("|")
            if rid and n.strip().isdigit():
                counts[rid] = int(n)
        if counts:
            break
        time.sleep(2)
    if not counts:
        rec["rows_error"] = err or "group_by_empty"
        stored = [
            "europe/denmark",
            "europe/germany/hamburg",
            "europe/germany/niedersachsen",
            "europe/germany/schleswig-holstein",
            "europe/norway/ostlandet",
            "europe/norway/sorlandet",
            "europe/norway/vestlandet",
            "europe/sweden/halland",
            "europe/sweden/skane",
            "europe/sweden/vastra_gotaland",
        ]
        for rid in stored:
            one = adb(
                "shell",
                f"sqlite3 -readonly {uri} \"SELECT COUNT(*) FROM name_entries WHERE region_id='{rid}';\"",
                timeout=120,
            )
            n = (one.stdout or "").strip()
            if n.isdigit():
                counts[rid] = int(n)
    rec["rows"] = counts
    return rec


def summarize(files, trip, text):
    """Route facts from the pulled artifacts; the stored report is cut to 2000
    chars, so ferries come from `enumerations` and Stage B legs from logcat."""
    rec = {}
    rr = files.get("route-result.json")
    fp = ""
    if rr:
        d = json.loads(rr.read_text())
        rec["distance_km"] = d.get("distance_km")
        fp = (d.get("enumerations") or {}).get("route_ferry_fp") or ""
    ferries = set()
    for part in fp.split("|"):
        label = part.split("@")[0].strip()
        if label:
            ferries.add(" | ".join(sorted(e.strip().lower() for e in label.split(" - "))))
    rec["ferries"] = sorted(ferries)
    rec["stage_b_legs"] = [
        ln[ln.index("stage_b leg=") :] for ln in text.splitlines() if "stage_b leg=" in ln
    ]
    poly = files.get("route-polyline.txt")
    pts = []
    if poly:
        for p in poly.read_text().strip().split(";"):
            try:
                lon, lat = map(float, p.split(",")[:2])
                pts.append((lat, lon))
            except ValueError:
                pass
    rec["polyline_km"] = round(sum(haversine_m(a, b) for a, b in zip(pts, pts[1:])) / 1000, 1)
    rec["via_m"] = [
        round(min(point_segment_m(v[:2], a, b) for a, b in zip(pts, pts[1:])), 1) if len(pts) > 1 else None
        for v in trip["vias"]
    ]
    hops = files.get("hops.json")
    if hops:
        rec["hops"] = len(json.loads(hops.read_text()).get("hops") or [])
    return rec


FU49_CAMERAS = [
    ("hamar", 60.79472, 11.06806),
    ("oslo", 59.91333, 10.73897),
    ("hamburg", 53.55034, 9.99368),
    ("hallingdal_bromma", 60.50, 9.17),
]
FU49_Q1 = [
    ("elsa_overview", 64.88873, 19.51604, 15),
    ("oslo", 59.91333, 10.73897, 5),
]
OSTLANDET = "/data/user/0/no.navi.app/files/pmtiles/europe_norway_ostlandet.pmtiles"


def half_screen_deg(lat, zoom, px=540):
    earth = 40075016.686
    m_per_px = earth * math.cos(math.radians(lat)) / (256.0 * (2.0 ** zoom))
    return (m_per_px * px) / 111195.0


def set_true_offline(on):
    """Airplane plus in-app force-offline. Flight mode alone left Wi-Fi up."""
    set_airplane(on)
    if on:
        adb("shell", "svc", "wifi", "disable")
        adb("shell", "svc", "data", "disable")
    else:
        adb("shell", "svc", "wifi", "enable")
        adb("shell", "svc", "data", "enable")
    extras = [
        "shell", "am", "start", "-n", f"{PKG}/.MainActivity",
        "--ez", "navi_fu49_force_offline", "true" if on else "false",
        "--ez", "navi_hide_chrome", "true",
    ]
    adb(*extras)
    time.sleep(1)


def camera_to_fu49(lat, lon, zoom, extra=None):
    args = [
        "shell", "am", "start",
        "-a", "android.intent.action.VIEW",
        "-f", "0x20000000",
        "-n", f"{PKG}/.MainActivity",
        "--ez", "navi_hide_chrome", "true",
        "--ed", "navi_camera_lat", str(lat),
        "--ed", "navi_camera_lon", str(lon),
        "--ed", "navi_camera_zoom", str(zoom),
    ]
    if extra:
        args.extend(extra)
    adb(*args)


def camera_held_matches(text, lat, lon, zoom):
    for ln in reversed(text.splitlines()):
        if "NaviMapTiles" in ln and "lat=" in ln:
            try:
                clat = float(field(ln, "lat") or "nan")
                clon = float(field(ln, "lon") or "nan")
                cz = float(field(ln, "zoom") or "nan")
            except ValueError:
                continue
            return abs(clat - lat) <= 0.03 and abs(clon - lon) <= 0.03 and abs(cz - zoom) <= 0.35
        if "NaviMapCamera" in ln and "at lat=" in ln:
            try:
                clat = float(field(ln, "lat") or "nan")
                clon = float(field(ln, "lon") or "nan")
                cz = float(field(ln, "zoom") or "nan")
            except ValueError:
                continue
            return abs(clat - lat) <= 0.03 and abs(clon - lon) <= 0.03 and abs(cz - zoom) <= 0.35
    return False


def parse_fu49_logs(text):
    totals = None
    idle_ms = None
    fully_ms = None
    net = None
    style_lines = []
    for ln in text.splitlines():
        if "NaviMapTotals" in ln and "roads=" in ln:
            totals = {
                "roads": int(field(ln, "roads") or 0),
                "water": int(field(ln, "water") or 0),
                "labels": int(field(ln, "labels") or 0),
                "blank_cells": int(field(ln, "blank_cells") or 0),
                "elapsed_ms": field(ln, "elapsed_ms"),
                "idle_ms": field(ln, "idle_ms"),
                "fully_ms": field(ln, "fully_ms"),
                "line": ln,
            }
        if "NaviFu49Event" in ln and "kind=idle" in ln:
            try:
                idle_ms = int(field(ln, "elapsed_ms") or -1)
            except ValueError:
                idle_ms = None
        if "NaviFu49Event" in ln and "kind=fully" in ln:
            try:
                fully_ms = int(field(ln, "elapsed_ms") or -1)
            except ValueError:
                fully_ms = None
        if "NaviFu49Net" in ln:
            net = {
                "airplane": field(ln, "airplane"),
                "has_internet": field(ln, "has_internet"),
                "usable": field(ln, "usable"),
                "force_offline": field(ln, "force_offline"),
                "http_allowed": field(ln, "http_allowed"),
                "line": ln,
            }
        if "NaviFu49Style" in ln:
            style_lines.append(ln)
    return {
        "totals": totals,
        "idle_ms": idle_ms,
        "fully_ms": fully_ms,
        "net": net,
        "style_lines": style_lines[-8:],
    }


def wait_fu49(seconds=20, want_fully=False):
    last = {}
    deadline = time.time() + seconds
    while time.time() < deadline:
        time.sleep(0.5)
        parsed = parse_fu49_logs(logcat())
        if parsed.get("totals"):
            last = parsed
            if want_fully:
                if parsed.get("fully_ms") not in (None, -1, "-1"):
                    if time.time() + 0.1 >= deadline:
                        return parsed
            else:
                return parsed
    return last or parse_fu49_logs(logcat())


def wait_fu49_settle(lat=None, lon=None, zoom=None, extra=None, seconds=20):
    """Settled when feature totals match on two samples one second apart."""
    prev_line = None
    prev_key = None
    last = {}
    deadline = time.time() + seconds
    while time.time() < deadline:
        if lat is not None and lon is not None and zoom is not None:
            text = logcat()
            if not camera_held_matches(text, lat, lon, zoom):
                camera_to_fu49(lat, lon, zoom, extra)
                time.sleep(1.0)
                continue
        parsed = parse_fu49_logs(logcat())
        tot = parsed.get("totals") or {}
        line = tot.get("line") or ""
        key = (tot.get("roads"), tot.get("water"), tot.get("labels")) if tot else None
        if line and key is not None and line != prev_line:
            empty = key == (0, 0, 0)
            # A blank grid can repeat while the camera or style is still
            # catching up (Taastrup online at 2.2 s). Wait for features.
            if empty:
                prev_key = None
                prev_line = line
                last = parsed
                time.sleep(1.0)
                continue
            if prev_key == key:
                return parsed
            prev_key = key
            prev_line = line
            last = parsed
            time.sleep(1.0)
            continue
        time.sleep(0.5)
    return last or parse_fu49_logs(logcat())


def pull_fu49_styles(dest):
    dest = pathlib.Path(dest)
    dest.mkdir(parents=True, exist_ok=True)
    adb(
        "shell",
        "run-as",
        PKG,
        "sh",
        "-c",
        "ls files/map-styles/protomaps-light",
    )
    for name in (
        "fu49-live-applied.json",
        "fu49-live-same-uri.json",
        "fu49-generated-last.json",
        ".asset_epoch",
    ):
        r = adb(
            "exec-out",
            "run-as",
            PKG,
            "cat",
            f"files/map-styles/protomaps-light/{name}",
        )
        if r.returncode == 0 and r.stdout:
            (dest / name).write_bytes(r.stdout.encode() if isinstance(r.stdout, str) else r.stdout)


def screencap_host(path):
    path = pathlib.Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = f"/sdcard/Download/{path.name}"
    adb("shell", "screencap", "-p", tmp)
    adb("pull", tmp, str(path))
    adb("shell", "rm", "-f", tmp)


def shoot_away_and_back(dest, lat, lon, zoom, extra=None, wait_s=20):
    dest = pathlib.Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    extra = list(extra or [])
    adb("logcat", "-c")
    camera_to_fu49(lat, lon, zoom, extra)
    idle = wait_fu49_settle(lat, lon, zoom, extra, wait_s)
    if dest.suffix == ".png":
        idle_png = dest.parent / (dest.stem + "_idle.png")
        back_png = dest.parent / (dest.stem + "_back.png")
    else:
        idle_png = dest / "idle.png"
        back_png = dest / "back.png"
    screencap_host(idle_png)
    dlat = half_screen_deg(lat, zoom)
    adb("logcat", "-c")
    camera_to_fu49(lat + dlat, lon, zoom, extra)
    wait_fu49_settle(lat + dlat, lon, zoom, extra, min(wait_s, 12))
    adb("logcat", "-c")
    camera_to_fu49(lat, lon, zoom, extra)
    back = wait_fu49_settle(lat, lon, zoom, extra, wait_s)
    screencap_host(back_png)
    return {
        "idle": idle,
        "back": back,
        "idle_png": str(idle_png),
        "back_png": str(back_png),
        "dlat": dlat,
    }


def style_source_urls(path):
    p = pathlib.Path(path)
    if not p.is_file() or p.stat().st_size == 0:
        return None
    try:
        data = json.loads(p.read_text())
    except json.JSONDecodeError:
        return None
    srcs = data.get("sources") or {}
    return {k: (v or {}).get("url") for k, v in srcs.items()}


FU51_HEAD_POSITIONS = [
    ("oslo", 59.91333, 10.73897),
    ("hamar", 60.79472, 11.06806),
    ("hallingdal_bromma", 60.50, 9.17),
    ("ostersund", 63.18, 14.64),
    ("ostlandet_varmland", 59.92, 12.29),
    ("hamburg", 53.55034, 9.99368),
]
FU51_HEAD_ZOOMS = (7, 9, 11, 13, 15)
FU51_HAMAR_ZOOMS = (3, 5, 7, 9, 11, 13, 15)
FU51_FINAL_POSITIONS = list(FU51_HEAD_POSITIONS) + [
    ("umea", 63.8258, 20.2630),
]
FU51_FINAL_ZOOMS = (3, 5, 7, 9, 11, 13, 15)
FU51_FINAL_OUTSIDE = (
    ("paris", 48.8566, 2.3522, 11),
    ("stockholm", 59.3293, 18.0686, 11),
)


def _fu51_totals(sample):
    tot = (sample or {}).get("totals") or {}
    return {
        "roads": tot.get("roads"),
        "water": tot.get("water"),
        "labels": tot.get("labels"),
        "blank_cells": tot.get("blank_cells"),
    }


def _fu51_agree(idle_t, back_t):
    def n(v):
        try:
            return int(v or 0)
        except (TypeError, ValueError):
            return 0

    ir, iw, il = n(idle_t.get("roads")), n(idle_t.get("water")), n(idle_t.get("labels"))
    br, bw, bl = n(back_t.get("roads")), n(back_t.get("water")), n(back_t.get("labels"))
    presence = (ir > 0) == (br > 0) and (iw > 0) == (bw > 0) and (il > 0) == (bl > 0)

    def within(a, b):
        if a == 0 and b == 0:
            return True
        m = max(a, b)
        return abs(a - b) <= 0.10 * m

    totals = within(ir, br) and within(iw, bw) and within(il, bl)
    return presence and totals


def run_fu51_head_matrix(out):
    out = pathlib.Path(out)
    out.mkdir(parents=True, exist_ok=True)
    rows = []
    extras = []
    # Map right after start, before the matrix moves the camera.
    start_png = out / "app_start.png"
    wait_fu49_settle(seconds=12)
    screencap_host(start_png)
    extras.append({"kind": "app_start", "file": start_png.name})
    for offline in (True, False):
        net = "off" if offline else "on"
        set_true_offline(offline)
        time.sleep(3)
        extra = ["--ez", "navi_fu49_force_offline", "true" if offline else "false"]
        try:
            for name, lat, lon in FU51_HEAD_POSITIONS:
                for z in FU51_HEAD_ZOOMS:
                    dest = out / f"{name}_z{z}_{net}"
                    dest.mkdir(parents=True, exist_ok=True)
                    shot = shoot_away_and_back(dest, lat, lon, z, extra=extra)
                    idle_t = _fu51_totals(shot.get("idle"))
                    back_t = _fu51_totals(shot.get("back"))
                    row = {
                        "position": name,
                        "lat": lat,
                        "lon": lon,
                        "zoom": z,
                        "network": net,
                        "idle": idle_t,
                        "back": back_t,
                        "idle_png": pathlib.Path(shot["idle_png"]).name,
                        "back_png": pathlib.Path(shot["back_png"]).name,
                        "agree": _fu51_agree(idle_t, back_t),
                        "net": (shot.get("idle") or {}).get("net"),
                    }
                    (dest / "counts.json").write_text(json.dumps(row, indent=2) + "\n")
                    rows.append(row)
                    log(
                        f"fu51-head {name} z{z} {net} "
                        f"idle_r={idle_t.get('roads')} back_r={back_t.get('roads')} agree={row['agree']}"
                    )
        finally:
            if offline:
                set_true_offline(False)
                time.sleep(2)
    # Hamar zoom sequence
    extra = ["--ez", "navi_fu49_force_offline", "false"]
    set_true_offline(False)
    for z in FU51_HAMAR_ZOOMS:
        dest = out / f"hamar_zoom_step_z{z}"
        dest.mkdir(parents=True, exist_ok=True)
        adb("logcat", "-c")
        camera_to_fu49(60.79472, 11.06806, z, extra)
        rec = wait_fu49_settle(60.79472, 11.06806, z, extra, 20)
        png = dest / "screen.png"
        # also publish a flat name for the table
        flat = out / f"hamar_zoom_step_z{z}.png"
        screencap_host(flat)
        extras.append({"kind": "zoom_step", "zoom": z, "file": flat.name, "totals": _fu51_totals(rec)})
        log(f"fu51-head hamar zoom {z} tot={_fu51_totals(rec)}")
    # Border pan z11
    blat, blon = 59.92, 12.29
    for i, dlon in enumerate((-0.35, -0.15, 0.0, 0.15, 0.35)):
        dest = out / f"border_pan_z11_{i}"
        dest.mkdir(parents=True, exist_ok=True)
        adb("logcat", "-c")
        camera_to_fu49(blat, blon + dlon, 11, extra)
        rec = wait_fu49_settle(blat, blon + dlon, 11, extra, 20)
        flat = out / f"border_pan_z11_{i}.png"
        screencap_host(flat)
        extras.append({"kind": "border_pan", "i": i, "file": flat.name, "totals": _fu51_totals(rec)})
        log(f"fu51-head border pan {i} tot={_fu51_totals(rec)}")
    # Elsa plan, after-plan, fit whole route, rotate
    clear_previous_plan()
    adb("logcat", "-c")
    start_plan(TRIPS["elsa"], False, "none")
    status, end, wall, _peak = wait_plan(240)
    log(f"fu51-head elsa plan {status} after {wall:.0f}s")
    after = out / "elsa_after_plan.png"
    time.sleep(4)
    wait_fu49_settle(seconds=16)
    screencap_host(after)
    extras.append({"kind": "elsa_after_plan", "file": after.name, "plan_status": status})
    # Whole route on screen: frame Elsa to Sjuvass without a map touch.
    fit_lat, fit_lon, fit_z = 64.88873, 19.51604, 4
    adb("logcat", "-c")
    camera_to_fu49(fit_lat, fit_lon, fit_z, extra)
    rec = wait_fu49_settle(fit_lat, fit_lon, fit_z, extra, 20)
    fit = out / "elsa_route_fit.png"
    screencap_host(fit)
    extras.append({"kind": "elsa_route_fit", "file": fit.name, "totals": _fu51_totals(rec)})
    # Rotate at Oslo z11
    camera_to_fu49(59.91333, 10.73897, 11, extra)
    wait_fu49_settle(59.91333, 10.73897, 11, extra, 16)
    adb("shell", "settings", "put", "system", "accelerometer_rotation", "0")
    adb("shell", "settings", "put", "system", "user_rotation", "1")
    time.sleep(3)
    rot = out / "oslo_z11_rotated.png"
    screencap_host(rot)
    extras.append({"kind": "rotate", "file": rot.name})
    adb("shell", "settings", "put", "system", "user_rotation", "0")
    time.sleep(2)
    return {"rows": rows, "extras": extras}


def run_fu51_final_matrix(out):
    out = pathlib.Path(out)
    out.mkdir(parents=True, exist_ok=True)
    rows = []
    extras = []
    for offline, net in ((True, "off"), (False, "on")):
        extra = ["--ez", "navi_fu49_force_offline", "true" if offline else "false"]
        set_true_offline(offline)
        time.sleep(2)
        try:
            for name, lat, lon in FU51_FINAL_POSITIONS:
                for z in FU51_FINAL_ZOOMS:
                    dest = out / f"{name}_z{z}_{net}"
                    dest.mkdir(parents=True, exist_ok=True)
                    shot = shoot_away_and_back(dest, lat, lon, z, extra=extra)
                    idle_t = _fu51_totals(shot.get("idle"))
                    back_t = _fu51_totals(shot.get("back"))
                    row = {
                        "position": name,
                        "lat": lat,
                        "lon": lon,
                        "zoom": z,
                        "network": net,
                        "idle": idle_t,
                        "back": back_t,
                        "idle_png": pathlib.Path(shot["idle_png"]).name,
                        "back_png": pathlib.Path(shot["back_png"]).name,
                        "agree": _fu51_agree(idle_t, back_t),
                    }
                    (dest / "counts.json").write_text(json.dumps(row, indent=2) + "\n")
                    rows.append(row)
                    log(
                        f"fu51-final {name} z{z} {net} "
                        f"idle_r={idle_t.get('roads')} back_r={back_t.get('roads')} agree={row['agree']}"
                    )
        finally:
            if offline:
                set_true_offline(False)
                time.sleep(2)
    extra = ["--ez", "navi_fu49_force_offline", "false"]
    set_true_offline(False)
    for name, lat, lon, z in FU51_FINAL_OUTSIDE:
        dest = out / f"{name}_z{z}_on"
        dest.mkdir(parents=True, exist_ok=True)
        shot = shoot_away_and_back(dest, lat, lon, z, extra=extra)
        idle_t = _fu51_totals(shot.get("idle"))
        extras.append(
            {
                "kind": "outside",
                "position": name,
                "zoom": z,
                "idle": idle_t,
                "agree": _fu51_agree(idle_t, _fu51_totals(shot.get("back"))),
            }
        )
        log(f"fu51-final {name} z{z} on idle_r={idle_t.get('roads')}")
    return {"rows": rows, "extras": extras}


def run_fu49_q1(out):
    out = pathlib.Path(out)
    rows = []
    for name, lat, lon, z in FU49_Q1:
        adb("logcat", "-c")
        camera_to_fu49(lat, lon, z)
        rec = wait_fu49_settle(lat, lon, z, None, 20)
        dest = out / f"{name}_z{z}"
        dest.mkdir(parents=True, exist_ok=True)
        pull_fu49_styles(dest)
        screencap_host(dest / "screen.png")
        gen = style_source_urls(dest / "fu49-generated-last.json")
        live = style_source_urls(dest / "fu49-live-applied.json") or style_source_urls(
            dest / "fu49-live-same-uri.json"
        )
        match = gen is not None and live is not None and gen == live
        (dest / "log.json").write_text(json.dumps(rec, indent=2) + "\n")
        row = {
            "name": name,
            "zoom": z,
            "log": rec,
            "generated": gen,
            "live": live,
            "style_match": match,
        }
        rows.append(row)
        log(f"q1 {name} z{z} style_match={match} totals={rec.get('totals')}")
    return {"rows": rows}


def run_fu49_away_back(out, build):
    out = pathlib.Path(out)
    rows = []
    for offline in (True, False):
        net = "off" if offline else "on"
        set_true_offline(offline)
        time.sleep(4)
        extra = ["--ez", "navi_fu49_force_offline", "true" if offline else "false"]
        try:
            for name, lat, lon in FU49_CAMERAS:
                dest = out / f"{name}_z11_{net}"
                dest.mkdir(parents=True, exist_ok=True)
                shot = shoot_away_and_back(dest, lat, lon, 11, extra=extra)
                idle_t = (shot["idle"] or {}).get("totals") or {}
                back_t = (shot["back"] or {}).get("totals") or {}
                row = {
                    "build": build,
                    "name": name,
                    "network": net,
                    "idle_roads": idle_t.get("roads"),
                    "idle_water": idle_t.get("water"),
                    "idle_labels": idle_t.get("labels"),
                    "back_roads": back_t.get("roads"),
                    "back_water": back_t.get("water"),
                    "back_labels": back_t.get("labels"),
                    "idle_ms": (shot["idle"] or {}).get("idle_ms"),
                    "fully_ms": (shot["idle"] or {}).get("fully_ms"),
                    "back_idle_ms": (shot["back"] or {}).get("idle_ms"),
                    "back_fully_ms": (shot["back"] or {}).get("fully_ms"),
                    "net": (shot["idle"] or {}).get("net"),
                    "underdrawn": (idle_t.get("roads") or 0) + 20 < (back_t.get("roads") or 0),
                    "idle_back_match": (
                        idle_t.get("roads") == back_t.get("roads")
                        and idle_t.get("water") == back_t.get("water")
                        and idle_t.get("labels") == back_t.get("labels")
                    ),
                }
                (dest / "counts.json").write_text(json.dumps(row, indent=2) + "\n")
                rows.append(row)
                log(
                    f"{build} {name} {net} idle_r={row['idle_roads']} back_r={row['back_roads']} "
                    f"underdrawn={row['underdrawn']} net={row['net']}"
                )
        finally:
            if offline:
                set_true_offline(False)
                time.sleep(1)
    return {"build": build, "rows": rows}


def run_fu49_q3(out):
    out = pathlib.Path(out)
    steps = [
        ("01_single", ["--ez", "navi_fu49_simple_mount", "true",
                       "--ez", "navi_fu49_bypass_queue", "true",
                       "--ez", "navi_fu49_disable_keep_previous", "true",
                       "--ez", "navi_fu49_force_offline", "true",
                       "--es", "navi_force_basemap_source", OSTLANDET]),
        ("02_queue", ["--ez", "navi_fu49_simple_mount", "true",
                      "--ez", "navi_fu49_bypass_queue", "false",
                      "--ez", "navi_fu49_disable_keep_previous", "true",
                      "--ez", "navi_fu49_force_offline", "true",
                      "--es", "navi_force_basemap_source", OSTLANDET]),
        ("03_keep_previous", ["--ez", "navi_fu49_simple_mount", "true",
                              "--ez", "navi_fu49_bypass_queue", "false",
                              "--ez", "navi_fu49_disable_keep_previous", "false",
                              "--ez", "navi_fu49_force_offline", "true"]),
        ("04_overview", ["--ez", "navi_fu49_simple_mount", "true",
                         "--ez", "navi_fu49_overview", "true",
                         "--ez", "navi_fu49_force_offline", "true"]),
        ("05_second_regional", ["--ez", "navi_fu49_simple_mount", "true",
                                "--ez", "navi_fu49_overview", "true",
                                "--ez", "navi_fu49_second_regional", "true",
                                "--ez", "navi_fu49_force_offline", "true"]),
        ("06_online", ["--ez", "navi_fu49_simple_mount", "true",
                       "--ez", "navi_fu49_overview", "true",
                       "--ez", "navi_fu49_second_regional", "true",
                       "--ez", "navi_fu49_online", "true",
                       "--ez", "navi_fu49_force_offline", "false"]),
    ]
    set_true_offline(True)
    lat, lon, z = 60.79472, 11.06806, 11
    recs = []
    try:
        for label, extra in steps:
            dest = out / label
            dest.mkdir(parents=True, exist_ok=True)
            if label == "06_online":
                set_true_offline(False)
            adb("logcat", "-c")
            camera_to_fu49(lat, lon, z, extra)
            early = wait_fu49(8, want_fully=False)
            fully = wait_fu49(20, want_fully=True)
            screencap_host(dest / "idle.png")
            pull_fu49_styles(dest)
            (dest / "early.json").write_text(json.dumps(early, indent=2) + "\n")
            (dest / "fully.json").write_text(json.dumps(fully, indent=2) + "\n")
            recs.append({"label": label, "early": early, "fully": fully})
            log(f"q3 {label} early={early.get('totals')} fully={fully.get('totals')}")
    finally:
        set_true_offline(False)
        adb(
            "shell", "am", "start", "-n", f"{PKG}/.MainActivity",
            "--ez", "navi_clear_basemap_test_hooks", "true",
        )
    return {"steps": recs}


def main():
    global SERIAL
    ap = argparse.ArgumentParser()
    ap.add_argument("trip", nargs="?", choices=sorted(TRIPS))
    ap.add_argument("--datex", choices=sorted(DATEX_MODES), default="none")
    ap.add_argument("--avoid-ferries", action="store_true")
    ap.add_argument("--out", required=True)
    ap.add_argument("--timeout-min", type=float, default=60)
    ap.add_argument("--search-check", action="store_true")
    ap.add_argument("--clear-region", action="append", default=[])
    ap.add_argument("--pause-test", action="store_true")
    ap.add_argument(
        "--long-trip-off",
        action="store_true",
        help="send navi_long_trip=false (default is true)",
    )
    ap.add_argument(
        "--plan-only",
        action="store_true",
        help="skip search, offline-map and display checks (short-route timing)",
    )
    ap.add_argument(
        "--fu44-map",
        action="store_true",
        help="idle/nudge blank-share screenshot matrix into --out (docs/fu44-map)",
    )
    ap.add_argument(
        "--fu46-map",
        action="store_true",
        help="idle/nudge native-source screenshot matrix into --out (docs/fu46-map)",
    )
    ap.add_argument(
        "--fu47-map",
        action="store_true",
        help="follow-up 47 screenshot matrix into --out (docs/fu47-map)",
    )
    ap.add_argument(
        "--fu49-map",
        action="store_true",
        help="follow-up 49 diagnosis: away-and-back + style dumps into --out",
    )
    ap.add_argument(
        "--fu49-q1",
        action="store_true",
        help="dump live vs generated style at Elsa z15 and Oslo z5",
    )
    ap.add_argument(
        "--fu49-q3",
        action="store_true",
        help="single-source Hamar z11 then add elements one at a time",
    )
    ap.add_argument(
        "--fu51-map",
        action="store_true",
        help="follow-up 51 head proof matrix into --out (docs/fu51-map/head)",
    )
    ap.add_argument(
        "--fu51-final",
        action="store_true",
        help="follow-up 51 final proof matrix into --out (docs/fu51-map/final)",
    )
    ap.add_argument(
        "--fu49-build",
        default="head",
        help="label under docs/fu49-map/<build>/ for --fu49-map",
    )
    a = ap.parse_args()
    if not a.trip and not a.search_check and not a.clear_region and not a.pause_test and not a.fu44_map and not a.fu46_map and not a.fu47_map and not a.fu49_map and not a.fu49_q1 and not a.fu49_q3 and not a.fu51_map and not a.fu51_final:
        ap.error("trip is required unless --search-check, --clear-region, --pause-test, --fu44-map, --fu46-map, --fu47-map, --fu49-* or --fu51-map")
    trip = TRIPS.get(a.trip) if a.trip else None
    out = pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    SERIAL = serial()
    ensure_running()
    pid0 = pid()
    log(f"serial={SERIAL} pid={pid0}")
    if a.fu51_map:
        rec = run_fu51_head_matrix(out)
        (out / "result.json").write_text(json.dumps(rec, indent=2) + "\n")
        print(json.dumps({"rows": len(rec.get("rows", [])), "extras": len(rec.get("extras", []))}, indent=2))
        sys.exit(0)
    if a.fu51_final:
        rec = run_fu51_final_matrix(out)
        (out / "result.json").write_text(json.dumps(rec, indent=2) + "\n")
        print(json.dumps({"rows": len(rec.get("rows", [])), "extras": len(rec.get("extras", []))}, indent=2))
        sys.exit(0)
    if a.fu49_q1:
        rec = run_fu49_q1(out)
        (out / "q1.json").write_text(json.dumps(rec, indent=2) + "\n")
        print(json.dumps(rec, indent=2))
        sys.exit(0 if all(r.get("style_match") for r in rec.get("rows", [])) else 1)
    if a.fu49_q3:
        rec = run_fu49_q3(out)
        (out / "q3.json").write_text(json.dumps(rec, indent=2) + "\n")
        print(json.dumps({"steps": [s.get("label") for s in rec.get("steps", [])]}, indent=2))
        sys.exit(0)
    if a.fu49_map:
        rec = run_fu49_away_back(out, a.fu49_build)
        fails = [r for r in rec.get("rows", []) if not r.get("idle_back_match")]
        rec["accepted"] = not fails
        rec["fail_count"] = len(fails)
        (out / "result.json").write_text(json.dumps(rec, indent=2) + "\n")
        print(json.dumps({"build": a.fu49_build, "accepted": rec["accepted"], "fail_count": rec["fail_count"]}, indent=2))
        sys.exit(0 if rec["accepted"] else 1)
    if a.fu47_map:
        rows, extras = run_fu47_map_matrix(out)
        fails = [
            r
            for r in rows
            if not r.get("ok")
        ]
        rec = {
            "accepted": not fails,
            "rows": rows,
            "extras": extras,
            "fail_count": len(fails),
            "known_failures": list(KNOWN_MAP_FAILURES),
        }
        (out / "result.json").write_text(json.dumps(rec, indent=2) + "\n")
        print(
            json.dumps(
                {
                    "accepted": rec["accepted"],
                    "fail_count": rec["fail_count"],
                    "known_failures": rec["known_failures"],
                },
                indent=2,
            )
        )
        sys.exit(0 if rec["accepted"] else 1)
    if a.fu46_map:
        rows, extras = run_fu46_map_matrix(out)
        fails = [
            r
            for r in rows
            if not r.get("camera_idle_ok")
            or not r.get("camera_nudge_ok")
            or not r.get("grid_agree")
        ]
        rec = {
            "accepted": not fails,
            "rows": rows,
            "extras": extras,
            "fail_count": len(fails),
        }
        (out / "result.json").write_text(json.dumps(rec, indent=2) + "\n")
        print(json.dumps({"accepted": rec["accepted"], "fail_count": rec["fail_count"]}, indent=2))
        sys.exit(0 if rec["accepted"] else 1)
    if a.fu44_map:
        rows = run_fu44_map_matrix(out)
        fails = [r for r in rows if not r.get("pair_ok")]
        rec = {"accepted": not fails, "rows": rows, "fail_count": len(fails)}
        (out / "result.json").write_text(json.dumps(rec, indent=2) + "\n")
        print(json.dumps({"accepted": rec["accepted"], "fail_count": rec["fail_count"]}, indent=2))
        sys.exit(0 if rec["accepted"] else 1)
    if a.pause_test and not trip and not a.search_check:
        index = {}
    else:
        index = place_index_facts()
        log(f"place index: {index.get('path')} bytes={index.get('bytes')} quick_check={index.get('quick_check')}")

    cleared = []
    for rid in a.clear_region:
        cleared.append({"region": rid, "line": clear_place_index_region(rid)})
        index = place_index_facts()

    run_pre = (a.search_check or trip) and not a.plan_only
    searches = run_search_check() if run_pre else []
    search_fail = [s for s in searches if not s.get("ok")]
    maps = run_offline_map_check() if run_pre else []
    map_fail = [m for m in maps if not m.get("ok")]
    display = run_display_checks() if run_pre else None
    display_fail = bool(display) and (
        display.get("hooks_cleared") is False
        or not all(
            display.get(k, {}).get("ok", False) for k in ("hop", "border_pan", "plan_no_blank")
        )
    )
    pause = run_pause_test() if a.pause_test else None
    pause_fail = bool(pause) and not pause.get("ok")

    if not trip:
        rec = {
            "status": "done",
            "accepted": not search_fail and not pause_fail and not map_fail and not display_fail,
            "place_index": index,
            "searches": searches,
            "offline_map": maps,
            "display_checks": display,
            "cleared": cleared,
            "pause_test": pause,
        }
        (out / "result.json").write_text(json.dumps(rec, indent=2) + "\n")
        print(json.dumps(rec, indent=2))
        sys.exit(0 if rec["accepted"] else 1)

    deadline = time.time() + a.timeout_min * 60
    while True:
        clear_previous_plan()
        adb("logcat", "-c")
        peak_reset = reset_peak()
        start_plan(trip, a.avoid_ferries, a.datex, long_trip=not a.long_trip_off)
        status, end, wall, peak_kb = wait_plan(deadline - time.time())
        log(f"{status} after {wall:.0f} s: {end[-200:]}")
        if status != "preparing" or time.time() > deadline:
            break
        log("sidecars preparing in the app; waiting 30 s before replanning")
        time.sleep(30)

    text = logcat()
    (out / "logcat.txt").write_text(text)
    bad, lines = check_inputs(text, trip, a.avoid_ferries, a.datex, long_trip=not a.long_trip_off)
    files = pull(out)
    stages, native_peak = plan_stage_peaks(text)
    sampled_mb = round(peak_kb / 1024) if peak_kb else 0
    plan_peak = max(sampled_mb, round(native_peak)) if (sampled_mb or native_peak) else None
    rec = {
        "trip": a.trip,
        "datex": a.datex,
        "avoid_ferries": a.avoid_ferries,
        "status": status,
        "wall_s": round(wall, 1),
        "pid_start": pid0,
        "pid_end": pid(),
        "plan_peak_mb": plan_peak,
        "process_peak_mb": round((proc_kb("VmHWM") or 0) / 1024) or None,
        "peak_reset": peak_reset,
        "plan_stage_rss_mb": stages,
        "input_lines": lines,
        "input_mismatch": bad,
        "place_index": index,
        "searches": searches,
        "offline_map": maps,
        "display_checks": display,
        "cleared": cleared,
        "pause_test": pause,
        "idle_job_pause": last_line(text, "idle_job_pause "),
    }
    rec.update(summarize(files, trip, text))
    rec["accepted"] = (
        not bad
        and not search_fail
        and not map_fail
        and not display_fail
        and not pause_fail
        and status == "done"
    )
    (out / "result.json").write_text(json.dumps(rec, indent=2) + "\n")
    print(json.dumps(rec, indent=2))
    if bad:
        log("REJECTED: the plan did not receive what was sent")
        sys.exit(2)
    if search_fail:
        log("REJECTED: search check failed")
        sys.exit(2)
    if map_fail:
        log("REJECTED: map over a gate-route start/via/destination was blank")
        sys.exit(2)
    if display_fail:
        log("REJECTED: hop, border-pan or plan-no-blank display check failed")
        sys.exit(2)
    if pause_fail:
        log("REJECTED: pause test wrote the product index or failed")
        sys.exit(2)
    sys.exit(0 if status == "done" else 1)


if __name__ == "__main__":
    main()
