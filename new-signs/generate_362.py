#!/usr/bin/env python3
"""Compose draft Norwegian-style 362 speed plates from NPRA digit outlines.

Digits are taken from Wikimedia NPRA EPS vectors (and Riksvei 2 for digit 2),
spaced per Trafikkalfabetet V2.3a, then fitted inside the inner disc.

This writes draft SVGs only. Do not copy output into the shipped catalogue
without explicit human review.

Examples:
  python3 generate_362.py --speeds 35 45 55
  python3 generate_362.py --speeds 50 --face yellow --out /tmp/yellow-plates
  python3 generate_362.py --speeds 80 --face '#f7d117' --ring '#dd1800' --ink '#010101'
"""
from __future__ import annotations

import argparse
import math
import re
import subprocess
import sys
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEFAULT_CACHE = Path("/tmp/no-362")
CX, CY = 100.0, 100.0
INNER_R = 75.0

# Norwegian 362 defaults
DEFAULT_RING = "#dd1800"
DEFAULT_FACE = "#ffffff"
DEFAULT_INK = "#010101"

# Common alternate face for temporary / yellow-backed plates (generator option).
FACE_PRESETS = {
    "white": "#ffffff",
    "yellow": "#f7d117",
}

EPS_TO_200 = "matrix(0.08798944 0 0 -0.08798944 0 200)"
EPS90_TO_200 = "matrix(0.08818342 0 0 -0.08818342 0 200)"
YDOWN_284_TO_200 = "scale(0.70421982)"

COMMONS = {
    "362_30.svg": "https://upload.wikimedia.org/wikipedia/commons/c/c8/NO_road_sign_362.30.svg",
    "362_40.svg": "https://upload.wikimedia.org/wikipedia/commons/3/31/NO_road_sign_362.40.svg",
    "362_50.svg": "https://upload.wikimedia.org/wikipedia/commons/8/8a/NO_road_sign_362.50.svg",
    "362_60.svg": "https://upload.wikimedia.org/wikipedia/commons/9/9e/NO_road_sign_362.60.svg",
    "362_70.svg": "https://upload.wikimedia.org/wikipedia/commons/1/1b/NO_road_sign_362.70.svg",
    "362_80.svg": "https://upload.wikimedia.org/wikipedia/commons/6/6c/NO_road_sign_362.80.svg",
    "362_90.svg": "https://upload.wikimedia.org/wikipedia/commons/a/a9/NO_road_sign_362.90.svg",
    "362_100.svg": "https://upload.wikimedia.org/wikipedia/commons/9/93/NO_road_sign_362.100.svg",
    "riksvei_2.svg": "https://upload.wikimedia.org/wikipedia/commons/b/b7/Riksvei_2.svg",
}

# V2.3a H=35 mm pairwise gaps as fractions of H (left digit, right digit).
GAP_H = {
    ("1", "0"): 8 / 35,
    ("1", "5"): 7 / 35,
    ("2", "5"): 7 / 35,
    ("3", "5"): 6 / 35,
    ("4", "5"): 6 / 35,
    ("5", "5"): 6 / 35,
    ("6", "5"): 6 / 35,
    ("7", "5"): 6 / 35,
    ("8", "5"): 6 / 35,
    ("9", "5"): 7 / 35,
    ("0", "5"): 7 / 35,
}


def extract_path_d(svg_text: str, path_id: str) -> str:
    for mm in re.finditer(r"<path\b[^>]*>", svg_text):
        if f'id="{path_id}"' in mm.group(0):
            m = re.search(r'\bd="([^"]+)"', mm.group(0))
            if m:
                return m.group(1)
    raise SystemExit(f"no path {path_id}")


def tokenize_d(d: str):
    d = d.replace(",", " ")
    return re.findall(r"[MmLlHhVvCcSsQqTtAaZz]|[-+]?(?:\d*\.\d+|\d+)(?:[eE][-+]?\d+)?", d)


