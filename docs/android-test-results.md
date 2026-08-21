# Android + corridor test results (full on-device wiring)

Date: 2026-07-21 (follow-up pass); toast/attribution status reconciled 2026-07-29.

**Canonical role:** chronological evidence log for Android instrumented /
emulator runs. For “what is the current product status?”, start at
[`status.md`](status.md) — do not treat older rows in this file as live truth
when a later Item supersedes them.

## Environment

| Item | Value |
|---|---|
| Emulator | `emulator-5554` / xtrons AVD API 35 |
| ABI | `x86_64` |
| Fixture HTTP | `http://10.0.2.2:8765/` (host `python3 -m http.server`) |
| Corridor PBF | `espa-atnbrufossen-corridor.osm.pbf` (~58 MiB cut from Ostlandet) |
| DEM seed | `elevation-corridor.tar` via in-app provisioner |
| Screenshot | `core/target/integration-fixtures/route_map.png` |

## Data source policy (item 0)

| Test | `TEST_KIND` | `DATA_SOURCE` | Validates |
|---|---|---|---|
| `smokeTest_ffiLinkageOnly_labeledSmoke` | **SMOKE** | `none` | UniFFI + `WorkerPoolPlan` only |
| `realPipeline_provisionsViaDownload_thenRoutes` | **REAL_PIPELINE** | `real_pbf` | parse → graph → eco reweight → cache → POI → route |
| `iconRasterization_producesNonEmptyBitmaps` | **ICON_RASTER** | `real_svg` | usvg/resvg bitmaps |
| `mapIsVisible_withRouteOverlay_andScreenshot` | (uses REAL_PIPELINE + UI) | `real_pbf` | MapLibre basemap + route + marker + screencap |

Silence on `DATA_SOURCE` must be treated as smoke/stub. This suite labels every report explicitly.

## Connected suite result

```
tests=4 failures=0 errors=0 skipped=0 time≈35.8s
```

| Test | Result | Notes |
|---|---|---|
| `smokeTest_ffiLinkageOnly_labeledSmoke` | **PASS** | Labeled SMOKE; no routing claim |
| `realPipeline_provisionsViaDownload_thenRoutes` | **PASS** | HTTP provision (no manual `adb push`); `DATA_SOURCE=real_pbf`; cache hit on 2nd load |
| `iconRasterization_producesNonEmptyBitmaps` | **PASS** | fuel, nav_straight, status_routing, eco-mode + country_NO.svgz |
| `mapIsVisible_withRouteOverlay_andScreenshot` | **PASS** | Screenshot evidence below |

## Item 1 — Automated provisioning (no manual adb push)

- In-app / FFI: `provision_region_data(data_dir, pbf_url, pbf_filename, elevation_tar_url?)`
- Emulator reaches host fixtures at `http://10.0.2.2:8765/...`
- Scripts: `scripts/cut-corridor-extract.py`, `scripts/prepare-android-fixtures.sh`, `scripts/serve-android-fixtures.sh`
- Instrumented tests call the same provisioner the UI uses

## Item 2 — Reweighted graph cache

- `core/src/routing/graph/cache.rs` (`NAVIGPH1` + bincode)
- Pipeline cold wipe → plan: with indexed packs, report `pack_hit=true` and warm pack load << cold; without packs, note `.navigph` deprecated and both passes rebuild from PBF (`cache_hit` on `.navigph` is no longer expected)

## Item 3 — Icon rasterization

- `usvg`/`resvg` + `flate2` for `.svgz` in `core/src/icons`
- FFI: `rasterize_icon_png` / `rasterize_icon_check`
- On-device: non-empty PNG/RGBA for POI, nav, status, eco; flag key resolves `.svgz`

## Item 4 — Visible map (screenshot evidence)

File: [`core/target/integration-fixtures/route_map.png`](core/target/integration-fixtures/route_map.png)

Captured on-device via `screencap -p /data/local/tmp/navi_route_map.png` during the instrumented map test (pulled with `adb pull`).

Confirmed in the image:

