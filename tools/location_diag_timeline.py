#!/usr/bin/env python3
"""Print a timeline from a pulled location-diag JSONL file.

Reads the rotating log written when Location diagnostics is on. Prints the
session header, GNSS fixes, dead-reckoning speed and along-track changes, and
IMU stillness. Does not embed coordinates or replay inputs of its own.
"""

import argparse
import json
import sys


def load(path):
    rows = []
    with open(path, encoding="utf-8") as handle:
        for lineno, line in enumerate(handle, 1):
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as exc:
                print(f"skip line {lineno}: {exc}", file=sys.stderr)
    return rows


def wall(row):
    return row.get("wall_ms") or 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("jsonl", help="path to a location-diag-*.jsonl file")
    parser.add_argument(
        "--along-drop-m",
        type=float,
        default=20.0,
        help="flag an along-track drop larger than this many metres",
    )
    args = parser.parse_args()
    rows = load(args.jsonl)
    if not rows:
        print("no records")
        return 1

    base = min(wall(r) for r in rows if wall(r))
    counts = {}
    for row in rows:
        kind = row.get("kind") or "?"
        counts[kind] = counts.get(kind, 0) + 1
    print("counts", counts)

    for row in rows:
        if row.get("kind") != "session":
            continue
        print(
            "session",
            "providers",
            row.get("providers"),
            "unusual",
            row.get("unusual_providers"),
            "test_provider",
            row.get("test_provider_registered"),
            "mock_appop",
            row.get("mock_location_appop"),
            "prediction",
            row.get("prediction"),
            "standstill_detector",
            row.get("standstill_detector"),
        )

    print("fixes")
    mock_fixes = 0
    for row in rows:
        if row.get("kind") != "fix":
            continue
        if row.get("is_mock"):
            mock_fixes += 1
        t = (wall(row) - base) / 1000.0
        print(
            f"  {t:8.1f}s {str(row.get('provider')):8} "
            f"acc={row.get('accuracy_m')} spd={row.get('speed_mps')} "
            f"mock={row.get('is_mock')}"
        )
    print("mock_fixes", mock_fixes)

    print("nav changes")
    prev_key = None
    prev_along = None
    max_along = 0.0
    drops = 0
    for row in rows:
        if row.get("kind") != "nav":
            continue
        along = row.get("dr_along_m")
        if isinstance(along, (int, float)):
            max_along = max(max_along, along)
            if (
                isinstance(prev_along, (int, float))
                and prev_along - along > args.along_drop_m
            ):
                drops += 1
                t = (wall(row) - base) / 1000.0
                print(
                    f"  DROP {t:8.1f}s {prev_along:.1f} -> {along:.1f} "
                    f"mode={row.get('dr_mode')} spd={row.get('dr_speed_mps')}"
                )
            prev_along = along
        speed = row.get("dr_speed_mps")
        speed_key = None if not isinstance(speed, (int, float)) else round(speed, 1)
        along_key = None if not isinstance(along, (int, float)) else round(along)
        key = (
            row.get("source"),
            row.get("dr_mode"),
            row.get("dr_blending"),
            speed_key,
            along_key,
            row.get("off_route"),
            row.get("reroute"),
        )
        if key == prev_key:
            continue
        prev_key = key
        t = (wall(row) - base) / 1000.0
        print(
            f"  {t:8.1f}s src={row.get('source')} mode={row.get('dr_mode')} "
            f"blend={row.get('dr_blending')} spd={speed} "
            f"dr_along={along} snap={row.get('snap_index')} "
            f"off={row.get('off_route')} reroute={row.get('reroute')} "
            f"sim={row.get('simulating')}"
        )
    print("max_dr_along_m", max_along, "along_drops", drops)

    imus = [r for r in rows if r.get("kind") == "imu"]
    if imus:
        def peak(name):
            vals = [r.get(name) for r in imus if isinstance(r.get(name), (int, float))]
            return max(vals) if vals else None

        print(
            "imu_samples",
            len(imus),
            "max_var_ax",
            peak("var_ax"),
            "max_var_ay",
            peak("var_ay"),
            "max_var_az",
            peak("var_az"),
            "max_abs_mean_ax",
            max(abs(r.get("mean_ax") or 0) for r in imus),
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