def parse_path_points(d: str, n_per_curve: int = 24):
    toks = tokenize_d(d)
    i = 0
    cmd = None
    cx = cy = sx = sy = 0.0
    x1 = y1 = 0.0
    pts = []

    def num():
        nonlocal i
        v = float(toks[i])
        i += 1
        return v

    def add(x, y):
        pts.append((x, y))

    def cubic(p0, p1, p2, p3):
        for t in range(n_per_curve + 1):
            u = t / n_per_curve
            w = 1 - u
            x = (
                w * w * w * p0[0]
                + 3 * w * w * u * p1[0]
                + 3 * w * u * u * p2[0]
                + u * u * u * p3[0]
            )
            y = (
                w * w * w * p0[1]
                + 3 * w * w * u * p1[1]
                + 3 * w * u * u * p2[1]
                + u * u * u * p3[1]
            )
            add(x, y)

    while i < len(toks):
        t = toks[i]
        if re.match(r"[A-Za-z]", t):
            cmd = t
            i += 1
            if cmd in "Zz":
                add(sx, sy)
                cx, cy = sx, sy
            continue
        rel = cmd.islower()
        c = cmd.upper()
        if c == "M":
            x, y = num(), num()
            if rel:
                x += cx
                y += cy
            cx, cy = x, y
            sx, sy = x, y
            add(x, y)
            cmd = "l" if rel else "L"
        elif c == "L":
            x, y = num(), num()
            if rel:
                x += cx
                y += cy
            cx, cy = x, y
            add(x, y)
        elif c == "H":
            x = num()
            if rel:
                x += cx
            cx = x
            add(cx, cy)
        elif c == "V":
            y = num()
            if rel:
                y += cy
            cy = y
            add(cx, cy)
        elif c == "C":
            a, b, c2, d2, x, y = num(), num(), num(), num(), num(), num()
            if rel:
                a += cx
                b += cy
                c2 += cx
                d2 += cy
                x += cx
                y += cy
            cubic((cx, cy), (a, b), (c2, d2), (x, y))
            x1, y1 = c2, d2
            cx, cy = x, y
        elif c == "S":
            c2, d2, x, y = num(), num(), num(), num()
            if rel:
                c2 += cx
                d2 += cy
                x += cx
                y += cy
            a, b = (2 * cx - x1, 2 * cy - y1) if cmd.upper() in "CS" else (cx, cy)
            cubic((cx, cy), (a, b), (c2, d2), (x, y))
            x1, y1 = c2, d2
            cx, cy = x, y
        elif c == "Q":
            a, b, x, y = num(), num(), num(), num()
            if rel:
                a += cx
                b += cy
                x += cx
                y += cy
            for t in range(n_per_curve + 1):
                u = t / n_per_curve
                w = 1 - u
                add(
                    w * w * cx + 2 * w * u * a + u * u * x,
                    w * w * cy + 2 * w * u * b + u * u * y,
                )
            x1, y1 = a, b
            cx, cy = x, y
        elif c == "A":
            for _ in range(5):
                num()
            x, y = num(), num()
            if rel:
                x += cx
                y += cy
            add(x, y)
            cx, cy = x, y
        else:
            raise SystemExit(f"unhandled command {cmd}")
    return pts


def mul(m, x, y):
    a, b, c, d, e, f = m
    return a * x + c * y + e, b * x + d * y + f


def parse_transform(s: str):
    m = (1, 0, 0, 1, 0, 0)
    if not s:
        return m
    for kind, args in re.findall(r"(matrix|translate|scale)\(([^)]*)\)", s):
        nums = [float(x) for x in re.split(r"[,\s]+", args.strip()) if x]
        if kind == "matrix":
            m2 = tuple(nums[:6])
        elif kind == "translate":
            m2 = (1, 0, 0, 1, nums[0], nums[1] if len(nums) > 1 else 0.0)
        else:
            sx = nums[0]
            sy = nums[1] if len(nums) > 1 else sx
            m2 = (sx, 0, 0, sy, 0, 0)
        a1, b1, c1, d1, e1, f1 = m
        a2, b2, c2, d2, e2, f2 = m2
        m = (
            a1 * a2 + c1 * b2,
            b1 * a2 + d1 * b2,
            a1 * c2 + c1 * d2,
            b1 * c2 + d1 * d2,
            a1 * e2 + c1 * f2 + e1,
            b1 * e2 + d1 * f2 + f1,
        )
    return m