- MapLibre basemap (Norway demotiles, MapLibre logo)
- Red route line overlay (Espa → Atnbrufossen corridor geometry)
- Rasterized POI marker + label along the route
- Launcher uses `appicon.svg` (dock icon)

## Item 5 — Stub / gap audit (reachable + related)

| Location | Status |
|---|---|
| `core/src/bus/mod.rs` | Minimal stubs (inter-thread snapshots) — not corridor-critical |
| `core/src/sensors/mod.rs` | Placeholders; GPS/IMU out of scope |
| `core/src/lib.rs` / ECU | Live ECU/OBD out of scope (extension point only; see [`docs/ECU.md`](docs/ECU.md)) |
| `core/src/icons` `unknown.svg` | Intentional fallback placeholder when a POI `icon_key` has no matching asset in the lean on-device pack |
| No `todo!()` / `unimplemented!()` | in corridor / FFI / map paths for this pass |

Known gaps after this pass (acknowledged, not claimed PASS):

- On-device icon asset pack is a **lean subset** of `core/src/icons` (full Navit set remains on disk in core; not all keys are bundled into the APK).
- Corridor extract is bbox-cut from Ostlandet (~58 MiB), not a full country download UX with progress UI polish.
- Ferrostar SDK is not used; MapLibre is wired directly (matches “Ferrostar hands rendering to MapLibre” split).
- Sensor/ECU/bus modules remain stubs as above.

## Commands (no `tee`)

```bash
python3 scripts/cut-corridor-extract.py
# serve fixtures:
python3 -m http.server 8765 --bind 0.0.0.0 --directory core/target/integration-fixtures
./scripts/build-android-native.sh x86_64-linux-android release
./gradlew :app:installDebug :app:installDebugAndroidTest
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.CorridorInstrumentedTest
adb pull /data/local/tmp/navi_route_map.png core/target/integration-fixtures/route_map.png
```

## Item 6 — Multi-zoom POI screenshots (closing pass)

Center: `58.991547, 6.138377` (basemap / standard map POIs, not route overlay).

Instrumented test: `no.navi.app.ZoomPoiScreenshotTest` via `NaviMapTestHooks.pendingCamera`.

| Zoom | Host path | Size |
|---|---|---|
| 6.5 (regional) | [`core/target/integration-fixtures/zoom_z6_5.png`](core/target/integration-fixtures/zoom_z6_5.png) | 117355 bytes |
| 11.0 (town) | [`core/target/integration-fixtures/zoom_z11.png`](core/target/integration-fixtures/zoom_z11.png) | 108077 bytes |
| 16.0 (street) | [`core/target/integration-fixtures/zoom_z16.png`](core/target/integration-fixtures/zoom_z16.png) | 96775 bytes |

```bash
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.ZoomPoiScreenshotTest
adb pull /data/local/tmp/navi_zoom_6_5.png core/target/integration-fixtures/zoom_z6_5.png
adb pull /data/local/tmp/navi_zoom_11.png core/target/integration-fixtures/zoom_z11.png
adb pull /data/local/tmp/navi_zoom_16.png core/target/integration-fixtures/zoom_z16.png
```

Connected result: `tests=1 failures=0` (2026-07-21 closing pass).

## Item 7 — HUD bar fix visual confirmation (2026-07-22)

Instrumented: `HudVerificationInstrumentedTest` **PASS** on `emulator-5554`
(xtrons AAOS). Fresh pulls under [`docs/images/hud/`](docs/images/hud/)
(including `hud_map_top_bottom_only.png`).

Status key: **fixed and visually confirmed** / **confirmed-broken** /
**still-needs-testing**.

