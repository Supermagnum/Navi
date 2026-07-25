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

| Kind | Tool | Where documented |
|---|---|---|
| **Static** (POI, nav, status, eco leaf, …) | [Inkscape](https://inkscape.org/) → plain `.svg` (preferred) or `.svgz` | Steps below |
| **Animated** | [Synfig Studio](https://www.synfig.org/) → SVG stills / frame packs | Plugin spec: [`plugins/animated-icons-spec.md`](plugins/animated-icons-spec.md) |

Do not author map/POI icons in proprietary binary formats for the override
pipeline — the core loads SVG bytes via `usvg` / `resvg`.

### Static icons — step by step (Inkscape)

1. **Open Inkscape** and create a new document (or open an existing icon to
   edit).
2. **Set a square canvas** (File → Document Properties), e.g. 48×48 or 128×128
   user units. Keep paths simple: solid fills, few filters — map pins often
   render at 32–64 px.
3. **Draw the artwork** aligned to the semantic meaning of an existing app key
   (fuel pump, toilet, eco leaf, turn arrow, …).
4. **File → Save As… → Plain SVG** (`*.svg`). Prefer Plain SVG over “Inkscape
   SVG” so editor-only metadata does not confuse `usvg`. For large flag-like
   assets you may gzip to **`.svgz`** (same basename rules).
5. **Name the file after the semantic key** the app already uses — basename
   only, matching resolver lookup:
   - `fuel.svg`, `toilets.svg`, `leaf.svg` (eco)
   - `amenity-restaurant.svg`, `tourism-wilderness_hut.svg`
   - Day/night nav or status: `nav_straight_bk.svg` / `nav_straight_wh.svg`
6. **Install** the file in exactly one of these places:
   - **User override directory** passed to `resolve_icon` / `rasterize_icon_png`
     (same basename **wins** over the bundled set), or
   - **Bundle:** copy into `core/src/icons/` (and into the Android lean icon pack
     under `app` assets if the key must ship on-device).
7. **Verify** the key rasterizes to non-empty pixels (not blank / not
   `unknown`):

```bash
# Host or instrumented check: rasterizeIconPng / rasterize_key
# key e.g. "fuel" or "eco-mode", theme Day, size 64×64 → non-empty PNG/RGBA
```

Aliases already mapped in code (examples): `eco` / `eco-mode` → `leaf.svg`;
`water` → `drinking_water.svg`; `leisure-fishing` / `fishing` / `fish` →
`fish.svg` (Navit-derived). Prefer matching those names so overrides apply
without code changes.

### Animated icons (Synfig)

Author motion in **Synfig Studio**, then export SVG stills or frame sequences for
Navi. Full packaging, host player, reduce-motion, and proposed capabilities are
in **[`plugins/animated-icons-spec.md`](plugins/animated-icons-spec.md)** — not
in the core icon crate. Until that plugin ships, export a **representative
still SVG** and install it with the static steps above.

### Checklist (static)

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
- `leaf.svg` — eco-mode indicator (**custom** Navi drop-in; not from the Navit
  inventory — same override-by-filename mechanism as other keys; document any
  further custom art the same way)
- `fish.svg` — **pre-existing Navit-derived** icon (present in the initial
  bundled Navit set under `core/src/icons`, same provenance/timestamp class as
  e.g. `fuel.svg`). Used for `PoiCategory::Fishing` via keys `leisure-fishing` /
  `fishing` / `fish`. Covered by the existing **GPL v2** Navit licensing note
  above — not a new custom asset for the Fishing POI work.
- `unknown.svg` — final fallback

Rasterization for the Android map overlay uses `usvg` / `resvg` via
`rasterize_key` / UniFFI `rasterizeIconPng`.

## Licensing flag

Treat the Navit-derived tree as **GPL-2.0**. Do not relicense those assets
individually without upstream-compatible terms. The repository root `LICENSE`
(GPL-3.0-or-later) applies to original Navi code; asset provenance remains as
above. Custom overrides you add remain under whatever license you choose for
those files — document that next to the override set.