def bbox(pts):
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    return min(xs), min(ys), max(xs), max(ys)


def fmt(n: float) -> str:
    s = f"{n:.8f}".rstrip("0").rstrip(".")
    return s if s else "0"


class Glyph:
    def __init__(self, name, paths, native=""):
        self.name = name
        self.paths = paths
        self.native = native
        nm = parse_transform(native)
        pts = []
        for d, _fill in paths:
            for x, y in parse_path_points(d):
                pts.append(mul(nm, x, y))
        self.pts_native = pts
        self.b = bbox(pts)
        self.w = self.b[2] - self.b[0]
        self.h = self.b[3] - self.b[1]

    def emit(self, x_left, y_top, h_target, ink: str, face: str):
        s = h_target / self.h
        t = (
            f"translate({fmt(x_left)} {fmt(y_top)}) "
            f"scale({s:.8f}) "
            f"translate({fmt(-self.b[0])} {fmt(-self.b[1])})"
        )
        if self.native:
            t += " " + self.native
        parts = [f'<g transform="{t}">']
        for d, fill in self.paths:
            # Counters punched in white on NPRA plates must follow the plate face.
            out = face if fill.lower() in ("#ffffff", "#fff", "white") else ink
            parts.append(f'<path fill="{out}" d="{d}"/>')
        parts.append("</g>")
        return "\n".join(parts)

    def placed_pts(self, x_left, y_top, h_target):
        s = h_target / self.h
        t = (
            f"translate({x_left} {y_top}) scale({s}) "
            f"translate({-self.b[0]} {-self.b[1]})"
        )
        m = parse_transform(t)
        return [mul(m, x, y) for x, y in self.pts_native]


def ensure_cache(cache: Path):
    cache.mkdir(parents=True, exist_ok=True)
    ua = {"User-Agent": "NaviSignDraft/1.0 (local plate generator)"}
    for name, url in COMMONS.items():
        dest = cache / name
        if dest.exists() and dest.stat().st_size > 500:
            continue
        print(f"download {name}")
        req = urllib.request.Request(url, headers=ua)
        dest.write_bytes(urllib.request.urlopen(req, timeout=60).read())


def load_glyphs(cache: Path):
    svg30 = (cache / "362_30.svg").read_text()
    svg40 = (cache / "362_40.svg").read_text()
    svg50 = (cache / "362_50.svg").read_text()
    svg60 = (cache / "362_60.svg").read_text()
    svg70 = (cache / "362_70.svg").read_text()
    svg80 = (cache / "362_80.svg").read_text()
    svg90 = (cache / "362_90.svg").read_text()
    svg100 = (cache / "362_100.svg").read_text()
    riks2 = (cache / "riksvei_2.svg").read_text()

    g = {}
    g["0"] = Glyph(
        "0",
        [
            (extract_path_d(svg50, "path35"), "#010101"),
            (extract_path_d(svg50, "path39"), "#ffffff"),
        ],
        EPS_TO_200,
    )
    g["1"] = Glyph("1", [(extract_path_d(svg100, "path2384"), "#010101")], YDOWN_284_TO_200)
    g["2"] = Glyph(
        "2",
        [(extract_path_d(riks2, "path1679"), "#010101")],
        "scale(0.99852009,1.0014821)",
    )
    g["3"] = Glyph("3", [(extract_path_d(svg30, "path43"), "#010101")], EPS_TO_200)
    g["4"] = Glyph(
        "4",
        [
            (extract_path_d(svg40, "path35"), "#010101"),
            (extract_path_d(svg40, "path39"), "#ffffff"),
        ],
        EPS_TO_200,
    )
    g["5"] = Glyph("5", [(extract_path_d(svg50, "path31"), "#010101")], EPS_TO_200)
    g["6"] = Glyph(
        "6",
        [
            (extract_path_d(svg60, "path31"), "#010101"),
            (extract_path_d(svg60, "path39"), "#ffffff"),
        ],
        EPS_TO_200,
    )
    g["7"] = Glyph("7", [(extract_path_d(svg70, "path31"), "#010101")], EPS_TO_200)
    g["8"] = Glyph(
        "8",
        [
            (extract_path_d(svg80, "path31"), "#010101"),
            (extract_path_d(svg80, "path39"), "#ffffff"),
            (extract_path_d(svg80, "path43"), "#ffffff"),
        ],
        EPS_TO_200,
    )
    g["9"] = Glyph(
        "9",
        [
            (extract_path_d(svg90, "path35"), "#010101"),
            (extract_path_d(svg90, "path43"), "#ffffff"),
        ],
        EPS90_TO_200,
    )
    g["0_3"] = Glyph(
        "0_3",
        [
            (extract_path_d(svg100, "path9"), "#010101"),
            (extract_path_d(svg100, "path11"), "#ffffff"),
        ],
        YDOWN_284_TO_200,
    )
    return g