| Claim | Status | Evidence |
|---|---|---|
| Collapsed by default (not stuck-open Map settings) | **fixed and visually confirmed** | [`hud_map_top_bottom_only.png`](docs/images/hud/hud_map_top_bottom_only.png) — single-line top (Map / Alt / N-up) + bottom; map settings closed |
| Bottom: break + ETA only (no `Turn --`, no Settings link) | **fixed and visually confirmed** | Same shot + apply shots; test asserts no `Turn --` |
| Eco on bottom when active | **fixed and visually confirmed** | Green `ECO` on bottom bar in `hud_map_top_bottom_only.png` (42 strong-green pixels in HUD band) |
| Eco absent when inactive | **fixed and visually confirmed** | [`hud_settings_eco_off.png`](docs/images/hud/hud_settings_eco_off.png) / [`hud_eco_off.png`](docs/images/hud/hud_eco_off.png) — eco switch off, 0 ECO green in HUD band |
| Tap top → map settings; tap bottom → drive settings; Apply/Close collapses | **fixed and visually confirmed** | [`hud_rot_mode_compass.png`](docs/images/hud/hud_rot_mode_compass.png), [`hud_settings_open.png`](docs/images/hud/hud_settings_open.png), [`hud_after_break_hours_apply.png`](docs/images/hud/hud_after_break_hours_apply.png) |
| Tap map does nothing to sheets | **fixed and visually confirmed** (Item 9) | `HudVerificationInstrumentedTest#hud_map_tap_does_not_affect_settings_sheets` — map tap does not open/close/toggle sheets; pan still works |
| One app zoom −/+ on bottom bar; AAOS `- 63 +` is climate | **fixed and visually confirmed** | App −/+ on bottom HUD; climate `- 63 +` in system bar (distinct position/style) |
| Toast vs MapLibre attribution | **fixed and visually confirmed** (Item 8) | Was broken when toast sat bottom-left over attribution. Fixed: shared `status_toast` at **BottomEnd**. Do not treat older Item 7 screenshots as current status — see Item 8 evidence. |
| Auto-zoom −/+ enabled styling consistent | **fixed and visually confirmed** | Map settings use matching `OutlinedButton` styling for `−`/`+`; [`hud_auto_zoom_preset.png`](docs/images/hud/hud_auto_zoom_preset.png) re-shot on SM-P613 (2026-08-20) |

### Collapsed bar heights vs Garmin reference

Measured on `hud_map_top_bottom_only.png` (1280×720) via lavender HUD-band rows:

| Bar | Pixels | % of screen | Garmin ref |
|---|---|---|---|
| Top | 48 (y 86–133) | **6.67%** | ~14% (instruction bar — different role) |
| Bottom | 64 (y 510–573) | **8.89%** | ~6.4% |

Content-driven `heightIn(min ≈ 48/46.dp)` is **legible** on this density; top is
much thinner than the Garmin approach/instruction reference (expected for the
collapsed strip). Bottom is slightly taller than the ~6.4% strip reference.

Note: `hud_upper_lower_bars_with_menus.png` currently MD5-matches
`hud_profile_menu.png` (duplicate capture) — gallery entry still needs a
correct re-shot.

## Item 8 — Toast placement, eco leaf icon, settings overlays (2026-07-22)

Instrumented: `HudVerificationInstrumentedTest` **PASS**.

| Item | Before | After | Evidence |
|---|---|---|---|
| Status toast vs attribution | Toast in bottom column (bottom-left) covered MapLibre/OSM | Shared `status_toast` chip at **BottomEnd** (`bottom = 88.dp`) — all status strings | [`hud_status_toast_settings_applied.png`](docs/images/hud/hud_status_toast_settings_applied.png), [`hud_after_rest_mins_apply.png`](docs/images/hud/hud_after_rest_mins_apply.png) |
| Eco indicator | Fell back to green **ECO** text (`leaf.svg` often missing after partial icon copy) | Rasterized `leaf.svg` via `eco-mode` / icon pipeline; no text fallback | [`hud_eco_leaf_on.png`](docs/images/hud/hud_eco_leaf_on.png) / [`hud_eco_leaf_off.png`](docs/images/hud/hud_eco_leaf_off.png) |
| Settings sheets | **In-layout:** map sheet under top bar in scroll column; drive sheet stacked above bottom bar in bottom column | **Overlay:** both sheets are Box layers with `zIndex` above map + bars; bar positions unchanged | [`hud_settings_overlay.png`](docs/images/hud/hud_settings_overlay.png), [`hud_map_settings_overlay.png`](docs/images/hud/hud_map_settings_overlay.png) |

