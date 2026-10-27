#!/usr/bin/env python3
"""Anonymise a pulled location-diag JSONL for unit-test fixtures.

Shifts every latitude/longitude by a constant offset so the first fix lands on
a fixed public test origin (test data only; never used by app logic). Rebases
wall and elapsedRealtime timestamps to start at 0 while keeping every interval.
Drops fields the replay does not need and any device identifiers.

Usage:
  python3 tools/anonymise_location_diag.py \\
    path/to/raw.jsonl \\
    -o app/src/test/resources/location-diag-replay.jsonl
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any

# Public seaside plaza next to the Oslo Opera House. Chosen only as a fixed
# test origin so relative geometry survives the shift. Never read by app code.
PUBLIC_ORIGIN_LAT = 59.907462
PUBLIC_ORIGIN_LON = 10.753595

# Replay (StationaryDeadReckoningTest) needs these kinds and fields only.
KEEP_BY_KIND = {
    "session": {
        "kind",
        "wall_ms",
        "elapsed_realtime_ms",
        "providers",
        "providers_enabled",
        "unusual_providers",
        "test_provider_registered",
        "mock_location_appop",
        "prediction",
        "standstill_detector",
        "dr_good_accuracy_m",
        "dr_degrade_accuracy_m",
        "dr_degrade_hold_ms",
        "dr_min_speed_mps",
        "dr_accel_deadband_mps2",
    },
    "fix": {
        "kind",
        "wall_ms",
        "elapsed_realtime_ms",
        "provider",
        "lat",
        "lon",
        "accuracy_m",
        "speed_mps",
        "bearing_deg",
        "is_mock",
    },
    "imu": {
        "kind",
        "wall_ms",
        "elapsed_realtime_ms",
        "n_accel",
        "n_gyro",
        "mean_ax",
        "mean_ay",
        "mean_az",
        "mean_gx",
        "mean_gy",
        "mean_gz",
        "var_ax",
        "var_ay",
        "var_az",
        "var_gx",
        "var_gy",
        "var_gz",
    },
    "route": {
        "kind",
        "wall_ms",
        "elapsed_realtime_ms",
        "samples",
        "from_follows_gps",
        "route_not_joined",
    },
}

# Nav lines are diagnostic display state; the engine replay feeds only fix/imu/route.
DROP_KINDS = {"nav", "jump"}

LAT_LON_KEYS = (
    "lat",
    "lon",
    "dr_seed_lat",
    "dr_seed_lon",
    "reroute_origin_lat",
    "reroute_origin_lon",
)

TIME_KEYS = (
    "wall_ms",
    "elapsed_realtime_ms",
    "fix_time_ms",
    "fix_elapsed_realtime_nanos",
)


def load_rows(path: str) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with open(path, encoding="utf-8") as handle:
        for lineno, line in enumerate(handle, 1):
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as exc:
                raise SystemExit(f"{path}:{lineno}: {exc}") from exc
    return rows


def first_fix_origin(rows: list[dict[str, Any]]) -> tuple[float, float]:
    for row in rows:
        if row.get("kind") != "fix":
            continue
        lat = row.get("lat")
        lon = row.get("lon")
        if isinstance(lat, (int, float)) and isinstance(lon, (int, float)):
            return float(lat), float(lon)
    raise SystemExit("no fix with lat/lon found; cannot choose shift origin")


def time_bases(rows: list[dict[str, Any]]) -> tuple[int, int]:
    walls = [int(r["wall_ms"]) for r in rows if isinstance(r.get("wall_ms"), (int, float))]
    elapsed = [
        int(r["elapsed_realtime_ms"])
        for r in rows
        if isinstance(r.get("elapsed_realtime_ms"), (int, float))
    ]
    if not walls:
        raise SystemExit("no wall_ms timestamps found")
    return min(walls), min(elapsed) if elapsed else 0


def shift_lat_lon(row: dict[str, Any], d_lat: float, d_lon: float) -> None:
    for key in LAT_LON_KEYS:
        if isinstance(row.get(key), (int, float)):
            if "lat" in key:
                row[key] = float(row[key]) + d_lat
            else:
                row[key] = float(row[key]) + d_lon
    samples = row.get("samples")
    if isinstance(samples, list):
        for sample in samples:
            if not isinstance(sample, dict):
                continue
            if isinstance(sample.get("lat"), (int, float)):
                sample["lat"] = float(sample["lat"]) + d_lat
            if isinstance(sample.get("lon"), (int, float)):
                sample["lon"] = float(sample["lon"]) + d_lon


def rebase_times(row: dict[str, Any], wall0: int, elapsed0: int) -> None:
    if isinstance(row.get("wall_ms"), (int, float)):
        row["wall_ms"] = int(row["wall_ms"]) - wall0
    if isinstance(row.get("elapsed_realtime_ms"), (int, float)):
        row["elapsed_realtime_ms"] = int(row["elapsed_realtime_ms"]) - elapsed0
    # Drop absolute device clocks that are not used by replay.
    for key in ("fix_time_ms", "fix_elapsed_realtime_nanos", "fix_age_ms"):
        row.pop(key, None)


def strip_identifiers(row: dict[str, Any]) -> None:
    for key in list(row.keys()):
        lowered = key.lower()
        if any(
            token in lowered
            for token in (
                "serial",
                "android_id",
                "fingerprint",
                "model",
                "device",
                "imei",
                "mac",
                "ssid",
            )
        ):
            row.pop(key, None)


def keep_fields(row: dict[str, Any]) -> dict[str, Any]:
    kind = row.get("kind")
    allowed = KEEP_BY_KIND.get(kind)
    if allowed is None:
        return row
    return {k: v for k, v in row.items() if k in allowed}


def anonymise(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    origin_lat, origin_lon = first_fix_origin(rows)
    d_lat = PUBLIC_ORIGIN_LAT - origin_lat
    d_lon = PUBLIC_ORIGIN_LON - origin_lon
    wall0, elapsed0 = time_bases(rows)
    out: list[dict[str, Any]] = []
    for row in rows:
        kind = row.get("kind")
        if kind in DROP_KINDS:
            continue
        copy = json.loads(json.dumps(row))
        strip_identifiers(copy)
        shift_lat_lon(copy, d_lat, d_lon)
        rebase_times(copy, wall0, elapsed0)
        out.append(keep_fields(copy))
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", help="raw location-diag JSONL")
    parser.add_argument(
        "-o",
        "--output",
        required=True,
        help="anonymised JSONL path",
    )
    args = parser.parse_args()
    rows = load_rows(args.input)
    anon = anonymise(rows)
    with open(args.output, "w", encoding="utf-8") as handle:
        for row in anon:
            handle.write(json.dumps(row, separators=(",", ":"), ensure_ascii=False))
            handle.write("\n")
    print(
        f"wrote {len(anon)} lines to {args.output} "
        f"(origin {PUBLIC_ORIGIN_LAT},{PUBLIC_ORIGIN_LON})",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