def official_targets(g, cache: Path):
    pts50 = g["5"].pts_native + g["0"].pts_native
    r50 = max(math.hypot(x - CX, y - CY) for x, y in pts50)
    svg100 = (cache / "362_100.svg").read_text()
    z0b = Glyph(
        "0b",
        [
            (extract_path_d(svg100, "path13"), "#010101"),
            (extract_path_d(svg100, "path15"), "#ffffff"),
        ],
        YDOWN_284_TO_200,
    )
    pts100 = g["1"].pts_native + g["0_3"].pts_native + z0b.pts_native
    r100 = max(math.hypot(x - CX, y - CY) for x, y in pts100)
    target2 = min(r50 * 0.98, INNER_R - 8.0)
    target3 = min(r100 * 0.98, INNER_R - 8.0)
    return target2, target3


def digit_keys(speed: int):
    s = str(speed)
    keys = []
    for i, ch in enumerate(s):
        if ch == "0" and len(s) == 3 and i == 1:
            keys.append("0_3")
        else:
            keys.append(ch)
    return keys


def gaps_for(keys):
    out = []
    for a, b in zip(keys, keys[1:]):
        left = "0" if a == "0_3" else a
        right = "0" if b == "0_3" else b
        key = (left, right)
        if key not in GAP_H:
            raise SystemExit(f"no V2.3a gap for pair {left}-{right}; extend GAP_H")
        out.append(GAP_H[key])
    return out


def layout(keys, glyphs, h, y_top, gaps):
    gs = [glyphs[d] for d in keys]
    widths = [gl.w * (h / gl.h) for gl in gs]
    gap_u = [gfrac * h for gfrac in gaps]
    total = sum(widths) + sum(gap_u)
    x = CX - total / 2
    placed = []
    for i, gl in enumerate(gs):
        placed.append((gl, x, y_top, h))
        x += widths[i]
        if i < len(gap_u):
            x += gap_u[i]
    return placed


def fit_scale(placed, target_r):
    pts = []
    for gl, x, y, h in placed:
        pts.extend(gl.placed_pts(x, y, h))
    max_r = max(math.hypot(px - CX, py - CY) for px, py in pts)
    s = 1.0 if max_r <= target_r else target_r / max_r
    return s, max_r


def resolve_color(value: str, presets: dict[str, str]) -> str:
    v = value.strip()
    if v.lower() in presets:
        return presets[v.lower()]
    if re.fullmatch(r"#[0-9a-fA-F]{3}([0-9a-fA-F]{3})?", v):
        return v.lower() if len(v) == 7 else v
    raise SystemExit(f"bad color {value!r}; use a preset ({', '.join(presets)}) or #rrggbb")


