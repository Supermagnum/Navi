#!/usr/bin/env python3
"""Clip sweden-latest.osm.pbf to one län using region_adjacency.bin outlines.

Uses the same PIP polygons as long-trip hop primary-pack coverage
(`core/src/long_trip/data/region_adjacency.bin` — Geofabrik index polys +
Natural Earth Admin-1 for Sweden län).

Host-only tooling for Follow-up 16 Option A. Writes large outputs under
TMPDIR / an explicit --out-dir on the data mount — never /tmp.

Example:

  export TMPDIR=/mnt/.../navi-fu16-tmp
  python3 scripts/clip-sweden-lan-from-adjacency.py \\
    --region europe/sweden/halland \\
    --sweden $TMPDIR/sweden-clip/sweden-latest.osm.pbf \\
    --out-dir $TMPDIR/sweden-clip
"""

from __future__ import annotations

import argparse
import struct
import sys
import time
from pathlib import Path

import osmium
from shapely.geometry import Point, Polygon
from shapely.prepared import prep

MAGIC = b"NAVIRADJ"
COORD_SCALE = 10_000_000.0


def load_region_rings(bin_path: Path, region_id: str) -> list[list[tuple[float, float]]]:
    data = bin_path.read_bytes()
    if data[:8] != MAGIC:
        raise SystemExit(f"bad magic in {bin_path}")
    off = 8
    fmt_ver = struct.unpack_from("<I", data, off)[0]
    off += 4
    if fmt_ver != 1:
        raise SystemExit(f"unsupported format_version={fmt_ver}")
    n = struct.unpack_from("<I", data, off)[0]
    off += 4
    found: list[list[tuple[float, float]]] | None = None
    for _ in range(n):
        id_len = struct.unpack_from("<H", data, off)[0]
        # u16 id_len + u8 source (see adjacency.rs decode_asset)
        off += 2
        _source = data[off]
        off += 1
        rid = data[off : off + id_len].decode("utf-8")
        off += id_len
        off += 8  # centroid lon/lat i32
        n_rings = struct.unpack_from("<H", data, off)[0]
        off += 2
        rings: list[list[tuple[float, float]]] = []
        for _r in range(n_rings):
            n_pts = struct.unpack_from("<I", data, off)[0]
            off += 4
            pts: list[tuple[float, float]] = []
            for _p in range(n_pts):
                lon_i, lat_i = struct.unpack_from("<ii", data, off)
                off += 8
                pts.append((lon_i / COORD_SCALE, lat_i / COORD_SCALE))
            rings.append(pts)
        if rid == region_id:
            found = rings
    if found is None:
        raise SystemExit(f"region not found in adjacency bin: {region_id}")
    return found


def rings_to_polygons(rings: list[list[tuple[float, float]]]) -> list[Polygon]:
    polys = []
    for ring in rings:
        if len(ring) < 3:
            continue
        # adjacency stores lon,lat; shapely uses (x=lon, y=lat)
        poly = Polygon(ring)
        if not poly.is_valid:
            poly = poly.buffer(0)
        if poly.is_empty:
            continue
        polys.append(poly)
    if not polys:
        raise SystemExit("no usable rings")
    return polys


def write_poly_file(path: Path, region_id: str, rings: list[list[tuple[float, float]]]) -> None:
    """Osmium/Osmosis .poly (lon lat pairs)."""
    leaf = region_id.rstrip("/").split("/")[-1]
    lines = [leaf]
    for i, ring in enumerate(rings, start=1):
        lines.append(str(i))
        for lon, lat in ring:
            lines.append(f"  {lon:.7f}  {lat:.7f}")
        lines.append("END")
    lines.append("END")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def outline_accuracy_notes(
    region_id: str,
    polys: list[Polygon],
    bbox_table: tuple[float, float, float, float] | None,
) -> str:
    """bbox_table is (min_lat, min_lon, max_lat, max_lon) from basemap regions.rs if known."""
    minx = min(p.bounds[0] for p in polys)
    miny = min(p.bounds[1] for p in polys)
    maxx = max(p.bounds[2] for p in polys)
    maxy = max(p.bounds[3] for p in polys)
    area = sum(p.area for p in polys)
    lines = [
        f"region_id={region_id}",
        f"rings={len(polys)}",
        f"outline_bbox lon=[{minx:.5f},{maxx:.5f}] lat=[{miny:.5f},{maxy:.5f}]",
        f"outline_area_deg2={area:.5f}",
        "source=region_adjacency.bin (NE Admin-1 for Sweden län; same PIP as hop primary pack)",
    ]
    if bbox_table is not None:
        t_min_lat, t_min_lon, t_max_lat, t_max_lon = bbox_table
        d_min_lon = abs(minx - t_min_lon)
        d_max_lon = abs(maxx - t_max_lon)
        d_min_lat = abs(miny - t_min_lat)
        d_max_lat = abs(maxy - t_max_lat)
        lines.append(
            f"basemap_region_bbox lat=[{t_min_lat},{t_max_lat}] lon=[{t_min_lon},{t_max_lon}]"
        )
        lines.append(
            f"bbox_delta_deg min_lon={d_min_lon:.4f} max_lon={d_max_lon:.4f} "
            f"min_lat={d_min_lat:.4f} max_lat={d_max_lat:.4f}"
        )
        # Halland coastal outline vs AABB: NE polygons follow coast; AABB is coarser.
        if max(d_min_lon, d_max_lon, d_min_lat, d_max_lat) > 0.15:
            lines.append(
                "accuracy=outline is finer than basemap AABB (expected for coastal län); "
                "use outline for clip, not AABB"
            )
        else:
            lines.append("accuracy=outline bbox close to basemap AABB (<0.15 deg)")
    return "\n".join(lines) + "\n"


