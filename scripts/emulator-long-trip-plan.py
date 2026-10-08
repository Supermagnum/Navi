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
`<out>/result.json` beside the pulled plan artifacts.

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
}

DATEX_MODES = {"none": "None", "saved": "Saved", "live": "Live"}

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
    """Reset VmHWM of the app process so the next read is this plan's peak."""
    p = pid()
    r = adb("shell", f"echo 5 > /proc/{p}/clear_refs && echo ok")
    if "ok" not in (r.stdout or ""):
        r = adb("shell", f"su 0 sh -c 'echo 5 > /proc/{p}/clear_refs' && echo ok")
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


def start_plan(trip, avoid_ferries, datex):
    f_lat, f_lon, f_name = trip["from"]
    t_lat, t_lon, t_name = trip["to"]
    args = [
        "shell", "am", "start", "-n", f"{PKG}/.MainActivity",
        "--ez", "navi_long_trip", "true",
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
        hwm = proc_kb("VmHWM") or 0
        peak = max(peak, hwm)
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


def check_inputs(text, trip, avoid_ferries, datex):
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
        "long_trip": "true",
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
    ap.add_argument("trip", choices=sorted(TRIPS))
    ap.add_argument("--datex", choices=sorted(DATEX_MODES), default="none")
    ap.add_argument("--avoid-ferries", action="store_true")
    ap.add_argument("--out", required=True)
    ap.add_argument("--timeout-min", type=float, default=60)
    a = ap.parse_args()
    trip = TRIPS[a.trip]
    out = pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    SERIAL = serial()
    ensure_running()
    pid0 = pid()
    log(f"serial={SERIAL} pid={pid0}")

    deadline = time.time() + a.timeout_min * 60
    while True:
        clear_previous_plan()
        adb("logcat", "-c")
        peak_reset = reset_peak()
        start_plan(trip, a.avoid_ferries, a.datex)
        status, end, wall, peak_kb = wait_plan(deadline - time.time())
        log(f"{status} after {wall:.0f} s: {end[-200:]}")
        if status != "preparing" or time.time() > deadline:
            break
        log("sidecars preparing in the app; waiting 30 s before replanning")
        time.sleep(30)

    text = logcat()
    (out / "logcat.txt").write_text(text)
    bad, lines = check_inputs(text, trip, a.avoid_ferries, a.datex)
    files = pull(out)
    rec = {
        "trip": a.trip,
        "datex": a.datex,
        "avoid_ferries": a.avoid_ferries,
        "status": status,
        "wall_s": round(wall, 1),
        "pid_start": pid0,
        "pid_end": pid(),
        "plan_peak_mb": round(peak_kb / 1024) if peak_reset and peak_kb else None,
        "process_peak_mb": round(peak_kb / 1024) if peak_kb else None,
        "peak_reset": peak_reset,
        "input_lines": lines,
        "input_mismatch": bad,
    }
    rec.update(summarize(files, trip, text))
    rec["accepted"] = not bad and status == "done"
    (out / "result.json").write_text(json.dumps(rec, indent=2) + "\n")
    print(json.dumps(rec, indent=2))
    if bad:
        log("REJECTED: the plan did not receive what was sent")
        sys.exit(2)
    sys.exit(0 if status == "done" else 1)


if __name__ == "__main__":
    main()
