#!/usr/bin/env python3
"""Generate the offline region-adjacency asset for long-trip corridors.

Primary geometry: Geofabrik download index-v1.json polygons (exact catalog path
match). Supplement: Natural Earth Admin-1 1:10m, Sweden län only.

Fixed road/tunnel links (named, justified — never hand-drawn edges elsewhere):
  - europe/denmark ↔ europe/sweden/skane — Øresund Bridge / Drogden Tunnel

Deferred (add one line when catalog splits Denmark into region leaves):
  - europe/denmark/syddanmark ↔ europe/denmark/sjaelland — Great Belt / Storebælt

Re-run:

  python3 scripts/generate-region-adjacency.py

Requires: pyshp, shapely.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
import urllib.request
import zipfile
from datetime import datetime, timezone
from pathlib import Path

from shapely.geometry import shape
from shapely.strtree import STRtree

MAX_ASSET_BYTES = 8 * 1024 * 1024

GEOFABRIK_INDEX_URL = "https://download.geofabrik.de/index-v1.json"
NE_VERSION = "5.1.1"
NE_ZIP_URL = (
    "https://naciscdn.org/naturalearth/10m/cultural/"
    "ne_10m_admin_1_states_provinces.zip"
)

UA = (
    "NaviRegionAdjacencyBuild/0.1 "
    "(https://github.com/navigate-me/Navi; generate-region-adjacency)"
)

MAGIC = b"NAVIRADJ"
FORMAT_VERSION = 1
COORD_SCALE = 10_000_000  # 1e-7 deg

# Polygon-touch tolerance (metres). Geofabrik clip polys are buffered; a small
# gap tolerance catches near-touch without inventing sea crossings.
TOUCH_TOLERANCE_M = 1_500.0

# Source tags stored per region (u8).
SRC_GEOFABRIK = 0
SRC_NE_SWEDEN = 1
SRC_MANUAL_STUB = 2

# Edge kinds (u8).
EDGE_POLYGON = 0
EDGE_FIXED_LINK = 1
EDGE_MANUAL_LEGACY = 2

# Named links: (a, b, note, kind). Undirected; script sorts endpoints.
NAMED_LINKS: list[tuple[str, str, str, int]] = [
    (
        "europe/denmark",
        "europe/sweden/skane",
        "Oresund Bridge / Drogden Tunnel",
        EDGE_FIXED_LINK,
    ),
    # Deferred until catalog publishes Danish region leaves:
    # ("europe/denmark/syddanmark", "europe/denmark/sjaelland",
    #  "Great Belt / Storebaelt Bridge", EDGE_FIXED_LINK),
]

# Polygon adjacency the generator must NOT keep even when Geofabrik clip polys
# touch (water / not-yet-open fixed links). Named and justified like fixed links.
DENIED_EDGES: list[tuple[str, str, str]] = [
    (
        "europe/germany/mecklenburg-vorpommern",
        "europe/denmark",
        "Fehmarnbelt: tunnel not open; Geofabrik extract polys intersect across water",
    ),
]

# Regions that must have degree 0 in the corridor graph (no ferry-only / overseas
# land bridges). Auto edges are stripped; no named links added.
FORCE_ISOLATE: set[str] = {
    "europe/sweden/gotland",
    "europe/norway/svalbard-janmayen",
    "north-america/us/alaska",
    "north-america/us/hawaii",
    "north-america/us/puerto-rico",
    "north-america/us/us-virgin-islands",
}

# Multi-state Geofabrik extracts published alongside state leaves. Including them
# in hop-count paths collapses a cross-country trip into a handful of huge packs.
# Keep geometry for PIP, but omit from corridor adjacency.
CORRIDOR_GRAPH_EXCLUDE: set[str] = {
    "north-america/us-midwest",
    "north-america/us-northeast",
    "north-america/us-pacific",
    "north-america/us-south",
    "north-america/us-west",
}

# Natural Earth name_en / name → catalog leaf under europe/sweden/.
SWEDEN_NE_TO_CATALOG: dict[str, str] = {
    "Blekinge": "blekinge",
    "Dalarna": "dalarna",
    "Gotland": "gotland",
    "Gavleborg": "gavleborg",
    "Gävleborg": "gavleborg",
    "Halland": "halland",
    "Jamtland": "jamtland",
    "Jämtland": "jamtland",
    "Jonkoping": "jonkoping",
    "Jönköping": "jonkoping",
    "Kalmar": "kalmar",
    "Kronoberg": "kronoberg",
    "Norrbotten": "norrbotten",
    "Orebro": "orebro",
    "Örebro": "orebro",
    "Skane": "skane",
    "Skåne": "skane",
    "Stockholm": "stockholm",
    "Sodermanland": "sodermanland",
    "Södermanland": "sodermanland",
    "Uppsala": "uppsala",
    "Varmland": "varmland",
    "Värmland": "varmland",
    "Vasterbotten": "vasterbotten",
    "Västerbotten": "vasterbotten",
    "Vasternorrland": "vasternorrland",
    "Västernorrland": "vasternorrland",
    "Vastmanland": "vastmanland",
    "Västmanland": "vastmanland",
    "Vastra Gotaland": "vastra_gotaland",
    "Västra Götaland": "vastra_gotaland",
    "Ostergotland": "ostergotland",
    "Östergötland": "ostergotland",
}


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def download(url: str, dest: Path) -> None:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=180) as resp, open(dest, "wb") as out:
        out.write(resp.read())


def load_catalog_ids(path: Path) -> list[str]:
    raw = json.loads(path.read_text())
    ids = [r["region_id"] for r in raw["regions"]]
    # Hedmark is retired — never admit it even if an old current.json still lists it.
    ids = [i for i in ids if i.strip().strip("/").lower() != "europe/norway/hedmark"]
    if not ids:
        raise SystemExit(f"empty catalog {path}")
    return ids


def geofabrik_full_path(by_id: dict, fid: str) -> str:
    parts: list[str] = []
    cur: str | None = fid
    seen: set[str] = set()
    while cur and cur not in seen:
        seen.add(cur)
        feat = by_id.get(cur)
        if feat is None:
            break
        parts.append(feat["properties"]["id"])
        cur = feat["properties"].get("parent")
    parts.reverse()
    return "/".join(parts)


def load_geofabrik(index_path: Path, catalog: set[str]) -> dict[str, object]:
    idx = json.loads(index_path.read_text())
    by_id = {f["properties"]["id"]: f for f in idx["features"]}
    out: dict[str, object] = {}
    for feat in idx["features"]:
        path = geofabrik_full_path(by_id, feat["properties"]["id"])
        if path not in catalog:
            continue
        geom = feat.get("geometry")
        if not geom:
            continue
        g = shape(geom)
        if g.is_empty:
            continue
        out[path] = g
    return out


def load_sweden_ne(shp_dir: Path, catalog: set[str]) -> dict[str, object]:
    try:
        import shapefile
    except ImportError as e:
        raise SystemExit("pyshp required: pip install pyshp") from e

    shp = next(shp_dir.glob("ne_10m_admin_1_states_provinces.shp"))
    sf = shapefile.Reader(str(shp))
    fields = [f[0] for f in sf.fields[1:]]
    out: dict[str, object] = {}
    for sr in sf.shapeRecords():
        rec = dict(zip(fields, sr.record))
        iso = str(rec.get("iso_a2") or "").strip().upper()
        if iso != "SE":
            continue
        name = str(rec.get("name_en") or rec.get("name") or "").strip()
        leaf = SWEDEN_NE_TO_CATALOG.get(name)
        if leaf is None:
            # Try ascii-folded variants already in the map; skip unknowns loudly.
            raise SystemExit(f"unmapped Sweden NE admin-1 name: {name!r}")
        rid = f"europe/sweden/{leaf}"
        if rid not in catalog:
            raise SystemExit(f"NE Sweden region not in catalog: {rid}")
        # pyshp → geojson-like
        __geo_interface__ = sr.shape.__geo_interface__
        g = shape(__geo_interface__)
        if g.is_empty:
            continue
        if rid in out:
            out[rid] = out[rid].union(g)
        else:
            out[rid] = g
    return out


def geom_rings_lonlat(g) -> list[list[tuple[f64, f64]]]:
    """Exterior rings only as closed (lon, lat) lists."""
    rings: list[list[tuple[float, float]]] = []
    if g.geom_type == "Polygon":
        polys = [g]
    elif g.geom_type == "MultiPolygon":
        polys = list(g.geoms)
    else:
        # Buffer/ degenerate — skip
        return rings
    for poly in polys:
        coords = list(poly.exterior.coords)
        if len(coords) < 3:
            continue
        if coords[0] != coords[-1]:
            coords.append(coords[0])
        rings.append([(float(x), float(y)) for x, y in coords])
    return rings


# type alias for clarity in signatures above
f64 = float


def centroid_lonlat(g) -> tuple[float, float]:
    c = g.centroid
    return (float(c.x), float(c.y))


def area_deg2(g) -> float:
    return float(g.area)


def build_polygon_edges(
    ids: list[str], geoms: list, skip_auto: set[str]
) -> list[tuple[int, int]]:
    """Undirected edges from polygon touch / near-touch. skip_auto: no auto edges."""
    # Slight buffer so near-touch counts; metres via geographic approx.
    # 1 deg lat ≈ 111 km → 1500 m ≈ 0.0135 deg.
    tol_deg = TOUCH_TOLERANCE_M / 111_000.0
    prepared: list = []
    for i, g in enumerate(geoms):
        if ids[i] in skip_auto:
            prepared.append(None)
        else:
            prepared.append(g.buffer(tol_deg) if tol_deg > 0 else g)

    tree_geoms = [g for g in prepared if g is not None]
    tree_idx = [i for i, g in enumerate(prepared) if g is not None]
    tree = STRtree(tree_geoms)

    edges: set[tuple[int, int]] = set()
    for local_i, gi in enumerate(tree_geoms):
        i = tree_idx[local_i]
        hits = tree.query(gi, predicate="intersects")
        for local_j in hits:
            j = tree_idx[int(local_j)]
            if j <= i:
                continue
            edges.add((i, j))
    return sorted(edges)


def encode_asset(
    regions: list[dict],
    edges: list[tuple[int, int, int]],
    named: list[tuple[int, int, str, int]],
) -> bytes:
    parts = [MAGIC, struct.pack("<II", FORMAT_VERSION, len(regions))]
    for r in regions:
        rid = r["id"].encode("utf-8")
        if len(rid) > 65535:
            raise SystemExit(f"id too long: {r['id']}")
        parts.append(struct.pack("<HB", len(rid), r["source"]))
        parts.append(rid)
        lon_c, lat_c = r["centroid"]
        parts.append(
            struct.pack(
                "<ii",
                int(round(lon_c * COORD_SCALE)),
                int(round(lat_c * COORD_SCALE)),
            )
        )
        rings = r["rings"]
        parts.append(struct.pack("<H", len(rings)))
        for ring in rings:
            parts.append(struct.pack("<I", len(ring)))
            for lon, lat in ring:
                parts.append(
                    struct.pack(
                        "<ii",
                        int(round(lon * COORD_SCALE)),
                        int(round(lat * COORD_SCALE)),
                    )
                )
    parts.append(struct.pack("<I", len(edges)))
    for u, v, kind in edges:
        parts.append(struct.pack("<HHB", u, v, kind))
    parts.append(struct.pack("<I", len(named)))
    for u, v, note, kind in named:
        nb = note.encode("utf-8")
        parts.append(struct.pack("<HHBH", u, v, kind, len(nb)))
        parts.append(nb)
    return b"".join(parts)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--catalog",
        type=Path,
        default=repo_root() / "core/tests/fixtures/long_trip/current.json",
    )
    ap.add_argument(
        "--geofabrik-index",
        type=Path,
        default=None,
        help="Reuse a downloaded index-v1.json (skip download)",
    )
    ap.add_argument(
        "--ne-dir",
        type=Path,
        default=None,
        help="Directory containing extracted NE admin-1 shapefile",
    )
    ap.add_argument(
        "--out-dir",
        type=Path,
        default=repo_root() / "core/src/long_trip/data",
    )
    ap.add_argument("--max-bytes", type=int, default=MAX_ASSET_BYTES)
    ap.add_argument(
        "--cache-dir",
        type=Path,
        default=Path("/tmp/navi_region_adjacency"),
    )
    args = ap.parse_args()

    catalog_ids = load_catalog_ids(args.catalog)
    catalog_set = set(catalog_ids)
    args.cache_dir.mkdir(parents=True, exist_ok=True)
    args.out_dir.mkdir(parents=True, exist_ok=True)

    gf_path = args.geofabrik_index
    if gf_path is None:
        gf_path = args.cache_dir / "index-v1.json"
        if not gf_path.is_file():
            print(f"downloading {GEOFABRIK_INDEX_URL} …", flush=True)
            download(GEOFABRIK_INDEX_URL, gf_path)

    ne_dir = args.ne_dir
    if ne_dir is None:
        ne_dir = args.cache_dir / "ne_10m_admin1"
        shp = ne_dir / "ne_10m_admin_1_states_provinces.shp"
        if not shp.is_file():
            zpath = args.cache_dir / "ne_10m_admin1.zip"
            if not zpath.is_file():
                print(f"downloading {NE_ZIP_URL} …", flush=True)
                download(NE_ZIP_URL, zpath)
            ne_dir.mkdir(parents=True, exist_ok=True)
            with zipfile.ZipFile(zpath) as zf:
                zf.extractall(ne_dir)

    print("loading Geofabrik …", flush=True)
    gf = load_geofabrik(gf_path, catalog_set)
    print(f"  matched {len(gf)} / {len(catalog_ids)}", flush=True)

    print("loading NE Sweden …", flush=True)
    se = load_sweden_ne(ne_dir, catalog_set)
    print(f"  sweden län {len(se)}", flush=True)

    # Assemble regions in catalog order (stable indices).
    geoms_by_id: dict[str, tuple[object, int]] = {}
    for rid, g in gf.items():
        geoms_by_id[rid] = (g, SRC_GEOFABRIK)
    for rid, g in se.items():
        if rid in geoms_by_id:
            raise SystemExit(f"duplicate geometry for {rid}")
        geoms_by_id[rid] = (g, SRC_NE_SWEDEN)

    missing = [rid for rid in catalog_ids if rid not in geoms_by_id]
    if missing:
        raise SystemExit(f"no geometry for catalog regions: {missing}")

    regions: list[dict] = []
    ids: list[str] = []
    geoms: list = []
    for rid in catalog_ids:
        g, src = geoms_by_id[rid]
        rings = geom_rings_lonlat(g)
        if not rings:
            raise SystemExit(f"empty rings for {rid}")
        regions.append(
            {
                "id": rid,
                "source": src,
                "rings": rings,
                "centroid": centroid_lonlat(g),
                "area": area_deg2(g),
            }
        )
        ids.append(rid)
        geoms.append(g)

    id_to_i = {rid: i for i, rid in enumerate(ids)}

    # Auto polygon edges — exclude forced isolates and multi-state extracts that
    # must not participate in hop-count corridors.
    skip_auto = FORCE_ISOLATE | CORRIDOR_GRAPH_EXCLUDE
    print("computing polygon adjacency …", flush=True)
    poly_pairs = build_polygon_edges(ids, geoms, skip_auto)
    edges: list[tuple[int, int, int]] = [
        (u, v, EDGE_POLYGON) for u, v in poly_pairs
    ]

    named_encoded: list[tuple[int, int, str, int]] = []
    for a, b, note, kind in NAMED_LINKS:
        if a not in id_to_i or b not in id_to_i:
            raise SystemExit(f"named link endpoints missing from catalog: {a} / {b}")
        u, v = sorted((id_to_i[a], id_to_i[b]))
        # Avoid duplicate undirected edge
        if not any(e[0] == u and e[1] == v for e in edges):
            edges.append((u, v, kind))
        else:
            # Upgrade polygon edge to named kind if present
            edges = [
                (u, v, kind) if e[0] == u and e[1] == v else e for e in edges
            ]
        named_encoded.append((u, v, note, kind))

    # Safety: drop any residual edges touching force-isolates / graph-excludes.
    ban = {id_to_i[r] for r in (FORCE_ISOLATE | CORRIDOR_GRAPH_EXCLUDE) if r in id_to_i}
    edges = [(u, v, k) for u, v, k in edges if u not in ban and v not in ban]

    denied_notes = []
    for a, b, note in DENIED_EDGES:
        if a not in id_to_i or b not in id_to_i:
            raise SystemExit(f"denied edge endpoints missing: {a} / {b}")
        u, v = sorted((id_to_i[a], id_to_i[b]))
        before = len(edges)
        edges = [(eu, ev, k) for eu, ev, k in edges if not (eu == u and ev == v)]
        if len(edges) < before:
            denied_notes.append({"a": a, "b": b, "note": note})

    edges.sort(key=lambda t: (t[0], t[1], t[2]))

    blob = encode_asset(regions, edges, named_encoded)
    if len(blob) > args.max_bytes:
        raise SystemExit(
            f"asset {len(blob)} bytes exceeds budget {args.max_bytes}"
        )

    bin_path = args.out_dir / "region_adjacency.bin"
    bin_path.write_bytes(blob)

    # Degree / isolates
    deg = [0] * len(ids)
    for u, v, _ in edges:
        deg[u] += 1
        deg[v] += 1
    isolates = [ids[i] for i, d in enumerate(deg) if d == 0]

    sha = hashlib.sha256(blob).hexdigest()
    generated_utc = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    manifest = {
        "format": "NAVIRADJ",
        "format_version": FORMAT_VERSION,
        "coord_scale": COORD_SCALE,
        "generated_utc": generated_utc,
        "sha256": sha,
        "bytes": len(blob),
        "n_regions": len(regions),
        "n_edges": len(edges),
        "n_named_links": len(named_encoded),
        "sources": {
            "geofabrik": {
                "url": GEOFABRIK_INDEX_URL,
                "n_regions": sum(1 for r in regions if r["source"] == SRC_GEOFABRIK),
            },
            "natural_earth_admin1_sweden": {
                "url": NE_ZIP_URL,
                "version_hint": NE_VERSION,
                "n_regions": sum(1 for r in regions if r["source"] == SRC_NE_SWEDEN),
            },
        },
        "named_links": [
            {
                "a": ids[u],
                "b": ids[v],
                "note": note,
                "kind": kind,
            }
            for u, v, note, kind in named_encoded
        ],
        "denied_edges": denied_notes,
        "isolates": isolates,
        "force_isolate": sorted(FORCE_ISOLATE),
        "corridor_graph_exclude": sorted(CORRIDOR_GRAPH_EXCLUDE),
        "touch_tolerance_m": TOUCH_TOLERANCE_M,
        "catalog": str(args.catalog),
    }
    (args.out_dir / "region_adjacency.manifest.json").write_text(
        json.dumps(manifest, indent=2, ensure_ascii=True) + "\n"
    )
    attribution = f"""Region adjacency boundaries
===========================

Geofabrik download index polygons (index-v1.json):
  https://download.geofabrik.de/
  OpenStreetMap data, ODbL.

Natural Earth Admin-1 (1:10m), Sweden only:
  Public Domain (CC0 1.0) — Natural Earth.
  Free vector and raster map data @ naturalearthdata.com.

Generated: {generated_utc}
Asset sha256: {sha}
"""
    (args.out_dir / "ATTRIBUTION.txt").write_text(attribution)

    print(
        f"wrote {bin_path} ({len(blob)} bytes); "
        f"regions={len(regions)} edges={len(edges)} isolates={len(isolates)}"
    )
    if isolates:
        print("isolates:")
        for r in isolates:
            print(f"  - {r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
