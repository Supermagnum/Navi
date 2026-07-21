#!/usr/bin/env python3
"""Cut a corridor-scoped OSM extract for Espa -> Atnbrufossen on-device tests.

Uses pyosmium. Output is small enough to serve over HTTP to the emulator.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

import osmium


# Espa (60.562, 11.256) -> Atnbrufossen (61.851, 10.234) with margin.
DEFAULT_BBOX = (10.00, 60.40, 11.50, 62.00)  # min_lon, min_lat, max_lon, max_lat


class BboxFilter(osmium.SimpleHandler):
    def __init__(self, writer: osmium.SimpleWriter, bbox: tuple[float, float, float, float]):
        super().__init__()
        self.writer = writer
        self.min_lon, self.min_lat, self.max_lon, self.max_lat = bbox
        self.kept_nodes = 0
        self.kept_ways = 0
        self.kept_rels = 0
        self._node_ids: set[int] = set()

    def _in_bbox(self, lon: float, lat: float) -> bool:
        return (
            self.min_lon <= lon <= self.max_lon and self.min_lat <= lat <= self.max_lat
        )

    def node(self, n: osmium.osm.Node) -> None:
        if self._in_bbox(n.location.lon, n.location.lat):
            self.writer.add_node(n)
            self._node_ids.add(n.id)
            self.kept_nodes += 1

    def way(self, w: osmium.osm.Way) -> None:
        # Keep ways that reference at least one in-bbox node (refs may be incomplete
        # without locations_on_ways; use tag+ref heuristic via node id set after pass).
        # First pass stores nodes; we need two-pass or locations.
        pass


class WayPass(osmium.SimpleHandler):
    def __init__(
        self,
        writer: osmium.SimpleWriter,
        node_ids: set[int],
        bbox: tuple[float, float, float, float],
    ):
        super().__init__()
        self.writer = writer
        self.node_ids = node_ids
        self.min_lon, self.min_lat, self.max_lon, self.max_lat = bbox
        self.kept_ways = 0
        self.way_node_ids: set[int] = set()

    def way(self, w: osmium.osm.Way) -> None:
        refs = [n.ref for n in w.nodes]
        if not refs:
            return
        # Keep if any referenced node was in the bbox extract.
        if any(r in self.node_ids for r in refs):
            self.writer.add_way(w)
            self.kept_ways += 1
            self.way_node_ids.update(refs)


class RelPass(osmium.SimpleHandler):
    def __init__(self, writer: osmium.SimpleWriter, way_ids: set[int], node_ids: set[int]):
        super().__init__()
        self.writer = writer
        self.way_ids = way_ids
        self.node_ids = node_ids
        self.kept_rels = 0

    def relation(self, r: osmium.osm.Relation) -> None:
        for m in r.members:
            if m.type == "n" and m.ref in self.node_ids:
                self.writer.add_relation(r)
                self.kept_rels += 1
                return
            if m.type == "w" and m.ref in self.way_ids:
                self.writer.add_relation(r)
                self.kept_rels += 1
                return


def cut(src: Path, dst: Path, bbox: tuple[float, float, float, float]) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    if dst.exists():
        dst.unlink()

    # Pass 1: nodes in bbox
    tmp_nodes = dst.with_suffix(".nodes.opl")
    # Use pbf writer directly with two-file approach via osmium FileProcessor is complex;
    # simpler: use osmium.geom + apply with locations_on_ways.

    class Collect(osmium.SimpleHandler):
        def __init__(self):
            super().__init__()
            self.node_ids: set[int] = set()
            self.nodes = 0

        def node(self, n):
            loc = n.location
            if not loc.valid():
                return
            if bbox[0] <= loc.lon <= bbox[2] and bbox[1] <= loc.lat <= bbox[3]:
                self.node_ids.add(n.id)
                self.nodes += 1

    print(f"Scanning nodes in bbox {bbox} from {src} ...", flush=True)
    collect = Collect()
    collect.apply_file(str(src), locations=True)
    print(f"  in-bbox nodes: {collect.nodes}", flush=True)

    class WayCollect(osmium.SimpleHandler):
        def __init__(self, node_ids: set[int]):
            super().__init__()
            self.node_ids = node_ids
            self.way_ids: set[int] = set()
            self.extra_nodes: set[int] = set()

        def way(self, w):
            refs = [n.ref for n in w.nodes]
            if any(r in self.node_ids for r in refs):
                self.way_ids.add(w.id)
                self.extra_nodes.update(refs)

    print("Scanning ways intersecting bbox nodes ...", flush=True)
    ways = WayCollect(collect.node_ids)
    ways.apply_file(str(src), locations=False)
    print(f"  ways: {len(ways.way_ids)}; referenced nodes: {len(ways.extra_nodes)}", flush=True)

    keep_nodes = collect.node_ids | ways.extra_nodes
    keep_ways = ways.way_ids

    class WriteAll(osmium.SimpleHandler):
        def __init__(self, writer, keep_nodes, keep_ways):
            super().__init__()
            self.writer = writer
            self.keep_nodes = keep_nodes
            self.keep_ways = keep_ways
            self.n = self.w = self.r = 0

        def node(self, n):
            if n.id in self.keep_nodes:
                self.writer.add_node(n)
                self.n += 1

        def way(self, w):
            if w.id in self.keep_ways:
                self.writer.add_way(w)
                self.w += 1

        def relation(self, r):
            for m in r.members:
                if (m.type == "n" and m.ref in self.keep_nodes) or (
                    m.type == "w" and m.ref in self.keep_ways
                ):
                    self.writer.add_relation(r)
                    self.r += 1
                    return

    print(f"Writing {dst} ...", flush=True)
    writer = osmium.SimpleWriter(str(dst))
    out = WriteAll(writer, keep_nodes, keep_ways)
    out.apply_file(str(src), locations=True)
    writer.close()
    size_mb = dst.stat().st_size / (1024 * 1024)
    print(
        f"Done: nodes={out.n} ways={out.w} rels={out.r} size={size_mb:.1f} MiB",
        flush=True,
    )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--src",
        type=Path,
        default=Path("core/target/integration-fixtures/ostlandet-latest.osm.pbf"),
    )
    ap.add_argument(
        "--dst",
        type=Path,
        default=Path("core/target/integration-fixtures/espa-atnbrufossen-corridor.osm.pbf"),
    )
    args = ap.parse_args()
    if not args.src.is_file():
        print(f"missing source PBF: {args.src}", file=sys.stderr)
        return 1
    cut(args.src, args.dst, DEFAULT_BBOX)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