Root causes fixed in code: recursive/`leaf.svg` refresh in `ensureIconsCopied`; toast `Alignment.BottomEnd`; sheets moved out of chrome columns.

## Item 9 — Map tap does not affect settings sheets (2026-07-22)

Instrumented: `HudVerificationInstrumentedTest` **PASS** (2/2), including
`hud_map_tap_does_not_affect_settings_sheets` (closes Item 7 “tap map does
nothing to sheets”).

| Case | Result |
|---|---|
| Sheets closed + map tap → still closed | **PASS** |
| Map settings open + map tap → stays open, mode/zoom/toggles unchanged | **PASS** |
| Drive settings open + map tap → stays open, fields unchanged | **PASS** |
| Sheets closed + pan / zoom gesture still work | **PASS** (pan + double-tap zoom; synthetic pinch begins but does not complete zoom on this AVD) |

Also fixed while enabling gestures: track-overlay Canvas forwards touches to MapView;
camera-idle is source of truth for zoom/lat/lon (poll no longer overwrites);
bearing updates no longer re-apply Compose zoom.

## Item 10 — OSM update copy, cross-region prompts, expanded catalog (2026-08-19)

Instrumented: `no.navi.app.OsmUpdateCatalogRoutingFollowupTest` (+ isolated
`d_sweden_border_keyboard_prompt` re-run). Device: **Samsung Galaxy Tab S6 Lite
(SM-P613)**, serial `R52TB0JQEDE`, Android 14. Initial pass 2026-08-19; fylke
crossing follow-up **`e_`/`f_`** re-run **2026-08-20** (both **PASS**, 29 s).

Connected suite result:

```
tests=6 failures=0  (a→c + isolated d/e/f)
```

| Test method | Result |
|---|---|
| `a_keyboard_four_routes_missing_coverage` | **PASS** — Grotli/Hjelle + Os/Roros prompt; Fagernes/Gol + Strandlykkja/Morskogen do not |
| `b_catalog_granularity_sweden_us_russia_germany` | **PASS** |
| `c_osm_check_and_apply_show_plain_language` | **PASS** |
| `d_sweden_border_keyboard_prompt` (isolated) | **PASS** |
| `e_fagernes_gol_crosses_fylke_stays_in_ostlandet` (isolated) | **PASS** |
| `f_strandlykkja_morskogen_near_border_stays_in_ostlandet` (isolated) | **PASS** |

Log tag: `OsmCatalogFollowup`.

### OSM update messaging (Tools toggles)

| | Before (bug) | After (confirmed on device) |
|---|---|---|
| **Check for OSM updates** | Raw FFI/planner dump shown in Tools status, e.g. `USER_VISIBLE=true`, `local_sequence=…`, `Full re-download recommended… reason=Local Geofabrik sequence unknown…`, or `OSM update check unsupported.\nreason=No region_meta.json…` | **"New map data is available. Tap Apply pending OSM update to download."** (when update available) or plain up-to-date / no-region strings |
| **Apply pending OSM update** | Raw apply report, e.g. `PASS\nmethod=full_redownload\nreason=…` piped into status | **"Download in progress…"** then **"Map data updated. Preparing search and routes in the background."** when indexing starts |

Implementation: `OsmUpdateUserCopy.kt` maps check/apply reports; Tools status
and bottom toast use `userFacingStatus()` safety net. Unit tests:
`OsmUpdateUserCopyTest`.

Device evidence (2026-08-19): Check → *"New map data is available…"*; Apply →
*"Download in progress…"*. No `method=`, `reason=`, `USER_VISIBLE=`, or
`region_meta` strings in on-screen copy.

### Cross-region / cross-border routing (keyboard entry, car profile)

