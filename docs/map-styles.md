# Map styles (online Liberty, offline Protomaps PMTiles, Mapterhorn hillshade 3D)

Navi uses MapLibre Native (`org.maplibre.gl:android-sdk-vulkan` **11.8.8**, which
includes `pmtiles://` support added in 11.7.0).

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
| **Online 3D (opt-in)** | Liberty vector basemap + **Mapterhorn** `raster-dem` **hillshade** | OpenMapTiles + Mapterhorn DEM | User enables “3D (experimental)” and Vulkan gate passes; network for live DEM |
| **Offline 2D** | Bundled **Protomaps light** | Protomaps | Completed local extract covers camera center |
| **Offline 3D (opt-in)** | Protomaps light + local `{region}_dem.pmtiles` hillshade | Protomaps + Mapterhorn DEM extract | Same as offline 2D, plus local DEM file beside the basemap |

Liberty and Protomaps tiles are **not interchangeable**. Offline switches to the
bundled Protomaps style. Attribution: OpenStreetMap (+ Protomaps when offline);
when 3D is active, MapLibre also shows **© Mapterhorn** from the DEM TileJSON
([attribution sources](https://mapterhorn.com/attribution)).

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
2. Adds Mapterhorn DEM sources (`terrainSource`, `hillshadeSource`) and a
   **hillshade** layer (`navi-hills`) via [MapterhornTerrain] when 3D is on.
   Hillshade is inserted **below** the first hydro fill/line layer (`water` /
   `waterway*`), not below the first symbol layer, so DEM shading does not
   composite on top of water edges (see [Hydro soft-edge fringe](#hydro-soft-edge-fringe-known-limitation)).
3. Leaves **camera tilt independent** of the 3D toggle — map settings offer
   snapped presets **0° / 35° / 45° / 65°** (Vulkan-gated; locked to 0° without
   Vulkan, same discipline as other non-zero camera angles on the Automotive
   emulator’s former GLES crash path).
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
3D (Vulkan gate), offline mode loads **Protomaps light + local DEM hillshade**
with no network. Higher zooms than 12 need Mapterhorn’s sharded `6-*-*.pmtiles`
archives (not wired yet).

Host extract example:

```bash
pmtiles extract https://download.mapterhorn.com/planet.pmtiles \
  europe_norway_ostlandet_dem.pmtiles --bbox=7.5,58.5,13.5,62.8 --maxzoom=12
pmtiles extract https://build.protomaps.com/YYYYMMDD.pmtiles \
  europe_norway_ostlandet.pmtiles --bbox=7.5,58.5,13.5,62.8 --maxzoom=12
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

Default extract **maxzoom = 12** (region paths). Paths under `test/` use maxzoom
10 for fast e2e (e.g. Geofabrik path `test/oslo`).

## Region → bbox → local file

Tools Geofabrik paths map to `region_key` + bbox (`core/src/routing/basemap/regions.rs`).
Download writes `{dataDir}/pmtiles/{region_key}.pmtiles` and a `pmtiles_jobs` row.
Style load uses `pmtiles://file://{absolute_path}` plus bundled sprites/glyphs.

## Style assets (bundled once)

`app/src/main/assets/map-styles/protomaps-light/` — template JSON, sprites, latin
glyph ranges. Copied into app files on first offline use.

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

## Hydro soft-edge fringe (known limitation)

**Status:** partially fixed; residual fringe open pending
[real-hardware confirmation](real-hardware-testing.md#7-hydro-soft-edge-fringe-emulator-vs-device).
Not blocking, not prioritized.

### What was fixed (emulator)

When 3D hillshade was stacked **above** water (the old “below first symbol”
insert), soft lake/river edges looked worse and hillshade darkened the water
surface. `MapterhornTerrain` now inserts `navi-hills` **below** hydro fill/line
layers. On the Automotive emulator that cut measured 3D lake fringe pixels by
about **47%** and stopped hillshade from painting over water. Route / GPS /
waypoint overlays are re-stacked on top of the full basemap so they are not
trapped under water after the reorder.

Evidence (investigation / verify shots): `docs/images/tmp/hydro_bleed_before/`,
`docs/images/tmp/hydro_bleed_hills_below/`, side-by-side crops under
`docs/images/tmp/hydro_bleed_compare/`.

### What remains

A residual soft blue rim still appears around **lakes (fill), rivers, and
creeks (lines)** in **2D and 3D**, on **both** online Liberty and offline
Protomaps-light. Style paint experiments on the emulator (`fill-antialias`,
matching `fill-outline-color`, waterway `line-blur`) were **no-ops** for this
fringe on MapLibre Native’s **Vulkan** path — the cause sits at
rasterization/compositing below what style configuration can control. Roads
and other non-hydro layers stay sharp (not a global AA regression).

### Assessment

Negligible practical impact on map usability/visibility at this time. Documented
as a known limitation; left open until the same shoreline / river / creek
checks are run on **real hardware** (emulator-only MapLibre issues have been
wrong before — see moving-icons / GLES `SIGSEGV` notes in the README and
[`real-hardware-testing.md`](real-hardware-testing.md)).

### If revisited later

Options would be engine/MSAA-level MapLibre Native renderer configuration (larger
than a style tweak) or a deliberate cosmetic coastline stroke to mask the rim.
Neither is pursued in the current pass.
