#!/usr/bin/env python3
"""Generate the offline country-polygon asset for `country_iso_at`.

Downloads Natural Earth Admin-0 Countries (script-selected scale), converts
rings to a compact little-endian binary, and writes a provenance manifest.

Never hand-edits vertices. Re-run to refresh:

  python3 scripts/generate-country-polys.py

Requires: pyshp (`pip install pyshp`).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
import tempfile
import urllib.request
import zipfile
from datetime import datetime, timezone
from pathlib import Path

# Tunable: compiled asset must stay under this size (bytes).
MAX_ASSET_BYTES = 5 * 1024 * 1024

# Natural Earth is public domain (CC0). Prefer the smallest scale that still
# classifies the evidence fixture correctly (measured: 1:50m with coastal snap).
NE_VERSION = "5.1.1"
NE_SCALE = "50m"
NE_ZIP_URL = (
    f"https://naciscdn.org/naturalearth/{NE_SCALE}/cultural/"
    f"ne_{NE_SCALE}_admin_0_countries.zip"
)
NE_LICENSE = "Public Domain (CC0 1.0) — Natural Earth"
NE_ATTRIBUTION = (
    "Country boundaries: Made with Natural Earth. "
    "Free vector and raster map data @ naturalearthdata.com."
)

UA = (
    "NaviCountryIsoBuild/0.1 "
    "(https://github.com/navigate-me/Navi; generate-country-polys)"
)

MAGIC = b"NAVICP50"
FORMAT_VERSION = 1
COORD_SCALE = 10_000_000  # i32 microdegrees * 10 → 1e-7 deg


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def download(url: str, dest: Path) -> None:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=120) as resp, open(dest, "wb") as out:
        out.write(resp.read())


def load_shapefile(shp_path: Path):
    try:
        import shapefile  # pyshp
    except ImportError as e:
        raise SystemExit("pyshp required: pip install pyshp") from e

    sf = shapefile.Reader(str(shp_path))
    fields = [f[0] for f in sf.fields[1:]]
    if "ISO_A2_EH" in fields:
        iso_i = fields.index("ISO_A2_EH")
    elif "ISO_A2" in fields:
        iso_i = fields.index("ISO_A2")
    else:
        raise SystemExit(f"no ISO_A2 field in {fields}")

    countries = []
    for sr in sf.shapeRecords():
        iso = str(sr.record[iso_i]).strip().lower()
        if len(iso) != 2 or iso in ("-9", "-99", "none", "nan"):
            continue
        shape = sr.shape
        parts = list(shape.parts) + [len(shape.points)]
        rings = []
        for i in range(len(parts) - 1):
            pts = [(float(p[0]), float(p[1])) for p in shape.points[parts[i] : parts[i + 1]]]
            if len(pts) >= 3:
                if pts[0] != pts[-1]:
                    pts.append(pts[0])
                rings.append(pts)
        if not rings:
            continue
        area = 0.0
        for ring in rings:
            a = 0.0
            for i in range(len(ring) - 1):
                a += ring[i][0] * ring[i + 1][1] - ring[i + 1][0] * ring[i][1]
            area += abs(a) * 0.5
        countries.append((iso, rings, area))

    # Merge multi-feature countries (e.g. fragmented records) by ISO.
    by_iso: dict[str, tuple[list, float]] = {}
    for iso, rings, area in countries:
        if iso not in by_iso:
            by_iso[iso] = (list(rings), area)
        else:
            by_iso[iso][0].extend(rings)
            by_iso[iso] = (by_iso[iso][0], by_iso[iso][1] + area)

    # Smallest area first → deterministic overlap preference (LI before CH, etc.).
    merged = [(iso, rings, area) for iso, (rings, area) in by_iso.items()]
    merged.sort(key=lambda t: (t[2], t[0]))
    return merged


def encode_asset(countries) -> bytes:
    parts = [MAGIC, struct.pack("<II", FORMAT_VERSION, len(countries))]
    for iso, rings, _area in countries:
        iso_b = iso.encode("ascii")
        assert len(iso_b) == 2
        parts.append(iso_b)
        parts.append(struct.pack("<H", 0))  # pad
        parts.append(struct.pack("<I", len(rings)))
        for ring in rings:
            parts.append(struct.pack("<I", len(ring)))
            for lon, lat in ring:
                lon_i = int(round(lon * COORD_SCALE))
                lat_i = int(round(lat * COORD_SCALE))
                parts.append(struct.pack("<ii", lon_i, lat_i))
    return b"".join(parts)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--max-bytes",
        type=int,
        default=MAX_ASSET_BYTES,
        help="fail if compiled asset exceeds this size",
    )
    ap.add_argument(
        "--scale",
        default=NE_SCALE,
        choices=("110m", "50m", "10m"),
        help="Natural Earth scale",
    )
    args = ap.parse_args()

    root = repo_root()
    out_dir = root / "core" / "src" / "routing" / "elevation" / "data"
    out_dir.mkdir(parents=True, exist_ok=True)
    bin_path = out_dir / "country_polys.bin"
    manifest_path = out_dir / "country_polys.manifest.json"
    attr_path = out_dir / "ATTRIBUTION.txt"

    scale = args.scale
    url = (
        f"https://naciscdn.org/naturalearth/{scale}/cultural/"
        f"ne_{scale}_admin_0_countries.zip"
    )

    with tempfile.TemporaryDirectory(prefix="navi_ne_") as tmp:
        tmp_p = Path(tmp)
        zip_path = tmp_p / "ne.zip"
        print(f"downloading {url}", file=sys.stderr)
        download(url, zip_path)
        zip_sha = hashlib.sha256(zip_path.read_bytes()).hexdigest()
        with zipfile.ZipFile(zip_path) as zf:
            zf.extractall(tmp_p / "ne")
        shp = next((tmp_p / "ne").rglob(f"ne_{scale}_admin_0_countries.shp"))
        version_txt = next(
            (tmp_p / "ne").rglob(f"ne_{scale}_admin_0_countries.VERSION.txt"),
            None,
        )
        ne_file_version = (
            version_txt.read_text(encoding="utf-8", errors="replace").strip()
            if version_txt
            else NE_VERSION
        )
        countries = load_shapefile(shp)

    required = {
        "li",
        "lu",
        "mt",
        "cy",
        "be",
        "nl",
        "dk",
        "ch",
        "at",
        "si",
        "hr",
        "sk",
        "cz",
        "hu",
        "ee",
        "lv",
        "lt",
        "ie",
        "pt",
        "bg",
        "ro",
        "gr",
        "pl",
        "it",
        "es",
        "de",
        "fr",
        "se",
        "fi",
        "is",
        "no",
        "gb",
        "us",
        "ca",
        "mx",
    }
    have = {iso for iso, _, _ in countries}
    missing = sorted(required - have)
    if missing:
        raise SystemExit(f"missing required ISO codes: {missing}")

    blob = encode_asset(countries)
    if len(blob) > args.max_bytes:
        raise SystemExit(
            f"asset {len(blob)} bytes exceeds budget {args.max_bytes} "
            f"(raise --max-bytes or pick a coarser --scale)"
        )

    bin_path.write_bytes(blob)
    generated_utc = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    manifest = {
        "format": "NAVICP50",
        "format_version": FORMAT_VERSION,
        "coord_scale": COORD_SCALE,
        "source": "Natural Earth Admin-0 Countries",
        "source_url": url,
        "natural_earth_version": ne_file_version,
        "natural_earth_scale": scale,
        "download_sha256": zip_sha,
        "asset_sha256": hashlib.sha256(blob).hexdigest(),
        "asset_bytes": len(blob),
        "max_asset_bytes": args.max_bytes,
        "country_count": len(countries),
        "generated_utc": generated_utc,
        "license": NE_LICENSE,
        "attribution": NE_ATTRIBUTION,
        "user_agent": UA,
    }
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    attr_path.write_text(
        NE_ATTRIBUTION
        + "\n\n"
        + f"License: {NE_LICENSE}\n"
        + f"Source: {url}\n"
        + f"Natural Earth version: {ne_file_version} ({scale})\n"
        + f"Generated: {generated_utc}\n"
        + f"ZIP SHA-256: {zip_sha}\n"
        + f"Asset SHA-256: {manifest['asset_sha256']}\n",
        encoding="utf-8",
    )
    print(
        f"wrote {bin_path} ({len(blob)} bytes, {len(countries)} countries)",
        file=sys.stderr,
    )
    print(f"wrote {manifest_path}", file=sys.stderr)
    print(f"wrote {attr_path}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