Fixture: **Ostlandet-only** download (`ostlandet-latest.osm.pbf`). Routes entered
via Route search coordinates (`lat, lon`) per standing keyboard rule.

| Route | Prompt? | Suggested download | Notes |
|---|---|---|---|
| **Grotli → Hjelle** | **Yes** | `europe/norway/vestlandet` | Destination west of Ostlandet bbox (`lon` 7.16 < min 7.5) |
| **Os → Røros** | **Yes** | `europe/norway` (country) | Cross-landsdel trip; dialog copy names Norway |
| **Fagernes → Gol** | **No** | — | Crosses **fylke** (Innlandet ↔ Buskerud) but both endpoints stay in **Ostlandet**; no download prompt (`e_fagernes_gol_crosses_fylke_stays_in_ostlandet`) |
| **Strandlykkja → Morskogen** | **No** | — | Near the Norway–Sweden border but both endpoints remain in Norway/Ostlandet; no prompt (`f_strandlykkja_morskogen_near_border_stays_in_ostlandet`) |
| **Rundfloen tollstasjon → Långflons Köpcentrum** | **Yes** (isolated re-run) | **`europe/sweden`** | Norway→Sweden border case; message: *"…is in Sweden, which is not downloaded. Download Sweden to plan this trip."*; **Download Sweden** action; dismiss clean (`poly=0`) |

Sweden fix: `RegionCoverage` uses Norway–Sweden border polyline +
identity-aware coverage (Ostlandet bbox overlap no longer masks Sweden).
Re-run log: `path=europe/sweden`, `prompted=true`.

### Expanded Geofabrik catalog (Tools download scope)

Honest granularity from Geofabrik HEAD + on-device picker notes
(`GeofabrikDownloadCatalog.regionGranularityNote`):

| Picker request | Real Geofabrik granularity | Confirmed size / behaviour |
|---|---|---|
| **Sweden → Kronobergs län** | **Country only** — Geofabrik page: *"No sub regions are defined for this region."* | `europe/sweden-latest.osm.pbf` **~814 MB**; `europe/sweden/kronobergs-lan` returns **~2.9 KB HTML stub** (not a PBF) |
| **USA → West Virginia** | US **state** extracts exist; country picker lists `north-america/us` only | `north-america/us/west-virginia-latest.osm.pbf` **~98 MB**; typed-path download started on device |
| **Russia** | Country **+ federal districts** on Geofabrik; **no district chips** in Tools picker today | `russia-latest.osm.pbf` **~4.1 GB**; `russia/kaliningrad-latest.osm.pbf` **~28 MB** download started; bogus slug `russia/central-federal-district` is HTML stub — UI note points at typed paths like `russia/kaliningrad` |
| **Germany → Bremen** | German **Bundesland** extracts exist; picker lists country only | `europe/germany/bremen-latest.osm.pbf` **~21 MB**; typed-path download started on device |

Norway remains the only country with **Region in country** sub-region chips
(Ostlandet, Vestlandet, …). Other countries: switch to **Country** or type a
Geofabrik subpath in the path field.

### Commands

```bash
./gradlew :app:installDebug :app:installDebugAndroidTest
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.OsmUpdateCatalogRoutingFollowupTest
# Sweden border only:
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.OsmUpdateCatalogRoutingFollowupTest#d_sweden_border_keyboard_prompt
# Ostlandet-only fylke-crossing sanity (no prompt expected):
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.OsmUpdateCatalogRoutingFollowupTest#e_fagernes_gol_crosses_fylke_stays_in_ostlandet,no.navi.app.OsmUpdateCatalogRoutingFollowupTest#f_strandlykkja_morskogen_near_border_stays_in_ostlandet
adb logcat -s OsmCatalogFollowup:I
```

Unit tests (host): `./gradlew :app:testDebugUnitTest --tests no.navi.app.OsmUpdateUserCopyTest --tests no.navi.app.RegionCoverageTest --tests no.navi.app.GeofabrikDownloadCatalogTest`

## Item 11 — Cycle routing, US routes, water POI pickup (SM-P613, 2026-08-20)

