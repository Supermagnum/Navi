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
]

env = os.environ.copy()
sdk = pathlib.Path.home() / "Android/Sdk/platform-tools"
env["PATH"] = f"{sdk}:{env.get('PATH', '')}"


def serial():
    r = subprocess.run(["adb", "devices"], env=env, capture_output=True, text=True, timeout=30)
    for ln in (r.stdout or "").splitlines():
        parts = ln.split("\t")
        if len(parts) == 2 and parts[1] == "device":
            return parts[0]
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
    adb("shell", "cmd", "connectivity", "airplane-mode", mode)


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
    adb(
        "shell",
        "am",
        "start",
        "-n",
        f"{PKG}/.MainActivity",
        "--ez",
        "navi_hide_chrome",
        "true",
        "--ez",
        "navi_clear_basemap_test_hooks",
        "true",
        "--ed",
        "navi_camera_lat",
        str(lat),
        "--ed",
        "navi_camera_lon",
        str(lon),
        "--ed",
        "navi_camera_zoom",
        str(zoom),
    )


def wait_tiles(seconds=12):
    visible = None
    kind = ""
    line = ""
    min_visible = None
    for _ in range(int(seconds * 2)):
        time.sleep(0.5)
        text = logcat()
        for ln in text.splitlines():
            if "NaviMapTiles" in ln and "visible=" in ln:
                line = ln
                raw = field(ln, "visible") or "0"
                try:
                    visible = int(raw)
                except ValueError:
                    visible = 0
                kind = field(ln, "kind") or ""
                if min_visible is None or visible < min_visible:
                    min_visible = visible
        if (visible or 0) > 0:
            break
    return {
        "visible": visible or 0,
        "min_visible": min_visible if min_visible is not None else 0,
        "kind": kind,
        "line": line,
        "ok": (visible or 0) > 0,
    }


def run_offline_map_check():
    """Network off, then on: tiles at each gate-route start, via and destination."""
    out = []
    for airplane, net in ((True, "off"), (False, "on")):
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
        set_airplane(airplane)
        time.sleep(2)
        try:
            for name, spec in TRIPS.items():
                points = [("from", spec["from"])]
                points += [("via", v) for v in spec["vias"]]
                points.append(("to", spec["to"]))
                for role, (lat, lon, label) in points:
                    hooks = clear_forced_basemap()
                    adb("logcat", "-c")
                    camera_to(lat, lon)
                    rec = wait_tiles()
                    rec.update(
                        {
                            "trip": name,
                            "role": role,
                            "label": label,
                            "lat": lat,
                            "lon": lon,
                            "network": net,
                            "hooks_cleared": hooks.get("ok", False),
                        }
                    )
                    if not hooks.get("ok"):
                        rec["ok"] = False
                    out.append(rec)
                    log(
                        f"map_{net} {name} {role} {label} visible={rec['visible']} "
                        f"kind={rec['kind']} ok={rec['ok']}"
                    )
        finally:
            if airplane:
                set_airplane(False)
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
        "line": last["line"],
    }
    log(f"hop_three_regions elapsed={hop['elapsed_s']}s visible={last['visible']} ok={hop['ok']}")

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
        if not rec["ok"] or rec.get("min_visible", 0) == 0:
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
        "before": before["visible"],
        "during": during["visible"],
    }
    log(f"plan_no_blank before={plan['before']} during={plan['during']} ok={plan['ok']}")
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


def run_pause_test():
    """Index a small fixture into a scratch database, then remove both.

    The product place index is not opened for write.
    """
    if not TINY_PBF.is_file():
        return {"ok": False, "error": f"missing {TINY_PBF}"}
    remote_pbf = f"{PAUSE_SCRATCH}/tiny.osm.pbf"
    remote_db = f"{PAUSE_SCRATCH}/place_index.db"
    adb("shell", f"rm -rf {PAUSE_SCRATCH} && mkdir -p {PAUSE_SCRATCH}")
    pushed = adb("push", str(TINY_PBF), remote_pbf)
    if pushed.returncode != 0:
        return {"ok": False, "error": (pushed.stderr or pushed.stdout or "push failed").strip()}
    before = place_index_facts()
    adb("logcat", "-c")
    adb(
        "shell",
        "am",
        "start",
        "-n",
        f"{PKG}/.MainActivity",
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
    for _ in range(60):
        time.sleep(1)
        text = logcat()
        for ln in text.splitlines():
            if (
                "debug place-index queued" in ln
                or "refusing PLACE_INDEX" in ln
                or "place_index_build" in ln
                or "idle_job_pause" in ln
            ):
                line = ln
        if line and (
            "queued" in line
            or "action=built" in line
            or "refusing" in line
            or "PAUSED" in line
            or "idle_job_pause" in line
        ):
            break
    after = place_index_facts()
    product_rows_before = (before.get("rows") or {}).get("test/fu38-pause", 0)
    product_rows_after = (after.get("rows") or {}).get("test/fu38-pause", 0)
    scratch_ls = (adb("shell", f"ls -l {remote_db} {remote_db}-wal 2>/dev/null").stdout or "").strip()
    finished = False
    for _ in range(60):
        text = logcat()
        if any(
            s in text
            for s in (
                "PlaceIndexBg: finished",
                "PlaceIndexBg: paused",
                "job PLACE_INDEX",
                "FAIL:",
                "action=built",
            )
        ):
            finished = True
            break
        time.sleep(1)
    if not finished:
        log("pause-test: index job did not finish in 60s; leaving scratch until then")
    adb("shell", f"rm -rf {PAUSE_SCRATCH}")
    gone = (adb("shell", f"ls {PAUSE_SCRATCH} 2>/dev/null").stdout or "").strip() == ""
    queued = "queued" in line or "action=built" in line
    refused_product = "product=true" in line
    ok = queued and not refused_product and product_rows_after == 0 and product_rows_before == 0 and gone
    rec = {
        "ok": ok,
        "line": line,
        "scratch_db": remote_db,
        "scratch_listing": scratch_ls,
        "scratch_removed": gone,
        "product_test_fu38_pause_before": product_rows_before,
        "product_test_fu38_pause_after": product_rows_after,
    }
    log(f"pause-test ok={ok} product_rows={product_rows_after} scratch_removed={gone} {line[-160:]}")
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
    a = ap.parse_args()
    if not a.trip and not a.search_check and not a.clear_region and not a.pause_test:
        ap.error("trip is required unless --search-check, --clear-region or --pause-test")
    trip = TRIPS.get(a.trip) if a.trip else None
    out = pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    SERIAL = serial()
    ensure_running()
    pid0 = pid()
    log(f"serial={SERIAL} pid={pid0}")
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
