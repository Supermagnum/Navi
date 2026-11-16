# Phase 6 -- hybrid CoMaps drape (partial) and Samsung acceptance

Status: **first pass, superseded by Phase 6b** (`docs/mwm-phase6b-drape.md`). This document describes
the hybrid (drape basemap behind a transparent MapLibre view) and its Samsung numbers; on branch
`feat/mwm-client` the MWM path is now drape only. Kept for the history of the bugs found and the
first-pass measurements.

Decision record and phase plan: `docs/mwm-client-migration.md` section 9.

## 1. What landed

### Pre-Phase-6 work (landed as part of this phase)

- `MwmProductionPlanner` + `MainActivity` wiring: with the flag ON and a session open, the
  route screen plans through the CoMaps session (`PLANNER=mwm`); `.rkyv` `planCarRoute` /
  `planHikingRoute` are blocked while it runs (`set_mwm_production_planner_active`).
- `comaps_session_cancel` / planner guard in `navi-ffi` (see bug 4 below for the cancel semantics).
- `releasetest` build type, `docs/mwm-releasetest-build.md`, `scripts/make-releasetest-keystore.sh`.

### Drape (new)

| Piece | Where | Notes |
|---|---|---|
| `:comaps-sdk` | `comaps-sdk/` | Android library over the pinned checkout's `android/sdk` Java, res and assets. **No CMake / externalNativeBuild**: `liborganicmaps.so` is the one Navi already stages. Own minimal manifest (no extra permissions, GLES 3 optional). Included by `settings.gradle.kts` only when the gitignored checkout exists; `-Pnavi.comapsDrape=false` forces it off. |
| `ComapsDrape` | `app/src/main/.../ComapsDrape.kt` | Facade with no SDK types; looks the backend up by name, so the app still builds and runs MapLibre-only without the checkout. |
| `ComapsDrapeBootstrap` | `app/src/drape/java/` (compiled only with `:comaps-sdk`) | `OrganicMaps` Platform + Framework on top of the open headless session, then `Framework.nativeChangeWritableDir({filesDir}/mwm)` so the same leaves the router uses are drawn. No-op location provider (Navi owns GPS). |
| `ComapsDrapeMap` | `app/src/main/.../ComapsDrapeMap.kt` | Compose `AndroidView` over `app.organicmaps.sdk.MapView`; `DrapeBasemapMask`; `DrapeCameraSync`. |
| `CorridorMapView` | `MainActivity.kt` | MapLibre built in translucent texture mode when maps are installed; drape added behind it once ready; pref sync loop. |

Runtime flow: session open (`ComapsSessionBootstrap`) -> `ComapsDrape.ensureInitialized` on the
main thread (about 0.5-1.2 s) -> `MapView` behind MapLibre. Failure at any step keeps the
MapLibre basemap and records the reason (`NaviMapTestHooks.drapeError`).

Camera: MapLibre is master. Each camera move pushes centre + zoom to the drape
(`Framework.nativeSetViewportCenter` with an integer level, then `Map.onScale` for the
fractional part). The level offset is derived per device from the CoMaps formulas
(`DrapeCameraSync.zoomOffset`: 0.585 on the SM-P613; calibrated on device, see 3.1). The
drape is shown only while MapLibre is **north-up and flat** (the SDK has no bearing/pitch
setter); otherwise the MapLibre basemap layers are un-masked and draw as before.

Prefs wired while the drape is active (idempotent loop, 400 ms):

| Navi | Framework |
|---|---|
| system night mode (or test override) | `MapAppearance.set(Light/Dark)` |
| contours toggle (`MapHudPrefs`) | `Framework.nativeSetIsolinesLayerEnabled` |
| 3D opt-in | `Framework.nativeSet3dMode(allow3d, allow3dBuildings)` |

### Bugs found and fixed on the way (all on code that Phase 6 depended on)

1. `ComapsSessionBootstrap.stageResources` threw on the `countries-strings` **directory**
   asset (added in Phase 5), so the production session never opened on a real install
   (only the instrumented tests, which stage their own root, worked). Now copies directories.
