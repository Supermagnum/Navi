# Icon system

POI, maneuver, status, and GUI icons under `core/src/icons` are derived from the
**Navit** project and are licensed under **GPL v2** (same as the Navit codebase).
Bundling them is permitted under GPL terms. Product licensing implications of
shipping GPL-licensed assets with the rest of Navi should be decided explicitly
before a public release.

The Android launcher icon (adaptive mipmaps under `app/src/main/res/mipmap-*`)
is a separate Navi brand asset (Norway silhouette on red) and is not from Navit.

## Resolution order

`driver_break_core::icons::resolve_icon`:

1. User override directory (same filename as the semantic key)
2. Bundled Navit set under `core/src/icons`
3. Placeholder `unknown.svg`

Day/night pairs use `_bk` / `_wh` suffixes. Country flags are `.svgz`
(gzip-compressed SVG) and are gunzipped before SVG parsing.

## Adding custom icons

Custom icons must be **SVG** (or gzip-compressed **`.svgz`** for flags / large
sets). Raster-only sources (PNG/JPEG) are not used by the resolver — convert or
re-draw to SVG first.

### Authoring tools

| Kind | Tool | Output |
|---|---|---|
| **Static** (POI, nav, status, eco leaf, …) | [Inkscape](https://inkscape.org/) | Plain `.svg` (preferred) or `.svgz` |
| **Animated** | [Synfig Studio](https://www.synfig.org/) | Design in Synfig; export for Navi as SVG frames or an SVG that the host can present (see note below) |

Do not author map/POI icons in proprietary binary formats for the override
pipeline — the core loads SVG bytes via `usvg` / `resvg`.

### Static icons (Inkscape)

1. Create or open the artwork in **Inkscape**.
2. Prefer a square canvas (e.g. 48×48 or 128×128 user units) and keep paths
   simple (few filters; solid fills work best when rasterized small).
3. Save as **Plain SVG** (`*.svg`), not Inkscape SVG with editor-only extras if
   you can avoid them — plain SVG rasterizes more reliably.
4. Name the file after the **semantic key** the app already uses, for example:
   - `fuel.svg`, `toilets.svg`, `leaf.svg` (eco)
   - `amenity-restaurant.svg`, `tourism-wilderness_hut.svg`
   - Day/night nav or status: `nav_straight_bk.svg` / `nav_straight_wh.svg`
5. Install the file by either:
   - **User override:** put it in the override directory passed to
     `resolve_icon` / rasterize (same basename wins over the bundled set), or
   - **Bundle:** copy into `core/src/icons/` (and into the Android lean icon pack
     under `app` assets if the key must ship on-device).
6. Verify with UniFFI / host:

```bash
# From a host that calls rasterize_key / rasterizeIconPng for key "fuel"
# Theme Day, size 64×64 — expect non-empty PNG/RGBA
```

Aliases already mapped in code (examples): `eco` / `eco-mode` → `leaf.svg`;
`water` → `drinking_water.svg`. Prefer matching those names so overrides apply
without code changes.

### Animated icons (Synfig Studio)

1. Author the animation in **Synfig Studio**.
2. Export in a form Navi can consume as SVG:
   - **Preferred for map markers today:** export a **representative still** (or
     the first/key frame) to SVG via Synfig’s SVG export / Inkscape cleanup, then
     install like a static icon; or
   - **Frame sequence:** export SVG (or PNG) frames and let a future UI layer
     animate them; the core rasterizer currently renders **one SVG document per
     call** (no Synfig `.sif` playback in `usvg`).
3. Keep frame artworks square and high-contrast at small sizes (map pins are
   often 32–64 px on screen).
4. Place exported SVG under the override or bundle path using the same semantic
   key naming as static icons.

**Runtime note:** `rasterize_key` / `rasterizeIconPng` produce a single bitmap
from SVG/SVGZ. Full Synfig timeline playback is not implemented in the Rust
icon crate; Synfig is the **authoring** tool for animated designs, with SVG
(or frame exports) as the interchange format into Navi.

### Checklist

- [ ] Format is `.svg` or `.svgz` (not only PNG)
- [ ] Filename matches the semantic key (and `_bk` / `_wh` if themed)
- [ ] File is in override dir or `core/src/icons` (and APK assets if needed)
- [ ] Day theme resolves; night theme resolves if you shipped `_wh`
- [ ] Rasterize smoke-test returns non-empty pixels (not blank / not `unknown`)
- [ ] License of *your* artwork is documented if it is not GPL-v2 Navit material

### Example: override eco leaf

```text
<override_dir>/leaf.svg          # Inkscape plain SVG
```

Resolution order finds `leaf.svg` in the override dir before
`core/src/icons/leaf.svg`. Keys `eco` and `eco-mode` also resolve to `leaf.svg`.

## Inventory (semantic keys)

Icons are named by OSM-style keys where applicable, for example:

- `amenity-*` (fuel, toilets, restaurant, …)
- `tourism-*` (wilderness_hut, alpine_hut, attraction, …)
- `natural-*`, `leisure-*`, `shop-*`
- Craft brewery / alcohol retail maps to `shop-alcohol` when classified as
  `PoiCategory::CraftBrewery`
- `leaf.svg` — eco-mode indicator (custom drop-in example)
- `unknown.svg` — final fallback

Rasterization for the Android map overlay uses `usvg` / `resvg` via
`rasterize_key` / UniFFI `rasterizeIconPng`.

## Licensing flag

Treat the Navit-derived tree as **GPL-2.0**. Do not relicense those assets
individually without upstream-compatible terms. The repository root `LICENSE`
(GPL-3.0-or-later) applies to original Navi code; asset provenance remains as
above. Custom overrides you add remain under whatever license you choose for
those files — document that next to the override set.