Device: **SM-P613** (`R52TB0JQEDE`). Fixture: **Ostlandet** +
eventual **west-virginia** / **nevada** downloads. Log tags:
`CycleWaterFollowup`, `UsRoutesFollowup`.

### Norway cycle routes (keyboard station names)

| Route | Profile | Result | Notes |
|---|---|---|---|
| **Elverum stasjon → Tynset stasjon** | Bicycle | **PASS** | FTS snap 60.883/11.547 → 62.275/10.776; **211 km** multi-leg UI plan; `slow_road_preference=applied` on single-leg audit (**204 km**); corridor uses **Nord-Østerdalsveien**, pilgrim/cycling network, local roads — **no Rv 3 or Fv 237 in street labels** (official cycling network + speed-class penalty avoids Rv 3 without labelled Fv 237 segments) |
| **Gjøvik stasjon → Kyrkjestølen** | Bicycle | **PASS** (prompt) | Missing-coverage dialog: *"Download **Norway** so the whole corridor is covered."* `path=europe/norway`; no polyline (expected) |

### US routes — initial pass (bugs found)

| Route | Profile | Initial result | Blocker |
|---|---|---|---|
| **CKB → Stringtown → Sandusky WV** | Car | Blocked | Missing-coverage suggested **`russia`** (bug); WV only **partial** (~89 MB) on disk |
| **Reese River → Eureka B&B NV** | Car | Blocked | Same **`russia`** suggestion; Nevada not downloaded |

### Water POI pickup (Norway)

| Route | Count | Examples |
|---|---|---|
| Elverum → Tynset | **4** | Unnamed sources near Elverum start; **Bjørns kilde** (~61.562, 11.164) at ~96 km along corridor |
| Gjøvik → Kyrkjestølen | N/A | No route (coverage prompt) |

---

## Item 12 — Region-suggestion fix, partial resume, US routes completed (SM-P613, 2026-08-20)

### 1. Russia region-suggestion bug — root cause and fix

**Root cause:** `suggest_geofabrik_path_for_point` picked the **smallest bbox area**
among all matches. **`russia`** and **`north-america/us`** both use Geofabrik
index bboxes spanning **longitude −180°…+180°** (antimeridian/world-spanning
metadata). For US points both match; Russia's **lat span is narrower**, so its
bbox **area** was smaller than the US country bbox — Russia won incorrectly.

**Fix** (`core/src/routing/basemap/regions.rs`):

- Added `bbox_is_coarse_longitude_fallback()` (lon span ≥ 120°).
- Two-pass selection: prefer **non-coarse** bboxes first, then coarse country
  fallbacks only if nothing tighter matches.
- Added **`north-america/us/west-virginia`** and **`north-america/us/nevada`**
  state bboxes (from Geofabrik index geometries).
- `RegionCoverage.suggestGeofabrikPath()` now delegates to UniFFI
  `suggestGeofabrikPath()` (single source of truth).

**Verified on device** (`UsRoutesRegionFollowupTest#a_region_suggestion_us_and_regression`):

| Coordinate | Before | After |
|---|---|---|
| CKB area (39.297, −80.228) | `russia` | **`north-america/us/west-virginia`** |
| Reese River (39.434, −117.272) | `russia` | **`north-america/us/nevada`** |
| Oslo regression (59.91, 10.75) | (unchanged) | **`europe/norway/ostlandet`** |

Rust unit tests: `us_state_beats_global_longitude_country_extracts`,
`coarse_longitude_fallback_only_when_no_tighter_match` — **PASS**.

**Proactive audit:** other coarse-longitude catalog entries include
`north-america/us`, `russia`, `antarctica`, and `australia-oceania` — all
excluded from the tight-match pass so sub-regions win when present.

### 2. Stale West Virginia `.partial` download

**Finding: expected behaviour, not a resume bug.**