2. `MwmProductionPlanner` emitted `speed_kmh: null` in the simulation samples;
   `parseRouteSimSamples` then dropped the whole route, so **no simulation, progress tracker or
   off-route detection** on the MWM planner. Samples now carry the route-average pace
   (unit tests `MwmProductionPlannerTest`).
3. CoMaps' `ZipFileReader` reads APK entries as raw bytes: assets must be stored
   uncompressed. `androidResources.noCompress` now lists the CoMaps types (same list as the
   CoMaps app plus `dat`).
4. `MwmProductionPlanner.cancel()` armed the native `navi_comaps_cancel` latch. That latch is
   only consumed by the **next** route call and never interrupts a running search, so the replan
   the app starts right after `cancel()` returned CANCELLED. Cancellation is now generation based
   on the Kotlin side (in-flight plan returns CANCELLED at the next leg boundary, next plan is
   unaffected).
5. `ConnectionState` must be initialised before `OrganicMaps.init` (NPE otherwise).

### Release-build hooks

`replayHooksEnabled()` is now true for `releasetest` as well as `debug` (route/sim/camera
hooks need it); the shipped `release` is unchanged. New hooks: `requestContoursEnabled`,
`drapeNightOverride`, `drapeDisabled`, `drapeZoomOffset`, `drapeState`/`drapeError`.

## 2. Build and CI

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | pass (one pre-existing formatting diff in `mwm_comaps_ffi.rs` fixed) |
| `cargo clippy -p navi-cat -p navi-ffi --lib -D warnings` | pass |
| `cargo test -p driver-break-core --lib` (flag ON / `NAVI_MWM_CLIENT_ENABLED=0`) | 815 passed / 815 passed (MWM-off job also `flag_off_blocks_fetch`, `pack_server::`) |
| `cargo test -p driver-break-core --lib mwm::` | 55 passed |
| `cargo test -p navi-ffi --lib`, `-p navi-cat --lib` | 21 / 64 passed |
| `./gradlew :app:ktlintCheck` | pass |
| `./gradlew :app:ktlintCheck :app:testDebugUnitTest` (also with `-Pnavi.comapsDrape=false`) | pass; 514 unit tests, 0 failures (new: `MwmProductionPlannerTest`, `ComapsDrapeCameraSyncTest`) |
| `cargo deny check` | **advisories fail, pre-existing** (RUSTSEC-2026-0459 libsqlite3-sys, RUSTSEC-2026-0346/0347 rustls); bans / licenses / sources ok |
| `scripts/check-plugin-host-gate.sh` | **fails on an untracked local dir** (`tools/fdroid-check/work-vc32/` contains a vendored `plugin-host`); not caused by this branch |

APK (releasetest): **171.2 MB** with the SDK,
**134.1 MB** without (`-Pnavi.comapsDrape=false`). The SDK adds about 14 MB of assets
(fonts, symbols, ICU, vulkan shaders) that partly duplicate `assets/comaps-data`
(drules, countries, classificator: the Framework reads them from the APK root before the
writable dir moves, the headless session reads them from the staged copy).

## 3. Device acceptance (Samsung SM-P613, Android 14, `R52TB0JQEDE`, releasetest)

Physical device only; no emulator, no other app. The "location" is Navi's own route
simulation (or camera hooks). Run with `scripts/phase6-device-accept.sh install|run|pull`; steps
are the `ComapsPhase6AcceptanceInstrumentedTest` methods. Map data: the six `Norway_*.mwm`
leaves + World files of release 261105 linked (symlinks) into the production layout
`{filesDir}/mwm`. Output (gitignored): `test-artifacts/phase6/{screenshots,logs}`.

### 3.1 Screenshot index (`test-artifacts/phase6/screenshots/`)