def write_plate(path: Path, speed: int, placed, s_fit, ring: str, face: str, ink: str):
    body = []
    if abs(s_fit - 1.0) > 1e-9:
        body.append(
            f'<g transform="translate({fmt(CX)} {fmt(CY)}) scale({s_fit:.8f}) '
            f'translate({fmt(-CX)} {fmt(-CY)})">'
        )
    for gl, x, y, h in placed:
        body.append(gl.emit(x, y, h, ink=ink, face=face))
    if abs(s_fit - 1.0) > 1e-9:
        body.append("</g>")
    text = f"""<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
<!-- Draft, unreviewed. Not a shipped catalogue asset. Generated by generate_362.py -->
<circle cx="100" cy="100" r="100" fill="{ring}"/>
<circle cx="100" cy="100" r="75" fill="{face}"/>
<!-- {speed} km/h composite; face={face} ring={ring} ink={ink} -->
{chr(10).join(body)}
</svg>
"""
    path.write_text(text)
    print(f"wrote {path}")


def parse_args(argv):
    p = argparse.ArgumentParser(
        description="Compose draft 362-style speed plates from NPRA digit outlines."
    )
    p.add_argument(
        "--speeds",
        nargs="+",
        type=int,
        required=True,
        help="Integer km/h values to generate (e.g. 35 45 55).",
    )
    p.add_argument(
        "--out",
        type=Path,
        default=HERE,
        help=f"Output directory (default: {HERE})",
    )
    p.add_argument(
        "--cache",
        type=Path,
        default=DEFAULT_CACHE,
        help=f"Directory for cached Commons SVGs (default: {DEFAULT_CACHE})",
    )
    p.add_argument(
        "--face",
        default="white",
        help="Inner disc colour behind the numerals: white, yellow, or #rrggbb "
        f"(default white = {DEFAULT_FACE}). Yellow is for temporary / alternate "
        "faces only; it is not a substitute for national DE/UK/US plate art.",
    )
    p.add_argument(
        "--ring",
        default=DEFAULT_RING,
        help=f"Outer ring colour (default Norwegian red {DEFAULT_RING}).",
    )
    p.add_argument(
        "--ink",
        default=DEFAULT_INK,
        help=f"Numeral fill colour (default {DEFAULT_INK}).",
    )
    p.add_argument(
        "--prefix",
        default="no_sign_362_",
        help="Filename prefix (default no_sign_362_).",
    )
    p.add_argument(
        "--preview",
        action="store_true",
        help="Also write PNG previews via rsvg-convert when available.",
    )
    return p.parse_args(argv)


def main(argv=None):
    args = parse_args(argv if argv is not None else sys.argv[1:])
    face = resolve_color(args.face, FACE_PRESETS)
    ring = resolve_color(args.ring, {"red": DEFAULT_RING, "norwegian": DEFAULT_RING})
    ink = resolve_color(args.ink, {"black": DEFAULT_INK})

    ensure_cache(args.cache)
    glyphs = load_glyphs(args.cache)
    target2, target3 = official_targets(glyphs, args.cache)
    args.out.mkdir(parents=True, exist_ok=True)

    H2, Y2 = glyphs["5"].h, glyphs["5"].b[1]
    H3, Y3 = glyphs["1"].h, glyphs["1"].b[1]

    for speed in args.speeds:
        if speed <= 0 or speed > 999:
            raise SystemExit(f"speed out of range: {speed}")
        keys = digit_keys(speed)
        three = len(keys) >= 3
        h, y, target = (H3, Y3, target3) if three else (H2, Y2, target2)
        if len(keys) == 1:
            # Single digit: centre at two-digit height.
            gl = glyphs[keys[0]]
            w = gl.w * (h / gl.h)
            placed = [(gl, CX - w / 2, y, h)]
        else:
            placed = layout(keys, glyphs, h, y, gaps_for(keys))
        s, max_r = fit_scale(placed, target)
        path = args.out / f"{args.prefix}{speed}.svg"
        write_plate(path, speed, placed, s, ring=ring, face=face, ink=ink)
        print(f"  fit={s:.4f} max_r={max_r:.2f} face={face}")
        if args.preview:
            png = path.with_suffix(".png")
            try:
                subprocess.check_call(
                    ["rsvg-convert", "-w", "400", "-h", "400", "-o", str(png), str(path)]
                )
                print(f"  preview {png}")
            except FileNotFoundError:
                print("  preview skipped (rsvg-convert not installed)")


if __name__ == "__main__":
    main()
