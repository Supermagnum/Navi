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
above.