| File | What | Notes |
|---|---|---|
| `b_align_z12_0`, `z13_5`, `z14_0`, `z14_3`, `z15_0` | Espa->Dombas route (MapLibre) over drape, zoom 12-15 incl. fractional | red route stays on the E6 casing at every zoom |
| `k_calib_auto_z13_0` ... `z17_0` | same route at the curved Espa interchange, zoom 13-17 incl. 14.5 / 15.4, analytic offset | route loops sit on the drape roundabouts at all zooms: this is the scale calibration proof. (A first attempt with offset 1.0 was 33 % off; measured best fit 0.5-0.59, formula gives 0.585) |
| `c_land_mjosa_farmland_forest_day/night` | land cover day + night, Mjosa area z12 | drape dark style in night |
| `c_land_gjendebu_alpine_day/night` | alpine: rock, ice, lakes | |
| `c_land_lillehammer_town_day/night` | town z12 | |
| `c_contours_z12/14/16_off/on` | contour lines (isolines) at 12 / 14 / 16 | isolines come from the leaf MWMs (labelled, major/minor); `off` shots are the same view without them |
| `c_3d_off_z17`, `c_3d_on_z17` | Lillehammer centre z17 | 3D on: extruded buildings with shading; off: flat footprints |
| `c_label_honefoss_oe`, `c_label_al_aa`, `c_label_favang_aa`, `c_label_gjovik_oe` | oe/aa labels (Honefoss, Al, Favang, Gjovik) | drape labels render the Norwegian glyphs (the Å in Ål is correct) |
| `c_label_e6_lillehammer`, `c_label_rv_dombas` | E6 and Rv shields / road names | |
| `perf_drape_route_planned`, `perf_drape_sim_1min/5min/end` | route overlay + puck + HUD over drape during simulation | simulation was 12x time-compressed: the drape trails the camera by its animation delay, which at 12x shows as up to about 40 px along the track |
| `realtime_sim_route_planned`, `_sim_1min`, `_sim_end` | same at **1x**: overlay and drape stay aligned along the whole screen | the real-speed evidence |
| `perf_maplibre_only_*` | same run with the drape disabled (PMTiles / online MapLibre basemap) | comparison |

Not captured: DATEX and repeater symbols on screen (no live DATEX content in the shots; repeaters are CAT-only logs).

### 3.2 CAT dry run (no Bluetooth, no radio)

Setup: Navi's own route simulation drives the app's CAT follow tick (every 5 s of real time
while the plugin is enabled); the rig is the in-process **Benshi lab** backend
(`catPluginConnectBenshiLab`, a fake VR-N76: no Bluetooth, no radio). The new
`cat_cmd set_vfo_state ...` log line (navi-cat `program_vfo1_verified`) shows every rig write; logcat
tag `NaviNative`. Repeater DB: the debug app's server mirror (12,435 sites: osm 665,
radioid 11,381, user 389; **not** pure OSM).

Place resolution (`a_resolve_places`, `logs/places-resolve.json`): "Espa" is ambiguous
(bus stops "Espa E6" x2, village, tunnel, street, kindergarten ...); the test used the first
hit **Espa E6** bus stop 60.5626,11.2546 (matches the CAT fixture coordinates). "Dombas":
railway station 62.0688,9.1255 (first of 30 hits; village centroid at 62.0756,9.1279).
"Litjfossen": rest area 62.6692,10.1522 in Oppdal (first hit; also a power station, a farm
and power lines of that name). No start/destination was moved.

| Run | Route | Mode | Time scale | Wall time |
|---|---|---|---|---|
| d | Espa->Dombas 252.3 km, 198 min plan | `auto` (nearest network = LA5MR) | 12x | 16 min |
| e | Espa->Litjfossen 302.8 km, 248 min plan | `auto` | 12x | 20.5 min |
| f | Espa->Dombas | `standalone` (non-networked) | 20x | 12 min |

Switches (route km is approximate: chord length between 5 s ticks; logs `logcat-d_cat.txt`,
`logcat-e_cat.txt`, `logcat-f_cat_standalone.txt`; analysis against the DB with
`tuned == nearest member`):