# From core/src/routing/basemap/regions.rs — Halland only for the trial report.
HALLAND_BBOX = (56.32, 11.85, 57.55, 13.55)  # min_lat, min_lon, max_lat, max_lon


class CollectNodes(osmium.SimpleHandler):
    """Pass 1: collect node ids inside outline (bbox reject then prepared PIP)."""

    def __init__(self, prepared, bbox: tuple[float, float, float, float]):
        super().__init__()
        self.prepared = prepared
        self.min_lon, self.min_lat, self.max_lon, self.max_lat = bbox
        self.node_ids: set[int] = set()

    def node(self, n: osmium.osm.Node) -> None:
        if not n.location.valid():
            return
        lon, lat = n.location.lon, n.location.lat
        if lon < self.min_lon or lon > self.max_lon or lat < self.min_lat or lat > self.max_lat:
            return
        if any(p.contains(Point(lon, lat)) for p in self.prepared):
            self.node_ids.add(n.id)


class WriteKept(osmium.SimpleHandler):
    """Pass 2: write kept nodes + ways/rels that reference them."""

    def __init__(self, writer: osmium.SimpleWriter, node_ids: set[int]):
        super().__init__()
        self.writer = writer
        self.node_ids = node_ids
        self.way_ids: set[int] = set()
        self.kept_nodes = 0
        self.kept_ways = 0
        self.kept_rels = 0

    def node(self, n: osmium.osm.Node) -> None:
        if n.id in self.node_ids:
            self.writer.add_node(n)
            self.kept_nodes += 1

    def way(self, w: osmium.osm.Way) -> None:
        refs = [x.ref for x in w.nodes]
        if refs and any(r in self.node_ids for r in refs):
            self.writer.add_way(w)
            self.way_ids.add(w.id)
            self.kept_ways += 1

    def relation(self, r: osmium.osm.Relation) -> None:
        for m in r.members:
            if (m.type == "n" and m.ref in self.node_ids) or (
                m.type == "w" and m.ref in self.way_ids
            ):
                self.writer.add_relation(r)
                self.kept_rels += 1
                return


def clip(sweden: Path, out_pbf: Path, polys: list[Polygon]) -> dict:
    prepared = [prep(p) for p in polys]
    minx = min(p.bounds[0] for p in polys) - 0.02
    miny = min(p.bounds[1] for p in polys) - 0.02
    maxx = max(p.bounds[2] for p in polys) + 0.02
    maxy = max(p.bounds[3] for p in polys) + 0.02
    if out_pbf.exists():
        out_pbf.unlink()
    t0 = time.perf_counter()
    collect = CollectNodes(prepared, (minx, miny, maxx, maxy))
    collect.apply_file(str(sweden), locations=True)
    writer = osmium.SimpleWriter(str(out_pbf))
    write = WriteKept(writer, collect.node_ids)
    write.apply_file(str(sweden), locations=True)
    writer.close()
    elapsed = time.perf_counter() - t0
    return {
        "kept_nodes": write.kept_nodes,
        "kept_ways": write.kept_ways,
        "kept_rels": write.kept_rels,
        "out_bytes": out_pbf.stat().st_size,
        "clip_sec": elapsed,
        "method": "bbox_pad+outline_pip_two_pass",
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--region", required=True, help="e.g. europe/sweden/halland")
    ap.add_argument("--sweden", required=True, type=Path)
    ap.add_argument("--out-dir", required=True, type=Path)
    ap.add_argument(
        "--adjacency-bin",
        type=Path,
        default=Path("core/src/long_trip/data/region_adjacency.bin"),
    )
    ap.add_argument("--clip", action="store_true", help="also clip the PBF (slow)")
    args = ap.parse_args()
    region = args.region.strip().strip("/")
    if not args.sweden.is_file():
        print(f"missing sweden PBF: {args.sweden}", file=sys.stderr)
        return 2
    if str(args.out_dir).startswith("/tmp"):
        print("refuse out-dir under /tmp — use data mount TMPDIR", file=sys.stderr)
        return 2
    args.out_dir.mkdir(parents=True, exist_ok=True)
    rings = load_region_rings(args.adjacency_bin, region)
    polys = rings_to_polygons(rings)
    leaf = region.split("/")[-1]
    poly_path = args.out_dir / f"{leaf}.poly"
    write_poly_file(poly_path, region, rings)
    bbox = HALLAND_BBOX if region == "europe/sweden/halland" else None
    notes = outline_accuracy_notes(region, polys, bbox)
    notes_path = args.out_dir / f"{leaf}-outline-accuracy.txt"
    notes_path.write_text(notes, encoding="utf-8")
    print(notes)
    print(f"wrote {poly_path}")
    if not args.clip:
        print("skip clip (pass --clip to cut PBF)")
        return 0
    out_pbf = args.out_dir / f"{leaf}-latest.osm.pbf"
    stats = clip(args.sweden, out_pbf, polys)
    stats_path = args.out_dir / f"{leaf}-clip-stats.txt"
    body = (
        f"region_id={region}\n"
        f"out_pbf={out_pbf}\n"
        f"out_bytes={stats['out_bytes']}\n"
        f"kept_nodes={stats['kept_nodes']}\n"
        f"kept_ways={stats['kept_ways']}\n"
        f"kept_rels={stats['kept_rels']}\n"
        f"clip_sec={stats['clip_sec']:.1f}\n"
        f"method={stats.get('method', 'unknown')}\n"
    )
    stats_path.write_text(body, encoding="utf-8")
    print(body)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