- `west-virginia-latest.osm.pbf.partial` (**~89 MB**) from the earlier catalog
  session was **not consumed** on the cycle-routing pass because the missing-coverage
  prompt suggested **`russia`**, the user/test **dismissed** without starting a WV
  download, and `downloadedGeofabrikPaths()` intentionally lists **complete**
  `.osm.pbf` files only (> 1 MB), not `.partial` siblings.
- The shared HTTP downloader (`core/src/download/http.rs`) **does** auto-resume
  from `.partial` when `provision_region_data` / `download_file` runs for the
  same destination filename.
- **Confirmed on follow-up:** `provisionRegionData` for WV resumed the partial
  and finished **`west-virginia-latest.osm.pbf`** at **98 339 645 bytes** (~2 s
  after test start — not a full re-download).
- Added explicit resume logging in `provision_region` when a non-empty partial
  exists before download.

### 3. US routes re-run (complete downloads + planning)

Downloads (`UsRoutesRegionFollowupTest`):

| Region | File size | Method |
|---|---|---|
| West Virginia | **98 MB** | Resume from partial + complete |
| Nevada | **123 MB** | Fresh download (~11 s) |

Routes (Car profile; keyboard **`lat, lon`** entry in UI; native `planCarRoute`
for planning — UI Plan exceeded 10 min on first US graph build on SM-P613):

| Route | Distance | Result | Water POIs |
|---|---|---|---|
| **CKB (39.297, −80.228) → Stringtown (39.456, −79.707) → Sandusky WV (39.556, −80.859)** | **201 km** (2 legs) | **PASS** | **1** — `water:8996923368` at (39.517, −80.095), sample ~108 km |
| **Reese River (39.434, −117.272) → Eureka (39.513, −115.962)** | **134 km** | **PASS** | **46** along corridor (many mapped spring/water nodes in central NV; e.g. `water:10015106031` at route start) |

WV low POI count is plausible (rural highway corridor); Nevada high count
matches OSM spring density in the Reese River / Austin area — not a pickup bug.

### Commands

```bash
./scripts/build-android-native.sh aarch64-linux-android release
./gradlew :app:installDebug :app:installDebugAndroidTest
cargo test -p driver-break-core 'basemap::regions::tests'
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.UsRoutesRegionFollowupTest
adb logcat -s UsRoutesFollowup:I CycleWaterFollowup:I
```

## Item 13 — Norwegian road signs + children-zone proximity (SM-P613, 2026-08-20)

Device: **SM-P613** (`R52TB0JQEDE`). Fixture: **Ostlandet** Geofabrik extract.
Log tags: `RoadSignIntegration`, `RoadSignSchool`.

### Implementation summary

| Layer | What shipped |
|---|---|
| Catalogue | Vendored Supermagnum/road-signs (`be4dda9`) plus 12 generated every-5 km/h plates and the `362.20` stand-in; **129** SVG keys; Norway jurisdiction gate |
| Tagged signs | PBF scan at region load; `nearest_road_sign_warning_json`; approach box |
| Children-zone fallback | `amenity=school`, `amenity=kindergarten`, `leisure=playground` POIs; 200 m route corridor; single generic **142** warning (`source=children_proximity`); explicit tagged **142** wins |
| Dedup | Nearest POI across categories — no stacked warnings when school + kindergarten cluster |

### Device tests

| Test | Result | Notes |
|---|---|---|
| `RoadSignIntegrationInstrumentedTest` (5 methods) | **PASS** | Index includes all three POI categories; Vallset barnehage corridor; cluster dedup; 200 m boundary; `NO:109` probe unchanged |
| `RoadSignSchoolCorridorInstrumentedTest#vallset_skole_proximity_school_warning_on_real_route` | **PASS** | Real Fv1890 corridor; `142` at ~67 m; screenshot `vallset_skole_school_proximity_142.png` |

Evidence screenshots: [`docs/screenshots/road-signs/`](screenshots/road-signs/).

### Commands

