# Norwegian road signs (Supermagnum/road-signs)

Vendored flat-icon catalogue for OSM `traffic_sign=NO:…` approach warnings in Norway.
Graphics and metadata are from Statens vegvesen / Kartverket via the upstream
[Supermagnum/road-signs](https://github.com/Supermagnum/road-signs) project.

## Snapshot

| Item | Value |
|---|---|
| Upstream commit | `be4dda9c6debe210a2a0d2fbbde5ed252714a7f4` |
| Refresh script | `scripts/vendor-road-signs.sh [commit]` |
| Core tree | `core/src/icons/road-signs/` |
| Android lean pack | `app/src/main/assets/icons/road-signs/` |
| Catalogue JSON | `core/src/icons/road-signs/database/osm_tags.json` (compile-time include) |

Categories vendored: `fareskilt/` (51), `speed_limit/` (19), `serviceskilt/` (13),
`vegvisning/` (33) — **116 SVG files**. Five catalogue entries with `"svg": null`
(`362.20`, `364.20`, `560.1`, `560.3`, `856`) are skipped until upstream adds art.

## Licensing (separate from Navit icons)

Road-sign SVGs are **not** part of the Navit GPL-2.0 icon set documented in
[`icons.md`](icons.md). They are distributed under
[NLOD 2.0](https://data.norge.no/nlod/en/2.0) via Statens vegvesen / Kartverket.
Keep attribution and licence text for these assets **separate** from Navit-derived
icons when documenting or shipping the app.

## Exclusions (runtime filter)

Matching uses only catalogue rows where:

- `"svg"` is present (116 vendored files)
- `navi_usable_as_fixed_symbol` is true (~91 after SVG filter)
- `match_status` is **not** `variable_content` (8, including `136.*`, `140`, **`812`**) or `not_for_navigation` (3: `723.71`–`723.73`)

`812` (recommended speed / advisory plate) is filed under `speed_limit` upstream but
flagged `variable_content` with `maxspeed:advisory` — it is **excluded** from fixed-icon
warnings. Compound OSM values such as `NO:100.1,812[40 km/t],807.2` match the first
usable base sign only (`100.1`); underskilt segments are ignored.

## Underskilt / compound-sign scope (known limitation)

This catalogue provides standalone, flat sign graphics only. Real-world Norwegian
signage frequently deploys warning triangles and other signs together with an
**underskilt** (supplementary plate — distance, vehicle-type, time-of-day, or
explanatory plates), which this catalogue does not represent as machine-readable
compound assemblies. Confirmed real examples: `104.1`/`104.2` commonly paired with
`813.1`/`813.2`; `156` (Other danger)'s meaning depends on its underskilt; real OSM
tagging shows compound assemblies like `NO:100.1,812[40 km/t],807.2`. Navi renders
the **base sign only** — the underskilt/plate context is not shown. This is a known,
deliberate scope limitation, not a bug, pending future underskilt-specific catalogue
data (807/813 series) becoming available upstream.

## OSM matching

Implementation: `core/src/routing/road_sign.rs`

- Primary: `traffic_sign=NO:…` (compound values split on `,`; bracket suffixes stripped)
- Companion: `hazard=*` where mapped in `osm_tags.json` implied tags
- Jurisdiction: **Norway only** for v1 (`road_sign_jurisdiction_allows`)
- Never apply `NO:` IDs outside Norway

Icon keys: `no_sign_{code}` with non-alphanumeric separators → `_`
(e.g. `no_sign_100_1` → `icons/road-signs/no_sign_100_1.svg`).

## Guidance UX

Warnings use the same approach distance phases as maneuver and speed-camera chrome
(750 m appear / 150 m urgency / 25 m hide). UI: `RoadSignWarningBox` in the approach
column — **not** a MapLibre layer.

Speed-limit **plates** from this catalogue complement the existing way-based posted-limit
HUD (`resolve_speed_limit_kmh` / edge `maxspeed`); they do not replace graph parsing.
When a warning triangle (`1xx` fareskilt) and a speed plate (`362`/`364`/`366`) are both
in the same approach phase, the warning is shown — otherwise a dense 30/40/60 cluster
hides school-area signs such as `109`.

FFI: `load_road_signs_json`, `nearest_road_sign_warning_json`, `road_sign_jurisdiction_allows`.

## Offline / pack architecture (decision)

**Decision:** one-time PBF scan at region load, JSON cache in memory (same pattern as
speed cameras). No separate indexed sidecar in v1.

**Reasoning:** tagged sign nodes are sparse; a full-region scan completes in seconds on
device and reuses the already-downloaded Geofabrik extract. Indexed packs remain reserved
for dense geometry (graph, POI/barriers, wetlands). Sign hits are reloaded when the active
region PBF changes (`LaunchedEffect(dataDir)`), preserving offline-first discipline with
no runtime network fetch of catalogue data.

Østlandet Geofabrik extract (nodes scanned on-device): **no** `traffic_sign=NO:142`
or `hazard=children` objects. School buildings (`amenity=school`) are still widely
mapped, so tag-only matching can miss school-zone risk context.

Fallback now adds a route-corridor children-zone proximity signal:

- load real child-zone POIs from the active region PBF (nodes + way geometry points):
  `amenity=school`, `amenity=kindergarten`, `leisure=playground`
- keep only POIs within **200 m** of the planned route corridor (`CorridorBand`)
- surface a single generic `142` (Children) warning in `RoadSignWarningBox` using the
  same approach phases (750 / 150 / 25 m) — one warning per approach even when several
  categories cluster (nearest POI wins)
- keep explicit mapped children warnings (`NO:142`, `hazard=child_safety`, etc.) as
  higher-priority when present

Presentation uses the same `no_sign_142` icon and generic “Children ahead” / “Children
zone: {name}” label for all three categories; sign 142’s catalogue meaning already
covers schools and playgrounds.

This fallback is jurisdiction-agnostic by design: it does not assume Norwegian
tagging completeness and works as a last-resort safety cue where explicit children
warning tags are sparse.

Innlandet east of 11°E is mis-labelled `se` by the coarse Sweden ISO ring; road-sign
jurisdiction treats that overlap as Norway so Vallset / Elverum / Løten warnings
are not suppressed.

## Tests

- Rust unit: `cargo test -p driver-break-core road_sign::`
- Icon raster: `core/tests/road_sign_icon_assets.rs`
- Device: `RoadSignIntegrationInstrumentedTest`, `RoadSignIconScreenshotTest`,
  `RoadSignSchoolCorridorInstrumentedTest` (Vallset skole / `NO:109` + children-zone
  proximity on SM-P613)