| Run | Sequence of tuned sites | Check |
|---|---|---|
| d (auto/network) | LA5MR node 1473491257 (hold) -> **LA2TRR node 5576506182 @ ~78 km** (145.2375, shift +287.9375 MHz) -> LA6JRR way 1194875539 @ ~156 km (145.6625, -0.6, CTCSS 88.5) | tuned = nearest network member at 163/167 ticks, worst excess 4.8 km (< the 5 km margin). Matches the fixture Run B (LA5MR -> LA6JRR) **plus the LA2TRR hop** |
| e (auto/network) | LA5MR (first fix, before the lab rig was connected: `VFO1 program FAIL ... backend not gated: no radio connected`, expected) -> LA5TRR node 5576656416 @ ~158 km (145.725, -0.6) -> LA9AR way 1005486272 Tronfjell @ ~197 km (145.600, -0.6) | 202/211 ticks, worst 4.6 km |
| f (standalone) | LA6GR -> **LD2HJ, LA2LRR, LA2L (RadioID DMR rows, programmed as FM)** -> LA5ARR -> LA7GR -> LA2JRR | after removing the three DMR rows the order is exactly the fixture Run A (LA6GR, LA5ARR, LA7GR, LA2JRR) |

Findings (reported, not fixed):

1. **Standalone follow programs RadioID DMR-only rows as analog FM** (LD2HJ 434.700, LA2LRR 434.700, LA2L 433.775). `CAT.md` treats DMR as info-only.
2. **LA2TRR (OSM node 5576506182, network LA5MR) has `shift_mhz = 287.9375`** in the server-mirror DB, so the host programmed a 287.9 MHz offset (TX 433.175). The fixture notes say sites without `repeater:shift` must be skipped, never guessed. Needs a data fix or a sanity gate (|offset| > 10 MHz) in `navi-cat`.
3. Wall-clock hysteresis: dwell (30 s) and gap (60 s) are real-time, so at 12x/20x they are 6/12 or 10/20 minutes of drive time. Switch positions are therefore late versus the fixture tables (which assume 80 km/h GC); at 1x they would match.
4. The first follow tick runs on the device's real GPS fix before simulation starts (logged `VFO1 program FAIL: no radio connected` when no rig is attached); harmless.

### 3.3 Function matrix (`h_function_matrix`, `logs/function-matrix.json`)