```bash
./scripts/build-android-native.sh aarch64-linux-android release
./gradlew :app:installDebug :app:installDebugAndroidTest
adb shell am instrument -w -e class \
  'no.navi.app.RoadSignIntegrationInstrumentedTest,no.navi.app.RoadSignSchoolCorridorInstrumentedTest#vallset_skole_proximity_school_warning_on_real_route' \
  no.navi.app.test/no.navi.app.NaviAndroidTestRunner
cargo test -p driver-break-core road_sign::
```

Design reference: [`road-signs.md`](road-signs.md).

## Item 14 — PMTiles/DEM download progress (Luxembourg bbox, SM-P613, 2026-08-20)

Instrumented: `LuxembourgDownloadProgressInstrumentedTest` (log tag
`LuxembourgProgress`). Verifies monotonic byte progress and non-stale labels during
Protomaps region + DEM queue/download for `europe/luxembourg` (small bbox — fast
on-device check).

Fixes covered: coalesced range download progress callbacks; indexed-map convert
stage labels on the shared `download_progress` channel; Tools status shows plain
**"Download in progress…"** during OSM apply (Item 10) — no raw FFI dumps.

```bash
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.LuxembourgDownloadProgressInstrumentedTest
adb logcat -s LuxembourgProgress:I NaviDownload:I
```

## Item 15 — Live hazard cone (route-independent, SM-P613, 2026-08-21)

Device: **SM-P613**. Fixture: Ostlandet PBF + host-extracted
`live_hazards_cache/ostlandet-latest.osm/` (compact layers; avoids multi-pass
PBF OOM on the tablet). Log tags: `LiveHazardConeVallset`, `LiveHazardConeOverhead`.

### Implementation summary

| Layer | What shipped |
|---|---|
| Compact points | Signs, children **centroids**, cameras, speed bumps — parse once / ingest once |
| Window | Same cell as `road_label_near` (~0.05° + 1 pad) |
| Cone | **300 m**, ±60° heading; approach chrome 750 / 150 / 25 m |
| Speed-limit look-ahead | Existing cell graph only — no new dataset |
| Host path | Cone when `progressTracker == null`; corridor path unchanged with a planned route |

### Device tests

| Test | Result | Notes |
|---|---|---|
| `LiveHazardConeOverheadInstrumentedTest` | **PASS** | Ingest ~96 ms; **COMPACT_TICK mean ≈ 2.9 ms**; compact UTF-8 est ≈ 2.2 MB; `cone_m=300` |
| `LiveHazardConeVallsetInstrumentedTest` (3 methods) | **PASS** | Simulator, no planned route; Vallset children `142` + `NO:109`; near-miss ~350 m does not fire |

```bash
# Optional host extract → push cache for low-RAM devices
cargo run -p navi-ffi --bin live-hazard-extract --release -- \
  /path/to/ostlandet-latest.osm.pbf /tmp/live_hazards_cache/ostlandet-latest.osm
adb push /tmp/live_hazards_cache/ostlandet-latest.osm \
  /data/local/tmp/navi_fixtures/live_hazards_cache/ostlandet-latest.osm

./scripts/build-android-native.sh aarch64-linux-android release
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.LiveHazardConeOverheadInstrumentedTest
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.LiveHazardConeVallsetInstrumentedTest
```

Design reference: [`road-signs.md`](road-signs.md), [`route-simulation.md`](route-simulation.md).

## Item 15 — Generated every-5 km/h speed plates (2026-08-21)

Dedicated look-forward plates for OSM `maxspeed` values
`5/10/15/25/35/45/55/65/75/85/95/105` (digit-composited from official NLOD 362
plates; not original government art for those codes). Snap fallback remains only
for odd values still without a shipped plate.

| Check | Result |
|---|---|
| `live_hazard` unit: dedicated icons for 5/45/65/105 | **PASS** |
| `road_sign_icon_assets`: all cone plates resolve; catalogue rasterize | **PASS** (no `unknown.svg`) |
| Docs / upstream | [`road-signs.md`](road-signs.md); [Supermagnum/road-signs#1](https://github.com/Supermagnum/road-signs/pull/1) |

On-device confirmation of dedicated plates for real `maxspeed=45` / `65` ways is still recommended after installing this build.
