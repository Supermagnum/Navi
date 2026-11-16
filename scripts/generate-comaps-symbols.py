#!/usr/bin/env python3
"""Generate the CoMaps drape symbol atlases (data/symbols/<density>/<style>/symbols.{png,sdf}).

CoMaps builds these with tools/skin_generator (Qt6 Svg) from data/styles/default/<style>/symbols/*.svg
(see tools/unix/generate_symbols.sh). Without them the drape has no symbol texture: no POI icons, no
route / user-mark pins, no road shields from symbols. The pinned checkout does not ship them.

This script produces the same two files per (density, style) with rsvg-convert + Pillow, so the
build needs no Qt6 development packages: SVGs are rendered at default_size * (symbol_size / 18),
packed (+2 px border each side) into a power-of-two atlas, and an `.sdf` XML with the packed
rectangles is written in the format the drape reads (`DefinitionLoader`).

Navi sprites: every SVG in comaps-sdk/navi-symbols/ (position puck states) and core/src/icons/aprs/
(APRS station icons) is added to the atlas of both styles (same id wins over a CoMaps symbol). The
sources live in this repository, so the sprites are generated at build time like every other symbol.

Usage: generate-comaps-symbols.py [CHECKOUT_DIR]   (default third_party/comaps)
Requires: rsvg-convert (librsvg2-bin), python3-pil.
"""
import concurrent.futures
import io
import os
import re
import subprocess
import sys
import xml.etree.ElementTree as ET

from PIL import Image

DENSITIES = [("mdpi", 18), ("hdpi", 27), ("xhdpi", 36), ("6plus", 43), ("xxhdpi", 54), ("xxxhdpi", 64)]
STYLES = ["dark", "light"]
MEDIUM = 18.0
MAX_SIZE = 4096


def qround(v: float) -> int:
    return int(v + 0.5) if v >= 0 else -int(-v + 0.5)


def svg_default_size(path: str):
    root = ET.parse(path).getroot()

    def num(v):
        if v is None:
            return None
        m = re.match(r"^\s*([0-9.]+)", v)
        return float(m.group(1)) if m else None

    w, h = num(root.get("width")), num(root.get("height"))
    if w is None or h is None:
        vb = root.get("viewBox")
        if vb:
            p = [float(x) for x in re.split(r"[ ,]+", vb.strip())]
            w, h = p[2], p[3]
    if w is None or h is None:
        w, h = 100.0, 100.0
    return max(1, qround(w)), max(1, qround(h))


def pack(items, width, height):
    """Shelf packing of (w, h) boxes sorted by height descending. Returns positions or None."""
    pos = []
    x = y = row_h = 0
    for w, h in items:
        if w > width:
            return None
        if x + w > width:
            x = 0
            y += row_h
            row_h = 0
        if y + h > height:
            return None
        pos.append((x, y))
        x += w
        row_h = max(row_h, h)
    return pos


def next_pow2(n):
    p = 1
    while p < n:
        p *= 2
    return p


def render(svg, w, h):
    out = subprocess.run(
        ["rsvg-convert", "-w", str(w), "-h", str(h), "--format", "png", svg], check=True, capture_output=True
    ).stdout
    return Image.open(io.BytesIO(out)).convert("RGBA")


def navi_symbol_dirs():
    here = os.path.dirname(os.path.abspath(__file__))
    repo = os.path.normpath(os.path.join(here, ".."))
    return [
        os.path.join(repo, "comaps-sdk", "navi-symbols"),
        os.path.join(repo, "core", "src", "icons", "aprs"),
    ]


def build(symbols_dir, out_dir, symbol_size, pool):
    by_name = {f: os.path.join(symbols_dir, f) for f in os.listdir(symbols_dir) if f.endswith(".svg")}
    for extra in navi_symbol_dirs():
        if os.path.isdir(extra):
            by_name.update({f: os.path.join(extra, f) for f in os.listdir(extra) if f.endswith(".svg")})
    syms = []
    for f in sorted(by_name):
        svg = by_name[f]
        dw, dh = svg_default_size(svg)
        sw = max(1, qround(dw * (symbol_size / MEDIUM)))
        sh = max(1, qround(dh * (symbol_size / MEDIUM)))
        syms.append({"id": f[: f.rfind(".")], "svg": svg, "w": sw, "h": sh, "pw": sw + 4, "ph": sh + 4})
    # Same order as the Qt generator: height descending, then id descending.
    syms.sort(key=lambda s: (-s["ph"], [-ord(c) for c in s["id"]]))
    dim = 0
    for s in syms:
        dim = max(dim, s["pw"], s["ph"])
    width = height = next_pow2(dim)
    items = [(s["pw"], s["ph"]) for s in syms]
    while True:
        pos = pack(items, width, height)
        if pos is not None:
            break
        if width == height:
            width *= 2
        else:
            height *= 2
        if width > MAX_SIZE:
            width = MAX_SIZE
            height *= 2
            if height > MAX_SIZE:
                raise SystemExit("atlas overflow")
    rendered = list(pool.map(lambda s: render(s["svg"], s["w"], s["h"]), syms))
    atlas = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    for s, (x, y), img in zip(syms, pos, rendered):
        atlas.paste(img, (x + 2, y + 2))
        s["x"], s["y"] = x, y
    os.makedirs(out_dir, exist_ok=True)
    atlas.save(os.path.join(out_dir, "symbols.png"), optimize=True)
    lines = ['<!DOCTYPE skin>', "<root>", f' <file width="{width}" height="{height}">']
    for s in syms:
        lines.append(
            f'  <symbol minX="{s["x"]}" minY="{s["y"]}" maxX="{s["x"] + s["pw"]}" maxY="{s["y"] + s["ph"]}" '
            f'name="{s["id"].lower()}"/>'
        )
    lines += [" </file>", "</root>", ""]
    with open(os.path.join(out_dir, "symbols.sdf"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines))
    return len(syms), width, height


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    root = sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "..", "third_party", "comaps")
    data = os.path.join(root, "data")
    with concurrent.futures.ThreadPoolExecutor(max_workers=os.cpu_count() or 4) as pool:
        for style in STYLES:
            symbols_dir = os.path.join(data, "styles", "default", style, "symbols")
            if not os.path.isdir(symbols_dir):
                raise SystemExit(f"missing {symbols_dir}")
            for res, size in DENSITIES:
                n, w, h = build(symbols_dir, os.path.join(data, "symbols", res, style), size, pool)
                print(f"{res}/{style}: {n} symbols, atlas {w}x{h}")


if __name__ == "__main__":
    main()
