# Map styles (online Liberty, offline Protomaps PMTiles, Mapterhorn hillshade 3D)

Navi uses MapLibre Native (`org.maplibre.gl:android-sdk` **11.13.5** GLES —
**default, finalized 2026-07-31**; previously `android-sdk-vulkan`). Maven has
GLES 11.13.5 (HTTP 200), so the tree keeps **11.13.5** and switches renderer
artifact rather than falling back to 11.8.8. History: Vulkan worked around an
AAOS emulator bearing SIGSEGV; GLES clears SM-P613 hillshade wash; AAOS
`BearingCrashIsolationTest` re-check **PASS** under GLES (no SIGSEGV). The GLES
artifact includes `pmtiles://` support added in 11.7.0 and the style.json DEM
`encoding` override from [#3570](https://github.com/maplibre/maplibre-native/pull/3570) /
[#3564](https://github.com/maplibre/maplibre-native/issues/3564) shipped in
**11.13.1**.

## On-device storage (large downloads)

Navi stores PMTiles basemap extracts, Mapterhorn DEM extracts, OSM `.pbf`
files, elevation tiles, and graph caches under the app **internal** files
directory (`Context.filesDir` → `/data/user/<id>/no.navi.app/files/…` on
device). That volume is backed by the large `/data` partition.

Earlier builds preferred `getExternalFilesDir()`. On some Automotive emulator
images the primary external volume is a tiny emulated SD card (~510 MB under
`/mnt/media_rw/…`), which cannot hold a regional basemap (~180 MB) plus DEM
(~2.7 GB). Downloads then stalled or failed with little UI feedback. Internal
storage is now the default; legacy external trees are migrated once on launch.

Download progress is logged to logcat (`NaviNative` / `NaviDownload`): start
(URL + target path + expected bytes + free space), periodic progress, completion,
and insufficient-space failures with `available_bytes`.

## Two visual basemaps (intentional)

| Mode | Style | Tile schema | When used |
|---|---|---|---|
| **Online 2D (default)** | OpenFreeMap **Liberty** | OpenMapTiles | No local PMTiles covering the camera |
| **Online 3D (opt-in)** | Liberty vector basemap + **Mapterhorn** `raster-dem` **hillshade** | OpenMapTiles + Mapterhorn DEM | User enables “3D (experimental)” (`vulkanRendererAvailable()` still returns true under GLES); network for live DEM |
| **Offline 2D** | Bundled **Protomaps light** | Protomaps | Completed local extract covers camera center |
| **Offline 3D (opt-in)** | Protomaps light + local `{region}_dem.pmtiles` hillshade | Protomaps + Mapterhorn DEM extract | Same as offline 2D, plus local DEM file beside the basemap |

Liberty and Protomaps tiles are **not interchangeable**. Offline switches to the
bundled Protomaps style. Attribution: OpenStreetMap (+ Protomaps when offline);
when 3D is active, MapLibre also shows **© Mapterhorn** from the DEM TileJSON
([attribution sources](https://mapterhorn.com/attribution)).

**Railways (offline):** Protomaps stores physical rail in `roads` with
`kind=rail`. The offline light template paints them as `roads_rail` /
`roads_rail_hatch` (from ~z11 / z13). Online Liberty already draws rail via
OpenMapTiles transportation layers. PMTiles bbox extracts do not filter out rail.

MapLibre’s **offline pack** API does **not** support PMTiles. Navi builds a local
`.pmtiles` via HTTP **range extract** from Protomaps’ public planet file — no
MapLibre OfflineManager, no project-hosted cutouts, no GitHub Releases.

## What “3D” means on MapLibre Native

The web reference (Mapterhorn terrain demo) sets style root keys:

- `terrain: { source, exaggeration }` — elevates the map mesh
- `sky: {}` — atmospheric sky
- `hillshade` layer on a second `raster-dem` source

**MapLibre Native Android does not implement mesh `terrain` or `sky`** (tracking:
[maplibre-native#252](https://github.com/maplibre/maplibre-native/issues/252);
still open as of mid-2026; style-spec SDK support tables mark both ❌ for
Android/iOS Native). Navi therefore:

1. Keeps the **existing vector basemap** (Liberty online / Protomaps offline).
2. Adds Mapterhorn DEM sources (`hillshadeSource` only; no duplicate
   `terrainSource`) and a
   **hillshade** layer (`navi-hills`) via [MapterhornTerrain] when 3D is on.
   Hillshade is inserted **below** the first hydro fill/line layer (`water` /
   `waterway*`), not below the first symbol layer, so DEM shading does not
   darken water (see [Hydro soft-edge fringe](#hydro-soft-edge-fringe-screenshot-artifact)).
3. Leaves **camera tilt independent** of the 3D toggle — map settings offer
   snapped presets **0° / 35° / 45° / 60°** (gated by
   `vulkanRendererAvailable()`, which still returns true under GLES; 60° is
   MapLibre Native’s maximum tilt). A former 65° preset could never match the
   live camera (engine clamps at 60°), which made idle tilt re-apply fight HUD
   zoom.
4. Does **not** set unsupported `terrain` / `sky` root properties (omitted on
   purpose — silent no-ops would be misleading).

Earlier Navi builds used OpenFreeMap’s pitched-Liberty workaround
(`liberty-3d`: pitch ≈ 60° / bearing ≈ 55°, no DEM) because OpenFreeMap
`/styles/3d` 404s. That pitch-only stand-in is replaced by DEM hillshade.

DEM TileJSON (confirmed at implementation time):
`https://tiles.mapterhorn.com/tilejson.json` (terrarium, tileSize 512).

## Offline DEM (Mapterhorn PMTiles)

Mapterhorn publishes `https://download.mapterhorn.com/planet.pmtiles` (terrarium
webp, maxzoom 12). Navi range-extracts the same Ostlandet bbox used for the
vector basemap into `{dataDir}/pmtiles/{region_key}_dem.pmtiles` (e.g.
`europe_norway_ostlandet_dem.pmtiles`).

When that file sits beside a completed Protomaps extract and the user opts into
3D (`vulkanRendererAvailable()`), offline mode loads **Protomaps light + local
DEM hillshade** with no network. Higher zooms than 12 need Mapterhorn’s sharded
`6-*-*.pmtiles` archives (not wired yet).

### Offline DEM encoding (olive wash) — 2026-07

**Symptom:** Opt-in 3D tints most land olive ~RGB(88,80,60) or a flat dark slab;
water and labels can look fine. With 3D off, Protomaps land is cream `#f8f4f0`.
Not wrong `earth`/`landcover` paint in `style.template.json`.

**Status on SM-P613 + `android-sdk` 11.13.5 GLES (default, finalized):** online
Liberty + remote Mapterhorn at Gjendebu **no longer washes**
(`OnlineGjendebu3dHillshadeDiagnosticTest` 2026-07-31 finalize run:
washFrac≈0.090, creamFrac≈0.144, lum_std≈41.0, mode 116,108,92, GPU Adreno
618). Offline Protomaps + local DEM also **PASS**
(`OfflineDownloaded3dScreenshotTest`: demHitsOk=**18**, washFrac≈0.002,
creamFrac≈0.70, lum_std≈16.3, elev sane, 512×512). Confirming shots:
`docs/images/tmp/online_gjendebu_3d_gles_11135.png`,
`docs/images/tmp/offline_downloaded_3d_gjendebu_gles_11135.png`. Prior **Vulkan**
on the same device was **wash outstanding** (washFrac≈0.999).

**Regression bisect:** **Earliest working Mapterhorn 3D** (adds
[MapterhornTerrain.kt], opt-in toggle + hillshade attach): **`6327e8f`** (*Fix
opt-in 3D basemap toggle…*, MapLibre **`android-sdk-vulkan` 11.8.8**). Checked
out in worktree `/tmp/navi-first-3d` (main WIP untouched). SM-P613 online
Liberty + remote TileJSON at Gjendebu (61.493, 8.351, z12, pitch 50°, airplane
off): **wash from day one** —
`docs/images/tmp/sm_p613_first_3d_6327e8f_online_3d.png`, washFrac≈**0.999**,
creamFrac≈**0**, mode **88,80,60**, lum_std≈**3.1**, terrain attached. **Do
not** bisect **`6327e8f..03a3fc3`** for SM-P613 wash; treat as **device /
Vulkan hillshade** (or upstream), not a Navi regression since first 3D.

Gallery baseline **7c50728** is also **not** good on SM-P613 (online wash at
**11.8.8**). Emulator-good tip **`03a3fc3`** on SM-P613 already washes
(`docs/images/tmp/sm_p613_base_03a3fc3_online_3d.png`, washFrac≈0.998). WIP
diff vs **`03a3fc3`** is **not** the SM-P613 hillshade root cause.

**GLES A/B (commit `6327e8f`, SM-P613, airplane off):** Throwaway worktree
`/tmp/navi-gles-ab` — dependency only: `org.maplibre.gl:android-sdk:11.8.8`
(GLES) instead of `android-sdk-vulkan:11.8.8`; no other app init changes
(`MapLibre.getInstance` unchanged). Online Liberty + remote Mapterhorn at
Gjendebu (61.493, 8.351, z12, pitch ~50°, 3D on):
`docs/images/tmp/sm_p613_gles_6327e8f_online_3d.png` — washFrac≈**0.09**,
creamFrac≈**0.15**, mode **116,108,92**, lum_std≈**41.3**, terrain attached.
Same scene on **Vulkan** at this commit:
`docs/images/tmp/sm_p613_first_3d_6327e8f_online_3d.png` — washFrac≈**0.999**,
creamFrac≈**0**, mode **88,80,60**, lum_std≈**3.1**. **Render stability (GLES on
SM-P613):** ~2 min instrumented stress (bearing 0–315°, 3D on/off cycles,
`BearingCrashIsolationTest`) — **no** FATAL, SIGSEGV, or RenderThread tombstone;
Mbgl logs `GPU Identifier: Adreno (TM) 618`. **Default (finalized 2026-07-31):** main tree links `android-sdk:11.13.5` GLES
(Maven HTTP 200; keep 11.13.5, switch renderer — preferred over falling back to
11.8.8). On AAOS AVD `xtrons` / `emulator-5554`, `BearingCrashIsolationTest`
**PASS** under GLES 11.13.5 (no MapLibre SIGSEGV / FATAL / RenderThread
tombstone; GPU Identifier: Emulator OpenGL ES Translator). SM-P613 online +
offline wash cleared under GLES (see status above). **Recommendation:** Treat
SM-P613 hillshade wash as a **Vulkan backend issue on this GPU**; ship GLES as
default.

| Step | Result |
|---|---|
| **1 — SDK bump** | **Skipped.** [#3564](https://github.com/maplibre/maplibre-native/issues/3564) fixed in **11.13.1** (we ship **11.13.5**). [#3565](https://github.com/maplibre/maplibre-native/issues/3565) still open. No evidence a newer release fixes hillshade wash; 12.x/13.x adds Vulkan regression risk. |
| **2 — terrarium→Mapbox conversion** | **Attempted;** Norway elev sane, **512×512**, round-trip **~0.1 m** — screenshot **still washed**. |

**Why conversion did not explain it alone**

1. **Mapbox path with wrong decode:** terrarium bytes treated as Mapbox → classic olive (~88,80,60).
2. **Corrected Mapbox/WebP path:** math verified, wash persisted (dark slab / olive tiles).
3. **Online control:** Liberty + **remote** terrarium TileJSON on the same device **also washes** → not only offline loopback or PMTiles encoding.

**Exp 1:** `hillshade-exaggeration` **0** → **cream** land (`creamFrac≈0.9`) while DEM tiles still load → wash is **hillshade paint**, not missing basemap vectors. Custom shadow/highlight paint is restored for the intended Navi look but is **not** the sole root cause.

**Default offline path (uncommitted tree):** [LocalDemTileServer] reads **terrarium WebP** from PMTiles and serves **terrarium-encoded** 512×512 tiles on loopback as **lossless PNG** (Native on SM-P613 did not fetch loopback WebP DEM). **`/tilejson.json`** mirrors CDN metadata. Baked `style.local.json`: `hillshadeSource.url` + inline `tiles` + `"encoding":"terrarium"`; **`attachMapterhornTerrain = false`**, but after load MainActivity **re-attaches** hillshade with an explicit loopback `TileSet` so raster-dem requests hit the server. Mapbox re-encode: `NaviMapTestHooks.localDemMapboxConversion` only.

**Hillshade paint:** shadow **#473B24**, highlight **#FFFFFF**, illumination **335°**, exaggeration **0.5** ([MapterhornTerrain]; tests may override exag).

**Tests:** [OfflineDownloaded3dScreenshotTest] — `disableGpsFollow`, camera at Gjendebu, **no airplane mode** (SM-P613 skips loopback DEM fetches in airplane). Gates: `demHitsOk >= 1`, elev sanity, **512×512**, no olive/dark **wash** (`washFrac < 0.45`), Protomaps **cream** (`creamFrac >= 0.12`), relief spread (`lum_std >= 10`). Under GLES 11.13.5 these gates **PASS** on SM-P613; must **not** pass with `demHitsOk=0`. Diagnostic: `OnlineGjendebu3dHillshadeDiagnosticTest` (online only, metrics-only).

Host extract example:

```bash
pmtiles extract https://download.mapterhorn.com/planet.pmtiles \
  europe_norway_ostlandet_dem.pmtiles --bbox=7.5,58.5,13.5,62.8 --maxzoom=12
pmtiles extract https://build.protomaps.com/YYYYMMDD.pmtiles \
  europe_norway_ostlandet.pmtiles --bbox=7.5,58.5,13.5,62.8 --maxzoom=15
```

Place both under the app `pmtiles/` dir and queue/run `europe/norway/ostlandet`
(so the job row marks completed when the basemap file already exists).

## Protomaps public planet (default basemap source)

| Item | Value |
|---|---|
| Builds UI | https://maps.protomaps.com/builds |
| Metadata JSON | https://build-metadata.protomaps.dev/builds.json |
| File URL pattern | `https://build.protomaps.com/YYYYMMDD.pmtiles` |
| Fallback (if metadata down) | `https://build.protomaps.com/20260722.pmtiles` |

At queue/run time Navi resolves the newest key from metadata (or uses the Tools
override field / fallback). Protomaps documents discourage indefinite hotlinking
of full-planet downloads; Navi only range-fetches the tiles for the selected
bbox (same idea as `pmtiles extract https://build.protomaps.com/….pmtiles out.pmtiles --bbox=…`).

Default extract **maxzoom = 15** (region paths). Paths under `test/` use maxzoom
10 for fast e2e (e.g. Geofabrik path `test/oslo`).

## Region → bbox → local file

Tools Geofabrik paths map to `region_key` + bbox (`core/src/routing/basemap/regions.rs`).
Download writes `{dataDir}/pmtiles/{region_key}.pmtiles` and a `pmtiles_jobs` row.
Style load uses `pmtiles://file://{absolute_path}` plus bundled sprites/glyphs.

## Style assets (bundled once)

`app/src/main/assets/map-styles/protomaps-light/` — template JSON, sprites, latin
glyph ranges. Copied into app files on first offline use, and again when the
prepared-style **asset epoch** changes (see `BasemapStyleResolver`), so sprite /
template fixes ship without wiping user PMTiles. Runtime file is
`style.local.v3.json` under the prepared directory.

## Basemap road names and amenity POIs (zoom ladder)

Distinct from Navi [`poi.md`](poi.md) rest/overnight **PoiIndex** categories.
Applied at style load via `BasemapLabelPolicy` (Liberty) and baked into the
offline Protomaps template (`roads_label_*`, `pois`):

| Camera zoom | Road / shield labels | Basemap amenity / peak / glacier / wetland / water POIs |
|---|---|---|
| ≥ 9 | (roads unchanged below) | Lake point names (`water_label_lake`; tile `min_zoom` often 9) |
| ≥ 10 | Same | + river line names (`water_label_river`; tile `min_zoom` often 10) |
| ≥ 12 | (roads unchanged below) | Glacier + wetland names (`pois.kind=glacier` / `wetland`); railway station names (`pois.kind=station`, OSM `railway=station`) when tile `min_zoom` allows |
| ≥ 13 | Motorways | + peak/hill name+elevation (`pois.kind=peak` / `hill`; OSM Carto / osm.org peak floor) when tile `min_zoom` allows |
| ≥ 14 | + secondary | Same |
| ≥ 15 | + other majors and minor streets | + townhall names (`pois.kind=townhall`; feature `min_zoom` often 15) |
| ≥ 16 | Same | Urban amenities (`pois` kinds other than glacier/wetland/station/townhall/peak/hill) |

Offline Protomaps peaks live in the `pois` source-layer (`kind=peak` / `hill`),
not only `places`. Glacier **names** are also `pois` points (`kind=glacier`,
tile floor ~z12) — not properties on `landuse`/`landcover` fill polygons.
Wetland **names** are likewise `pois` points (`kind=wetland`, ~z12); wetland
**polygons** in `landuse` carry no `name`. Lake/river names sit on the mixed
`water` layer (Point vs LineString) and use dedicated symbol layers — fill/line
geometry filters are unchanged (shard fix stays labeling-orthogonal). The
`pois` layer keeps a **per-kind** zoom floor (`glacier`/`wetland`/`station` → 12,
`peak`/`hill` → 13, `townhall` → 15, everything else → 16) so enabling
glacier/peak/station labels does not pull schools/fuel/etc. down to z12. Peak/hill
use **z13** (same floor as openstreetmap.org / OSM Carto peak names), not the
default z16: offline extracts are native **maxzoom 15**, so a z16 gate never
draws. Railway stations (`kind=station`) were missing from the allow-list
entirely until whitelisted — tiles already had the features; the style filter
dropped them. Station icons use the Navit `rail_station` sprite packed into
the Protomaps atlas (see [`poi-icon-whitelist.md`](poi-icon-whitelist.md)).
The baked style also pins the vector source `maxzoom` to the PMTiles
header so camera z16–z18 overzooms z15 tiles instead of requesting empty ones.
Peak/hill labels append OSM elevation when the tile carries it: Protomaps
exposes numeric `elevation` (metres), not `ele`.
The template prints metres; `BasemapPeakElevationStyle` rewrites feet for
`UnitSystem.IMPERIAL_US` (UK stays metres, same as HUD altitude). Online
Liberty `mountain_peak_label` uses the same z13 floor. Glacier labels use the
outline teal (`#2a6e70`) for contrast on ice fill `#C8E9E9`;
wetland `#3d5c3d`; both omit icons. Lake/river labels use italic `#2a5a78` so
they read apart from town and peak names.

**Out of scope — generic named buildings:** Protomaps drops `name` from the
`buildings` fill layer. Ordinary `building=yes` + `name=*` footprints that are
not classified as a `pois` kind (amenity/shop/townhall/…) are **not** in the
`pois` layer and will not get labels from this whitelist. Fixing those would
need a separate, larger schema/style change. `kind=building` was checked in
Hamar/Gjøvik z12–15 tiles and was empty there — not added to the whitelist.
OpenFreeMap Liberty **tiles** include OpenMapTiles `mountain_peak` (`ele` /
`ele_ft`); upstream Liberty JSON does not bind that layer. Navi adds
`mountain_peak_label` at runtime (`BasemapPeakElevationStyle`) so named peaks
such as Galdhøpiggen and Elgpiggen can show height online. Offline Protomaps
peaks remain `pois.kind=peak` / `hill`.

**Liberty glacier names:** OpenFreeMap Liberty / OpenMapTiles expose ice only as
`landcover` fill (`landcover_ice`) with **no** named glacier POI/label path —
upstream gap, not a Navi Protomaps bug. Leave Liberty unchanged.

**Liberty wetland names:** OpenMapTiles exposes wetland only as `landcover`
fill (`class=wetland`) with **no** name properties and no wetland entry in
`water_name` / `poi` for named bogs such as Bårdsæterkjølen — upstream schema
gap. Navi cannot fix this in style JSON; offline Protomaps is the path that
labels wetlands. Liberty lake/river labels (`water_name_*_label`,
`waterway_line_label`) already exist upstream and are left as-is.

Sprites: every kind currently on the offline `pois` whitelist has a dedicated
(or shared) atlas key — see [`poi-icon-whitelist.md`](poi-icon-whitelist.md).
`townspot` is the `icon-image` default for unknown kinds only.
Match filters must not duplicate labels (MapLibre rejects the layer with
`Branch labels must be unique`).

**Alcohol shops and vehicle repair (offline Protomaps):** `shop=alcohol` tiles
as `pois.kind=alcohol` (Planetiler often `min_zoom` 16). That kind was missing
from the `pois` allow-list, so named shops such as Vinmonopolet never drew
even when the PMTiles feature existed. The template now allows `alcohol`,
`car_repair`, `motorcycle_repair` / `motorcycle`, `bicycle_repair` /
`bicycle_repair_station` / `bicycle`. They follow the default kind floor
(**z16+**). Rest/overnight `PoiIndex` (`CraftBrewery` / `shop=alcohol`) is a
separate plane — [`poi.md`](poi.md). Other dense shop kinds (clothes, kiosk, …)
stay off the whitelist on purpose.

## Coverage fallback

| Case | Behavior |
|---|---|
| No PMTiles for camera | Live Liberty |
| PMTiles covers camera center | Prefer local Protomaps; attach local `{region}_dem.pmtiles` hillshade when 3D opted in |
| Camera leaves coverage, network OK | Switch whole map to Liberty |
| Leaves coverage, offline | Keep local style; outside extract shows empty tiles |
| Style / hillshade load failure | Fall back to Liberty 2D; clear 3D opt-in |

Status toast stays `BottomEnd` so attribution is not covered.

## Hardware-gated 3D

Never default. `MapHudPrefs.opt_in_3d` + Vulkan SDK gate. Online: Mapterhorn
HTTP TileJSON. Offline: local `{region_key}_dem.pmtiles` beside the Protomaps
extract (Ostlandet/Innlandet covered by `europe_norway_ostlandet_dem.pmtiles`).
Turning 3D off or failing hillshade attach strips DEM hillshade only — **camera
tilt stays at the user’s map-tilt preset**. Without Vulkan, tilt is forced to
0° (same gate as other non-zero camera angles on the former GLES crash path).
Never a blank map from the Kotlin attach path.

## Elevation contour lines (opt-in, independent of hillshade)

**Toggle:** Map settings → **Contours** (`MapHudPrefs.contours_enabled`). Fully
decoupled from **3D (experimental)** / `opt_in_3d`. Either, both, or neither may
be on; toggling one never changes the other.

**Data source:** Contours are **derived on-device from the same Mapterhorn
terrarium DEM** used for hillshade — not pre-rendered OSM contour vectors and not
a separate fetch pipeline. MapLibre Native has no built-in `contour` source type
([maplibre-native#4283](https://github.com/maplibre/maplibre-native/issues/4283));
Navi uses `CustomGeometrySource` + marching squares (`ContourGenerator`) over
512×512 terrarium tiles (online CDN WebP, offline `{region}_dem.pmtiles` via
`pmtilesGetTile` / `LocalDemTileServer` loopback). Contours draw from camera
zoom **9–20** (same ceiling as [MapHudPrefs.MAX_ZOOM]); DEM native max zoom
remains **12** (parent-tile oversample at higher zoom). The former **15** layer
cap hid lines at street/farm zoom (~16–18).

**Interval ladder (Kartverket-aligned):** Minor (ekvidistanse) and index
(tellekurve) spacing follow [Statens kartverk](https://www.kartverket.no/) map-series
conventions. Index spacing is always **5×** minor — the same ratio as Kartverket
index contours and Navi's existing `major` flag in [ContourGenerator]. Values
are chosen from the official scale-dependent table, mapped to Navi camera zoom
using ground scale at ~61°N (Web Mercator m/px × cos(lat); same latitude band as
the default Ostlandet extract):

| Navi zoom | Approx. ground scale (~61°N) | Kartverket series | Minor (m) | Index (m) |
|-----------|------------------------------|-------------------|-----------|-----------|
| 9–10 | ~1:580k – 1:290k | Older 1:100 000 | 30 | 150 |
| 11–12 | ~1:145k – 1:72k | N50 1:50 000 | 20 | 100 |
| 13–14 | ~1:36k – 1:18k | N20 1:20 000 | 10 | 50 |
| 15–20 | ~1:9k and closer | N5 1:5 000 | 5 | 25 |

Zoom **13** sits between N50 and N20 on the ground; **10 m / 50 m** (N20) is
used so close-in hiking views match Kartverket N20 Kartdata rather than the
coarser N50 interval. Kartverket sometimes uses a finer interval in very flat
terrain (e.g. 2 m instead of 5 m on N5) as an editorial relief choice — not
implemented in v1; would need cheap slope/relief input.

**DEM resolution vs finest interval:** Terrarium decode ([LocalDemTileServer.terrariumElevM])
stores elevation as a `Double` with sub-metre precision (RGB → metres, no integer
quantization). At z15 the marching-squares grid is resampled to **160×160**
([DemTileFetcher.sampleDimForZoom]) over each native z12 512×512 parent tile
(~4–5 km wide at Innlandet latitudes → ~25–30 m between samples). On **typical
relief** that horizontal spacing vs a 5 m vertical interval is a reasonable ratio
— contours should appear at expected density.

On **genuinely flat terrain** (fjord-bottom farmland, flat valley floors — the
case Kartverket addresses with an optional finer interval, e.g. 2 m on N5), two
distinct failure modes apply and should not be conflated:

| Failure mode | What the user sees | Cause |
|---|---|---|
| **Sparse** | Some 5 m rings appear, but fewer than a paper N5 sheet would show | Low relief per grid step; marching squares still crosses thresholds at some cells |
| **Silently absent** | A 5 m rise that exists on the ground produces **no** contour ring | The whole resampled cell (or adjacent cells) straddle an isoline threshold without any sample catching the crossing — a grid-sampling miss, not just thin spacing |

The flat-terrain editorial override remains future work (needs cheap slope/relief
input). Until then, z15 / 5 m intervals are **not fully verified** for flat sites.

**Validation gap:** Gjendebu (Besseggen/Jotunheimen) is mountainous and exercises
N20/N50 tiers well but **does not** surface the flat-terrain edge case. Before
calling the finest-interval tier production-ready, field-check a known-flat site
at **z15** and compare against Kartverket N5 or a trusted 1:5 000 sheet.

**Flat-site reference:** **Skriverhaugen**, Søndre Land (Innlandet) —
`60.715°N, 10.456°E` (OSM `natural=peak` / `hill` node in low-relief farmland
near Randsfjorden, inside the Ostlandet extract). Use camera zoom **15** with
contours + 3D/hillshade on. Gjendebu remains the mountainous regression site;
Skriverhaugen is the flat-tier (5 m / 25 m) check — separate from reinstalling
the same build at Gjendebu.

**Style:** Minor contours `#7A5C44` (1.0 px, 62% opacity); major `#3D2914`
(1.6 px, 82% opacity) — warm browns chosen to stay legible over hillshade
shadow/highlight when both are active.

**Feature properties:** Each contour segment carries `ele` (elevation in metres)
and `major` (index contour boolean) from [ContourGenerator]. Line layers
filter on `major`; labels are not drawn from properties alone.

**Index labels:** `navi-contours-label` ([BasemapContourLabelStyle]) — symbol
layer on the same source, `symbol-placement: line`, **major contours only**,
min zoom **11**, zoom-interpolated `symbol-spacing` (280 px at z11 → 120 px at
z15). Text follows line bearing (`text-rotation-alignment: map`). Units follow
[UnitSystem] (metres default; feet for `IMPERIAL_US` only, same as peak labels).
Stacked at the **bottom** of the basemap symbol block (below road/place/POI
text, above contour line layers).

**Persistence:** Same SharedPreferences file as other map HUD prefs
(`navi_map_hud`).

### Layer stack (bottom → top)

Standing rule unchanged: **all fill/line layers precede all symbol (label/icon)
layers** (`BasemapLayerOrder`). Route/GPS/waypoint overlays are re-stacked on
top after basemap mutations (`ensureRouteAboveHillshade`).

| # | Layer (group) | Hillshade OFF / Contours OFF | Hillshade ON / Contours OFF | Hillshade OFF / Contours ON | Both ON |
|---|---|---|---|---|---|
| 1 | Background + landcover/landuse fills | yes | yes | yes | yes |
| 2 | Hillshade (`navi-hills`) | — | yes (below hydro) | — | yes (below hydro) |
| 3 | Water / waterway (hydro) | yes | yes | yes | yes |
| 4 | Other fills (wetland, sand, glacier fill, …) | yes | yes | yes | yes |
| 5 | Glacier / protected outlines (line) | yes | yes | yes | yes |
| 6 | **Contour lines** (`navi-contours-*`) | — | — | yes | yes |
| 7 | Roads / paths / tunnels / bridges (line) | yes | yes | yes | yes |
| 8 | **Symbol block** — contour index elevations (`navi-contours-label`, bottom of block), road names, shields, housenumbers, place/farm/peak/POI labels, icons | **topmost basemap** | **topmost basemap** | **topmost basemap** | **topmost basemap** |
| 9 | Navi route / GPS / track overlays | above basemap | above basemap | above basemap | above basemap |

**Label guarantee:** In all four toggle combinations, housenumbers, house/place
names, road names, and POI/peak labels remain **above** hillshade and contour
line layers (rows 8–9). Hillshade sits under hydro (row 3) so water fill is not
darkened; contours sit under roads (row 7) so navigation linework stays crisp.

**Performance (pan/zoom):** Marching squares runs when MapLibre
`CustomGeometrySource` requests a **new** geometry tile (pan into unseen area,
zoom change, or invalidation) — not every frame. Hillshade is cheaper because
MapLibre paints pre-decoded raster-DEM; contours add CPU work per new tile.
Mitigations:

- **DEM grid LRU** (64 entries): decoded 512×512 terrarium tiles keyed by
  `z/x/y` — shared across viewport tiles and revisits (`NaviMapTestHooks`
  `contourDemCacheHits` / `contourDemCacheMiss`).
- **Contour feature LRU** (72 entries): marching-squares output keyed by
  `demZ/tx/ty@mapZoom` before clipping to the viewport tile
  (`contourGenCacheHits` / `contourGenCacheMiss`).
- **Zoom LOD:** interval ladder + resample dimension (`sampleDimForZoom`) cap
  grid work at low zoom (48–160 px per axis).

**Offline / online parity:** Interval ladder and LOD are pure functions of map
zoom ([ContourGenerator], [DemTileFetcher.sampleDimForZoom]) — not of fetch
path. Both online (CDN WebP) and offline (`pmtilesGetTile` terrarium WebP, or
loopback PNG from [LocalDemTileServer]) decode through the same terrarium RGB
path ([DemElevationGridDecoder]). When a viewport tile crosses a DEM tile
boundary, [DemTileCover] + [DemElevationGridDecoder.stitch] mosaic intersecting
512×512 parents before resampling so marching squares does not split at DEM
seams (remaining sub-pixel gaps at geometry-tile edges are possible but much
less visible than per-viewport-tile parent mismatch).

## UniFFI

`pmtilesPlanetUrl`, `pmtilesQueueRegion`, `pmtilesQueueDemRegion`,
`pmtilesRunJob`, pause/resume/cancel, list/covering/delete, etc.

In the Android **Tools** panel: **Download basemap (PMTiles)** and
**Download terrain DEM (Mapterhorn)** use those APIs for the selected Geofabrik
path (Ostlandet covers Innlandet; `test/oslo` is a fast smoke extract).

## Build script

`scripts/build-android-native.sh` accepts `debug` or `release` as the second
argument. Cargo has no `--debug` flag; the script maps `debug` to the default
profile (fixed; was a pre-existing footgun, not introduced by PMTiles).

## Hydro soft-edge fringe (screenshot artifact)

**Status:** reclassified — **not** a live user-visible rendering-engine
limitation. Confirmed by direct visual comparison of the live Automotive app
against instrumented screenshot captures: the blue rim at lake / river / creek
edges appears **only in captures** (`screencap` / UiAutomation), not during
normal interactive use.

### Current best understanding

Direct observation: the blue rim does **not** appear during normal interactive
use on the Automotive emulator; it appears in instrumented captures
(`screencap` / UiAutomation). That is enough to withdraw the old
“live rendering-engine limitation” framing.

Capture helpers historically waited for `styleReady` (and/or a fixed sleep)
then copied pixels immediately — the same class of prematurity as the
moving-icons work before its `styleReady` wait. Shared helpers now also wait
for MapLibre **fully-rendered + idle**
(`NaviMapTestHooks.renderSettleRequestId` /
`InstrumentedMapCapture.awaitRenderSettled`) before capturing. Roads stay sharp
in the same frames where hydro can look soft, which still suggests hydro
compositing/AA is special relative to other layers.

**Re-verification (Automotive `xtrons`, after the settle wait):** the lake /
river / creek matrix (`WaterHydroBleedScreenshotTest`) still shows a soft blue
rim in fresh `screencap` PNGs (Liberty and Protomaps, 2D and 3D). So a missing
fully-rendered+idle wait alone does **not** fully explain the capture artifact —
timing hygiene is still correct to keep, but some other difference between the
live present path and UiAutomation/`screencap` remains. Do not claim the settle
wait “fixed” gallery fringes until a capture actually comes out clean.

Do **not** over-read gallery PNGs that show the fringe: they remain valid
evidence of the **capture-tooling quirk**, not of what end users see live.

### What the hillshade reorder still means

When 3D hillshade was stacked **above** water (old “below first symbol”
insert), DEM shading darkened water and made captured fringe look worse.
`MapterhornTerrain` still inserts `navi-hills` **below** hydro fill/line
layers — that remains a real stacking improvement (hillshade stays under water;
earlier ~47% fringe-pixel reduction on old captures is still a valid measured
benefit of the reorder). Route / GPS / waypoint overlays are re-stacked on top
of the full basemap so they are not trapped under water after the reorder.

### What the 2D paint experiments meant

Style paint experiments on the emulator (`fill-antialias`, matching
`fill-outline-color`, waterway `line-blur`) were **no-ops** for the fringe in
captures. That fits a capture-timing / compositor-present story better than a
style-layer bug: roads and other non-hydro layers stayed sharp in the same
frames.

### Earlier framing (superseded)

Docs previously called this a residual engine-level limitation “below what
style configuration can control,” left open pending real-hardware confirmation.
That framing is **withdrawn** for live interactive use. Real-hardware checks
may still note whether any capture vs live discrepancy remains on device GPUs
(see [`real-hardware-testing.md`](real-hardware-testing.md#7-hydro-soft-edge-fringe-capture-vs-live)),
but the product issue is screenshot timing, not a permanent shoreline defect.

### If a capture still shows fringe after the settle wait

That is the current state after re-verification: settle wait alone is
insufficient. Treat further work as investigating why UiAutomation/`screencap`
differs from the live display for hydro edges (present path / buffer copy),
not as re-opening a live user-visible shoreline defect. Do not force a pure
timing explanation beyond what the evidence supports.

## Farm place labels (`place=farm`) — case study

**Status:** closed (2026-07). Offline Protomaps: **working as designed** at
zoom ≈ **13+**. Online Liberty: **out of scope** for style-only fixes.

### Symptom

Named OSM farms (e.g. Dystingbo near Ridabu / Hamar, `place=farm`) did not
appear on the map after style-layer attempts (place-point filter, building-name
labels). Easy to treat as another style filter miss.

### What actually differed by basemap

| Source | Schema | Tile contents at Dystingbo | Style can fix? |
|---|---|---|---|
| **Liberty (OpenMapTiles)** | `place.class` enumerates city/town/village/hamlet/…/`isolated_dwelling` — **no `farm`** | No Dystingbo; no `class=farm` in inspected z12/z14 tiles. Nearby `isolated_dwelling` is a **different** OSM tag, not a reclassification of farms. Building polygons often lack `name`. | **No.** Data never enters the tiles. |
| **Offline Protomaps** (Ostlandet extract, **maxzoom 15**) | `places.kind_detail` includes **`farm`** ([Protomaps layers](https://docs.protomaps.com/basemaps/layers)) | Farm labels appear from feature `min_zoom` (often 13) once the extract includes that zoom. | Labels are already in the bundled `places` layer (`text-field: name`). |

Lesson: **inspect the vector tile (and the schema) before editing style JSON.**
Two failed style guesses were data/schema limits on Liberty, not wrong filters.

### Offline render check (evidence)

Instrumented ladder (`FarmLabelZoomScreenshotTest`) on offline Ostlandet
PMTiles, camera on Dystingbo (`60.8022727, 11.1389560`), 3D off. Screenshots
under `docs/images/tmp/farm_zoom/` (local evidence; not required in CI).

| Camera zoom | “Dystingbo” visible? |
|---|---|
| 11 | No (farms not in z11 tiles) |
| 12 | No for this farm (other farms e.g. Farmen can show; denser collision) |
| **12.9 / 13 / 13.5 / 14 / 15** | **Yes** |

That ladder was captured against an older extract **maxzoom 12**: camera ≥ 13
still labeled from the z12 parent tile (overzoom). With the current default
extract **maxzoom 15**, farms at feature `min_zoom` 13 are present in native
z13–z15 tiles as well. Glyphs for ASCII farm names are fine. Not a client
overzoom or font bug.

### Product floor (documented, not a bug)

Farm names on offline Protomaps are expected around **zoom 13+** (aligned with
Protomaps’ feature `min_zoom: 13`). Showing them earlier would need upstream
tile encoding / collision-priority changes — not another Liberty `class=farm`
filter.

**Liberty farm labels** need a **custom tile source** that preserves
`place=farm` (same class of undertaking as other custom basemap work). Do not
chase that with style JSON on OpenFreeMap Liberty.

### If “a label just won’t show” again

1. Pick a known OSM feature and decode the covering Liberty + Protomaps tiles
   (`pmtiles tile`, `mapbox_vector_tile`, or equivalent).
2. Confirm the schema’s supported classes (`farm` vs `isolated_dwelling`, etc.).
3. Only then change style filters — or accept a schema / zoom-floor product
   decision.

## Offline Protomaps water shards (live rendering)

**Status:** fixed in style (2026-07). Distinct from the
[screenshot-only soft fringe](#hydro-soft-edge-fringe-screenshot-artifact).

### Symptom (live app)

On offline Protomaps (Ostlandet extract), Lake Mjøsa / Hamar at camera zoom
~10–11 showed **malformed water**: triangular/trapezoidal blue shards across
land, tile-aligned missing lake sections, and flooded inland areas. Online
Liberty at the same coordinates looked correct. This was visible in settled
framebuffer captures of the interactive map (`docs/images/tmp/water_live/`),
not only mid-composite test artifacts.

At zoom ~13 (historically overzoom from an extract maxzoom 12; native z13 with
today’s default maxzoom 15) the lake outline was closer to correct; the
catastrophic mid-zoom failure was the actionable bug.

### Tile data (not an extract corruption)

At `10/543/292` and neighbours, Protomaps `water` features are present
(Polygon / MultiPolygon lakes, LineString rivers, Point labels). The Ostlandet
extract tile matched the public planet build **byte-for-byte** for that z10
tile. Point-in-polygon checks place Hamar on land and Mjøsa mid-lake in water.
So this was **not** a range-fetch / missing-geometry data bug.

### Style root cause

Protomaps packs **mixed geometry** in one `water` source-layer (points, lines,
polygons). Official Protomaps styles restrict the fill layer with
`["==", "$type", "Polygon"]`. Navi’s `protomaps-light` fill had **no** geometry
filter, so MapLibre Native’s fill path also saw Point/LineString features in the
same layer — producing the shard / missing-fill failure on this SDK.

### Fix

In `app/src/main/assets/map-styles/protomaps-light/style.template.json`:

- `water` fill: filter to `Polygon` + `MultiPolygon` only
- `waterway` line: filter to `LineString` + `MultiLineString` (also picks up
  multi-part rivers the old LineString-only filter skipped)

After the filter, mid-zoom offline water matches lake shorelines again
(`docs/images/tmp/water_live_after/`). Soft shoreline fringes in **captures**
may still appear; treat those under the fringe section above, not as a
regression of this fix.

### Relation to the fringe investigation

The earlier “screenshot-only fringe” conclusion remains for the soft AA rim that
affects both styles in captures. The shard / missing-tile water failure was a
**separate, live offline Protomaps style bug** and should not be folded into
that fringe write-up.

## Offline Protomaps military + glacier (`landuse` match)

Same class of bug as the protected-area `kind` gap: tile data was present;
Navi’s `landuse` fill `match` was incomplete.

| Kind | Tile path (Ostlandet PMTiles) | Style fix |
|---|---|---|
| `military` | `landuse.kind=military` at mid/high zoom | Fill `#c96a5a` (muted dusty red for legibility — deliberate departure from Protomaps light’s gray `#dcdcdc` and from OSM Carto `#f55`) |
| `glacier` | `landcover.kind=glacier` at low zoom only; **`landuse.kind=glacier` at z8–z12+** (hiking) | Keep landcover glacier fill; **add** `landuse` glacier `#C8E9E9` (exact ice tint) |

**Liberty:** OpenFreeMap Liberty has **no** military landuse layer (upstream
omission). Navi does **not** add one — deliberate, not a bug fix. Glaciers
already use Liberty `landcover_ice` (`class=ice` / `subclass=glacier`); Navi
adds a teal dashed `landcover_ice_outline` for edge visibility.

Evidence: `MilitaryGlacierLanduseScreenshotTest` (Rena leir way `962221904`;
Gjende glacier way `380644665`). On-device captures (SM-P613):
`docs/images/military-glacier/` (`offline_pm_military_rena_z{8,10,12}.png`,
`offline_pm_glacier_gjende_z{8,10,12}.png`, `…_z12_3d.png`, plus Liberty
baselines).

Overnight safety still reads glaciers from the Geofabrik PBF / poi-barrier pack,
not these tiles — see README known issues (PBF/PMTiles skew) and
[`poi.md`](poi.md#overnight-glacier--building-exclusion).

Glacier **names** are a separate fix on the `pois` symbol layer (`kind=glacier`
from ~z12) — see [Basemap road names and amenity POIs](#basemap-road-names-and-amenity-pois-zoom-ladder).
Do not label `landuse`/`landcover` glacier fills (no `name` property).
Dashed glacier outlines (`landcover_glacier_outline` / `landuse_glacier_outline`,
and Liberty `landcover_ice_outline`) use teal `#2a6e70` and the same
`line-dasharray` `[2, 1.5]` as nature reserves, so ice edges stay visible
without looking like a park boundary. Offline outlines are stacked **after**
`landcover`/`landuse` fills (and the protected outline) and **before**
water/roads so park and other landuse polygons cannot cover the ice edge,
and **before every symbol layer** so national-park / place / POI labels stay
on top (standing rule: fills/lines then all symbols — see
`metadata.navi:layer_order` / `BasemapLayerOrder`). Liberty inserts
`landcover_ice_outline` below the first road/tunnel casing for the same reason
(upstream draws wetland/landuse/water/sand after `landcover_ice`), and adds
`park_label_protected` / peak / housenumber symbols into the symbol block only.
