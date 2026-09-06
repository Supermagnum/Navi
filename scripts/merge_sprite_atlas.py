#!/usr/bin/env python3
"""Append spreet-packed icons onto an existing MapLibre sprite atlas."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from PIL import Image


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser()
    p.add_argument("--base-json", type=Path, required=True)
    p.add_argument("--base-png", type=Path, required=True)
    p.add_argument("--add-json", type=Path, required=True)
    p.add_argument("--add-png", type=Path, required=True)
    p.add_argument("--out-json", type=Path, required=True)
    p.add_argument("--out-png", type=Path, required=True)
    p.add_argument("--padding", type=int, default=1)
    p.add_argument(
        "--skip-existing",
        action="store_true",
        help="Skip add-pack keys that already exist in the base atlas (idempotent re-runs).",
    )
    return p.parse_args()


def atlas_size(meta: dict[str, dict]) -> tuple[int, int]:
    w = h = 0
    for m in meta.values():
        w = max(w, m["x"] + m["width"])
        h = max(h, m["y"] + m["height"])
    return w, h


def main() -> None:
    args = parse_args()
    base_meta: dict[str, dict] = json.loads(args.base_json.read_text())
    add_meta: dict[str, dict] = json.loads(args.add_json.read_text())

    if args.skip_existing:
        skipped = sorted(k for k in add_meta if k in base_meta)
        add_meta = {k: v for k, v in add_meta.items() if k not in base_meta}
        if skipped:
            print(f"skip-existing: omitting {', '.join(skipped)}")
        if not add_meta:
            print("skip-existing: nothing new to merge")
            return

    base_img = Image.open(args.base_png).convert("RGBA")
    add_img = Image.open(args.add_png).convert("RGBA")

    base_w, base_h = base_img.size
    add_w, add_h = add_img.size
    pad = args.padding

    # Place new pack to the right of existing content (simple shelf merge).
    x0 = base_w + pad
    y0 = 0
    out_w = max(base_w, x0 + add_w)
    out_h = max(base_h, add_h)

    out = Image.new("RGBA", (out_w, out_h), (0, 0, 0, 0))
    out.paste(base_img, (0, 0))
    out.paste(add_img, (x0, y0))

    merged = dict(base_meta)
    for name, m in add_meta.items():
        if name in merged:
            raise SystemExit(f"sprite name collision: {name}")
        merged[name] = {
            "width": m["width"],
            "height": m["height"],
            "x": x0 + m["x"],
            "y": y0 + m["y"],
            "pixelRatio": m.get("pixelRatio", 1),
        }

    args.out_png.parent.mkdir(parents=True, exist_ok=True)
    out.save(args.out_png)
    args.out_json.write_text(json.dumps(merged, separators=(",", ":")))


if __name__ == "__main__":
    main()
