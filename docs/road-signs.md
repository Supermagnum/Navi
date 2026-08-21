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
`vegvisning/` (33) — **116 SVG files**. Catalogue entries with `"svg": null`
(`362.20`, `364.20`, `560.1`, `560.3`, `856`) are skipped by `load_catalog` until
upstream adds art. Look-forward speed-limit cone ships a Navi-drawn stand-in
`no_sign_362_20.svg` (Vienna-style red ring + path digits) so 20 km/h plates
do not fall back to `unknown.svg`. Odd numeric `maxspeed` values from OSM that
lack a dedicated 362 plate snap to the nearest shipped plate
(`20/30/40/50/60/70/80/90/100/110`).

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

**Decision:** parse once into **compact native point sets** at region load
(catalogue signs, children **centroids** only, speed cameras, speed bumps), then
query in memory. Prefer an on-disk layer cache under
`live_hazards_cache/<pbf-stem>/` (`signs.json`, `children.json`, `cameras.json`,
`bumps.json`) when present so low-RAM devices skip multi-pass Ostlandet-scale
PBF rescans. Host extract helper:
`cargo run -p navi-ffi --bin live-hazard-extract --release -- <pbf> <out_dir>`.

No separate indexed sidecar in v1 for these sparse point hazards.

**Reasoning:** tagged sign / calming / camera nodes and facility centroids are
sparse (~MB for Østlandet, not tens of MB of way vertices). Re-decoding full
region JSON on every GPS tick was ruled out (tens–hundreds of ms). Compact
points + a cell window keep per-tick UniFFI cost in the low-millisecond range
on SM-P613.

Hits are (re)ingested when the active region PBF / cache key changes
(`LaunchedEffect(dataDir)`), preserving offline-first discipline with no
runtime network fetch of catalogue data.

Østlandet Geofabrik extract (nodes scanned on-device): **no** `traffic_sign=NO:142`
or `hazard=children` objects. School buildings (`amenity=school`) are still widely
mapped, so tag-only matching can miss school-zone risk context.

### Planned-route corridor (200 m)

When a route is active, children-zone proximity uses the corridor band:

- load child-zone facilities from the active region (nodes + **way centroids**):
  `amenity=school`, `amenity=kindergarten`, `leisure=playground`
- keep only facilities within **200 m** of the planned route corridor (`CorridorBand`)
- surface a single generic `142` (Children) warning in `RoadSignWarningBox` using the
  same approach phases (750 / 150 / 25 m) — one warning per approach even when several
  categories cluster (nearest facility wins)
- keep explicit mapped children warnings (`NO:142`, `hazard=child_safety`, etc.) as
  higher-priority when present

### Live hazard cone without a route (300 m) — product name: **Look forward**

When **no** progress tracker / planned route is active, GPS position + heading
drive the same approach chrome via a **route-independent cone** (README:
**Look forward**):

| Item | Value |
|---|---|
| Radius | **300 m** (distinct from the 200 m route-corridor children band) |
| Half-width | ±60° from GPS heading (isotropic disk if bearing unknown) |
| Categories | Catalogue road signs, speed bumps → `NO:109`, children centroids → `142`, opted-in speed cameras, upcoming speed-limit plate from the **existing** `road_label_near` cell graph |
| Window | Same ~0.05° cell + 1 pad as idle street / posted-limit refresh |
| Priority | Explicit tagged `142` > children proximity > other signs/humps; nearest-wins among clustered child-zone categories |
| Jurisdiction | Same Norway road-sign gate and camera jurisdiction / opt-in as the corridor path — a missing route must not bypass gates |

Implementation: `core/src/routing/live_hazard.rs`, UniFFI
`live_hazard_cone_*` / `live_hazards_ingest_from_json` / `live_speed_limit_cone_json`,
host wiring in `MainActivity.kt` (cone only when `progressTracker == null`).

Presentation uses the same `no_sign_142` / `no_sign_109` icons and
“Children ahead” / “Children zone: {name}” labels as the corridor path.

The **corridor** children proximity fallback remains a last-resort safety cue
where explicit children warning tags are sparse (it does not invent Norwegian
catalogue IDs outside the cone/sign jurisdiction path). Catalogue signs and
speed-bump `109` plates on the live cone stay Norway-gated like other road-sign
warnings.

Innlandet east of 11°E is mis-labelled `se` by the coarse Sweden ISO ring; road-sign
jurisdiction treats that overlap as Norway so Vallset / Elverum / Løten warnings
are not suppressed.

## Tests

- Rust unit: `cargo test -p driver-break-core road_sign::` and `live_hazard::`
- Icon raster: `core/tests/road_sign_icon_assets.rs`
- Device (corridor): `RoadSignIntegrationInstrumentedTest`, `RoadSignIconScreenshotTest`,
  `RoadSignSchoolCorridorInstrumentedTest` (Vallset skole / `NO:109` + children-zone
  proximity on SM-P613)
- Device (live cone): `LiveHazardConeVallsetInstrumentedTest` (built-in simulator,
  route-independent Vallset corridor), `LiveHazardConeOverheadInstrumentedTest`
  (compact ingest + tick cost; prefers `live_hazards_cache/`)