| Function | Result | Detail / cause |
|---|---|---|
| `profile_CAR` | PASS | routed as CoMaps CAR, 43.9 km / 30 min |
| `profile_CAR_ELECTRIC` | PARTIAL | routed as CoMaps CAR (43.9 km / 30 min); vehicle dimension / weight limits are not applied by the CoMaps shim |
| `profile_TRUCK` | PARTIAL | routed as CoMaps CAR (43.9 km / 30 min); vehicle dimension / weight limits are not applied by the CoMaps shim |
| `profile_MOBILE_HOME` | PARTIAL | routed as CoMaps CAR (43.9 km / 30 min); vehicle dimension / weight limits are not applied by the CoMaps shim |
| `profile_MOTORCYCLE` | PARTIAL | routed as CoMaps CAR (43.9 km / 30 min); vehicle dimension / weight limits are not applied by the CoMaps shim |
| `profile_BICYCLE` | PASS | routed as CoMaps BICYCLE, 33.9 km / 125 min |
| `profile_BICYCLE_ELECTRIC` | PASS | routed as CoMaps BICYCLE, 33.9 km / 125 min |
| `profile_HIKING` | PASS | routed as CoMaps PEDESTRIAN, 33.1 km / 412 min |
| `avoid_motorways` | PASS | 32.6 km / 38 min vs default 43.9 km / 30 min (PREFER_NON_MOTORWAY optimise mode; options persisted to CoMaps settings) |
| `avoid_tolls` | PASS | 32.6 km / 38 min vs default 43.9 km / 30 min (no toll road on this corridor; flag accepted) |
| `avoid_ferries` | PASS | default 13.2 km / 50 min vs avoid 128.8 km / 103 min |
| `vias` | PASS | 255.0 km / 204 min (direct 252.3 km / 198 min), closest approach to via 0 m |
| `tbt` | PASS | 12 maneuvers, 12 with nb+en text, 12 with nb voice text; kinds=[u_turn, exit_right, roundabout, destination] |
| `eta` | PASS | 43.9 km / 30 min => 87 km/h average (CoMaps time estimate) |
| `speed_limits` | FAIL | 0 of 835 samples carry a posted limit - the CoMaps shim returns geometry, turns and time only; overspeed HUD / limit sign have no data on the MWM planner (sim pace = route average 87 km/h) |
| `elevation` | PASS | ascent 361 m / descent 392 m, 835 elevation points, sources=[comaps], sidecar_hits=0 (climb is published to the HUD via RouteClimbSource, climb_source=mwm) |
| `wetland` | PARTIAL | hiking route OK (33.1 km / 412 min) but no *.navi-sidecar.v1.zst installed on the test device - wetland cost hook inactive, not exercised |
| `off_road_choice` | PASS | router refused (no automatic move); suggestion found=true snap=61.909746947562574,9.803558512058535 dist=5099 m, offered only as a user choice (SnapChoiceFlow) |
| `missing_maps` | FAIL | both points uncovered -> START_NOT_ON_ROAD 'FAIL: Start is not on a routable road for this profile'; start covered, end uncovered -> END_NOT_ON_ROAD 'FAIL: Destination is not on a routable road for this profile' |
| `cancel` | PASS | in-flight plan returned CANCELLED 896 ms after cancel (native router is not interruptible; result discarded at the leg boundary); the next plan routed OK (43.9 km / 30 min) |
| `gpx_export` | PASS | GPX 1.1 with <rte> and 835 <trkpt> |
| `gpx_import` | NOT_TESTED | no GPX import entry point exists in the app or FFI (export only); not changed by Phase 6 |
| `datex` | PARTIAL | refresh returned 15746 chars ({"active":[{"comment":"Vegarbeid.","delay_time_secs":null,"delays_present":false,"geometry":[[60.582645,11.273262]],"id":"cc327f9a-0ad6-4b7e-9475-fed5ed0e361b_1); live server content is outside the device acceptance and not verified for correctness |
| `airplane_mode` | PASS | no internet (online=false); route 43.9 km / 30 min, search 30 hits; DATEX offline -> '{"active":[],"active_count":0,"attribution":null,"data_source":"none","inactive"' |

Cause summary for the non-PASS rows:

- `speed_limits` FAIL: the CoMaps shim returns geometry, turns, time, ascent and elevation only; no per-segment speed or posted limit. The overspeed HUD and limit sign have no data on the MWM planner (flag OFF still has them via `.rkyv`).
- `missing_maps` FAIL: a start or end outside every installed MWM is reported as `START/END_NOT_ON_ROAD` ("not on a routable road") instead of `NEED_MORE_MAPS`; CoMaps returns `StartPointNotFound` / `EndPointNotFound` when no MWM covers the point and the shim maps those. `NEED_MORE_MAPS` only appears when the route crosses into a missing leaf. Needs a native or Kotlin bounds check.
- Vehicle profiles (truck, mobile home, motorcycle, electric car) route as CoMaps car: dimension / weight limits are not applied.
- `wetland` not exercised: no `*.navi-sidecar.v1.zst` on the test device.
- `datex`: the server answered with live data (`active` list); content was not verified. Offline (airplane + Wi-Fi off) it returns `data_source: none`.
- `cancel`: native routing cannot be interrupted (about 5-8 s for 300 km); the result is discarded within about 1 s of leg completion.
- `gpx_import` NOT_TESTED: no import entry point exists.
- Not run: config-change / activity recreation, Vulkan renderer, a second download/upgrade cycle (`ComapsSessionBootstrap.onMwmsChanged` while the drape is live; see open item 4), F-Droid recipe build.

### 3.4 Performance

Device on USB (battery stays 100 %, **no drain measurement possible**); thermal status 0 and
battery sensor 31-31.5 C throughout, no throttling. UI frame rate is the in-process
Choreographer rate (60 Hz panel); drape rate is SurfaceFlinger's per-layer present
timestamps for the SurfaceView. Samples every 15 s during route simulation with the camera
following the puck at zoom 15.

| Run | Duration | UI fps avg (min-max) | Janky frames / 15 s (>25 ms) | PSS avg (min-max) | Graphics PSS | Drape surface fps |
|---|---|---|---|---|---|---|
| Espa->Dombas, CAT on, drape (pre-calibration build) | 16.2 min | 52.5 (48.9-57.4) | 54 | 651 MB (604-691) | 186 MB | n/a (sampler fixed later) |
| Espa->Litjfossen, CAT on, drape (pre-calibration build) | 20.5 min | 53.1 (47.6-58.0) | 48 | 641 MB (615-675) | 181 MB | n/a |
| 6 min, drape (final build) | 5.6 min | 53.8 (49.4-56.7) | 44 | 654 MB (638-662) | 187 MB | **10.8 (6.8-19)** |
| 6 min, MapLibre only (final build) | 5.7 min | 52.6 (49.3-56.2) | 46 | 379 MB (348-483) | 111 MB | n/a |

Reading: UI frame rate is unchanged; the drape costs about **+230-275 MB PSS** (native heap
about 345 MB vs 152 MB). The drape surface itself only presents about 11 frames/s while following
a moving puck because each 120 ms camera push is an animated move; it is not a 60 fps map in this
design. A second drape-only run measured PSS avg 610 MB (576-639), so the memory delta varies by
about 40 MB between runs. The two long runs predate the zoom calibration; calibration only changes
the drape scale, not CAT, planner or memory behaviour.

## 4. MapLibre-only leftovers (documented, unchanged)

- Camera, gestures, rotation and tilt (including heading-up navigation and Navi 3D terrain).
- Every overlay: route GeoJSON, waypoints, GPS puck, tracks, DATEX, weather, camping, POI layers, Compose pins.
- Basemap fallback (PMTiles / online Liberty) whenever the drape is hidden (rotated or tilted camera, init failure, no maps).
- Hillshade / Mapterhorn terrain; Navi contour layers (masked while the drape shows isolines).
- MapLibre attribution control still draws over the drape; the drape's own copyright widget is not repositioned.
- `MapLibre.getInstance`, the `org.maplibre.gl:android-sdk` dependency, PMTiles / DEM download and bootstrap code.

## 5. Open items and decisions for the owner

1. Retire MapLibre / PMTiles on the MWM path (and then decide the camera owner: drape or MapLibre).
2. Move overlays into drape or one dedicated overlay surface.
3. Drape rotation / tilt: needs a native `SetViewportCenterAndAngle` / pitch entry point (JNI addition in `liborganicmaps`); today heading-up falls back to MapLibre. Drape frame rate while following (about 11 fps) would also improve with non-animated scroll/scale instead of animated `SetViewportCenter`.
4. `navi_comaps_init` re-run (`onMwmsChanged`) points the Platform *resource* dir back at `{filesDir}/mwm` after the Framework exists, which breaks lazy APK reads (fonts, symbols). Needs a one-line native guard (skip `SetResourceDir` when the resource dir is an APK); not done because it needs a `liborganicmaps` rebuild. Until then a leaf download while the drape is live should restart the app.
5. Navi has no in-app map day/night preference; the drape follows the system night mode. Add a pref or tell me to bind it to the existing sun/clock logic.
6. 3D: drape 3D = extruded buildings (visible when Navi 3D is on); Navi's pitched terrain stays MapLibre.
7. `missing_maps` message and `speed_limits` data on the MWM planner (see 3.3).
8. CAT data / logic: DMR rows in standalone follow, the 287.9 MHz shift row, provenance of the repeater DB (see 3.2).
9. APK size +37 MB (SDK assets, partly duplicated with `comaps-data`); packaging could read the staged files from one place.
10. `OrganicMaps.init` starts CoMaps' own `SearchEngine`, `BookmarkManager`, `TtsPlayer`, routing/traffic/subway managers although Navi uses none of them; trimming needs SDK changes.
11. F-Droid: the recipe must fetch the CoMaps checkout before Gradle configuration (the `:comaps-sdk` module is included only if the checkout exists at that time); verify before release.
12. Visual parity items still to judge by the owner: label languages follow the drape (device locale), drape style is CoMaps "default" (no Outdoors layer wiring), Navi POI sprites differ from CoMaps symbols.
