# Phase 6b -- CoMaps drape is the only map renderer on the MWM path

Status: **Phase 6 acceptance met on the Samsung SM-P613 (releasetest), with documented
decisions** (section 15). Phase 7 and 8 are untouched. Decision record and plan:
`docs/mwm-client-migration.md` section 9; the first (hybrid) pass is `docs/mwm-phase6-drape.md`.

All device numbers below come from `scripts/phase6-device-accept.sh` against the physical
tablet `R52TB0JQEDE` (Navi releasetest only, no emulator, no other app). Raw output:
`test-artifacts/phase6b/{screenshots,logs}` (gitignored).

## 1. Architecture change

| | Phase 6a (hybrid) | Phase 6b |
|---|---|---|
| Map surface | MapLibre (translucent texture) over the drape | **Drape only** (`ComapsDrapeMap`) |
| Camera owner | MapLibre; the drape followed with a per-device zoom offset (0.585 on this tablet) | **Drape** (`NaviDrape` camera API); `state.cameraZoom` is the CoMaps scale level, no offset |
| Rotation / tilt | MapLibre; drape hidden when rotated or tilted | Drape: gestures, heading-up (follow-and-rotate), automatic perspective in navigation |
| Follow | MapLibre camera pushes of centre + zoom (about 11 drape fps) | Drape follow mode fed with 1 Hz positions ([ComapsDrape.pushLocation]); **30 fps** |
| Overlays | MapLibre GeoJSON layers + Compose pins | CoMaps route renderer, route points, user marks, tracks (section 4) |
| Basemap fallback | PMTiles / online style when the drape hid | None: without maps the surface is a placeholder with a download entry |
| Map data download | none in the app (adb-linked leaves) | `MwmMapDownloader` + Tools section; live register, no restart |

Flag OFF (`NAVI_MWM_CLIENT_ENABLED=0` / `-Pnavi.mwmClient=false`) still composes the unchanged
`CorridorMapView` (the function is byte-identical to the Phase 5 version) with MapLibre and
PMTiles. On the MWM path `MapLibre.getInstance`, the PMTiles world overview download and the
Geofabrik / PMTiles / DEM region section of Tools are skipped.

### Native patch `patches/comaps/0004-navi-drape-camera-overlays-and-leaf-lock.patch`

**905 lines (+612 / -17) in 15 files**, applies on top of 0001-0003 to the pinned tag
(checked on a clean worktree). Biggest part: new `android/sdk/src/main/cpp/navi/navi_drape.cpp`
(407 lines; JNI class `app.organicmaps.sdk.NaviDrape`, compiled into the app).

| Area | Lines | What |
|---|---|---|
| `navi_drape.cpp` (new) | +407 | camera set / get / scale / rotate / fit-rect, follow modes, navigation camera (`FollowRoute`), tap block, screen to lat/lon, register maps, settings key, route subroutes (+turn arrows), route points, mark groups, track groups |
| `drape_frontend` (`my_position_controller`, `frontend_renderer`, `drape_engine`, `message_subclasses`) | +62 / -3 | `SetFollow` / `SetFollowAndRotate` messages (direct mode switch instead of cycling), north-up reset, zoom-in on the first fix from the start view |
| `libs/map/framework.hpp` | +2 | `GetCurrentModelView()` |
| `navi_comaps.cpp/.h` | +71 / -6 | `navi_comaps_set_platform_locked` (re-init guard, see 2b), `navi_comaps_leaf_for_point`, `speed_limits_json` in the route result |
| routing (`routes_builder`, `navi_wetland_cost`) | +66 / -7 | per-segment posted maxspeed JSON; wetland lookup now per leaf (feature ids are per MWM) |
| `CMakeLists.txt` | +2 / -1 | new source |

Kotlin / Java side: `ComapsDrapeBootstrap` (src/drape), `NaviDrape.java`, `NaviLocationBridge.java`
(public access to the package-private location entry), `ComapsDrapeMap.kt`.

CoMaps builds with `-ffast-math`: `isnan()` is optimised away, so the camera API never uses NaN as
"keep" (a bearing outside +-720 degrees means "keep rotation"). Found the hard way: the first
version produced a NaN camera.

## 2. Blockers from the function matrix

### 2a. Wetland sidecars on device

Why no `*.navi-sidecar.v1.zst` in production storage:

1. **There was no download path in the app.** `core/src/mwm` (catalog, mirrors, download) was never exported
   through UniFFI or called from Kotlin; the device runs used adb-linked leaves, which have no sidecars.
2. `ComapsSessionBootstrap` opened the router with `loadWetlandSidecars = false`, so even present sidecars
   would not have been read.
3. Latent: the wetland lookup callback scanned every loaded sidecar although feature ids are per MWM; with
   several leaves loaded the wrong leaf could answer. The hook now passes the segment's leaf name.

Fix: `core/src/mwm/install.rs` (+ `merge_staging_into_release`), `navi-ffi/src/mwm_install_ffi.rs`
(`mwm_list_regions`, `mwm_install_leaves`, `mwm_installed`, ...), `MwmMapDownloader` + Tools section
"Offline maps", sidecars loaded at session open.

Full chain from scratch (`a_download_from_scratch`, `pm clear` first, **no adb push**): catalog ->
mirror discovery -> verified `countries.txt` -> Oestlandet leaf set (9 leaves, 1.23 GB, MWM SHA-1 +
sidecar SHA-256 + fingerprint) + World + WorldCoasts -> activate -> router re-open -> **drape starts
live in the same process**. 122 s on the tablet's Wi-Fi. Foot detour 61.05,11.47 -> 61.08,11.52:
**13867 m without the hook, 16245 m with it** (production planner, 9 sidecars + 3 later).
`test-artifacts/phase6b/logs/wetland-chain.json`.

### 2b. Leaf download while the drape is live

Two causes, both fixed:

- `navi_comaps_init` moved the Platform resource / settings dir and reset the map style on every
  re-init, which breaks the live Framework (lazy APK reads of fonts and symbols). With
  `navi_comaps_set_platform_locked(1)` (set by the drape bootstrap) a re-open only rebuilds the router.
- **A hard crash** (found by the first live test): the CoMaps `Storage` inside the router owns a
  thread-checked downloader, so the router must be destroyed on the thread that created it.
  A re-open from the downloader thread aborted (`CHECK(m_checker.CalledOnOriginalThread())`).
  Every open / re-open now runs on the single `comaps-session` thread. Also: `comaps_session_open`
  dropped the old router *after* building the new one, and the old router's shutdown tore the new one down;
  the old one is now dropped first. New leaves are registered in the drape with `NaviDrape.nativeRegisterMaps`.

`b_live_download_no_restart`: Rogaland leaf downloaded and registered while the drape ran; same process,
drape state READY, writable dir unchanged, route after the re-open OK, text and symbols still render.

### 2c. Missing maps

`navi_comaps_leaf_for_point` (CoMaps `packed_polygons` / country borders) names the leaf of the start,
via and end points; leaves that are not installed return `NEED_MORE_MAPS` with `missing_leaves=...`
before the router can say "not on a routable road". A `NeedMoreMaps` from the router on a long leg names the
leaves along the straight line. `MainActivity` shows `MwmMissingMapsDialog` ("Download", names the leaves,
download then re-plan). Test `c_missing_maps`: Stockholm -> `Sweden_Stockholm`; Berlin / Potsdam ->
`Germany_Berlin, Germany_Brandenburg_North`; Trondheim -> `Norway_South Trondelag`.

### 2d. Speed limits

`routes_builder` exports the posted maxspeed per route segment (`RouteSegment::GetSpeedLimit`, car router,
MWM `maxspeeds` section) -> `speed_limits_json` -> `maxspeed_kmh` / `maxspeed_posted` in the sim samples ->
the existing speed-limit sign and overspeed warning. Three routes (`d_speed_limits`, HUD read at five
positions per route): see the table in section 9.

## 3. Camera and follow behaviour

- **No offset, no fallback.** `state.cameraZoom` is the CoMaps scale level (fractional) end to end; HUD +/-
  and auto-zoom work on it.
- Follow: `NaviDrape.nativeSetFollowMode` (free / north-up / heading-up) driven by `followGps` and the rotation
  mode; a user pan or gesture makes the drape leave follow and Navi sets `followGps = false` from the
  my-position listener. First fix from the start view zooms to level 16 (CoMaps' locate-me rule).
- Navigation camera: `FollowRoute` while a route is planned and the vehicle moves (puck low on the screen,
  perspective when 3D / a tilt preset is on, GPS bearing rotates the map).
- **Tilt presets (35 / 45 / 60 degrees) collapse to "perspective allowed"**: the drape computes the pitch from
  the zoom (about 20 degrees at level 16, up to 53 degrees in navigation at higher zoom). Free browsing is
  flat; the perspective only exists while navigating (as in the CoMaps app).
- **Frame rate while following: 29.9 fps** (SurfaceFlinger per-layer present times; CoMaps caps the
  route-following camera at 30 fps, `frontend_renderer.cpp`). UI thread 58 fps. The simulator now emits
  interpolated fixes (1 Hz, up to 5 Hz at high time scales) on the MWM path; with the old one-fix-per-300 m
  cadence the drape idled between fixes (3 fps).
- Graphics API: CoMaps default (Vulkan, fallback GLES) works on this tablet (Adreno 618, both tested);
  `navi_drape_api` launch extra / `MapHudPrefs` pref select one (test builds).

## 4. Overlay mapping

| Navi overlay | Drape mechanism | Notes |
|---|---|---|
| Route line (on-trail) | `df::Subroute` via `nativeAddSubroute` (route renderer, casing, per-profile style) | CoMaps blue instead of Navi red; profile maps to car / pedestrian (dashed) / bicycle style |
| Turn arrows | Subroute turn distances from the planner's maneuver polyline indices | |
| Off-trail legs (hiking) | second subroute, pedestrian dash | |
| Alternatives | not present in Navi today | would be tracks (grey), no code needed |
| Start / via / end | `RoutingManager` route points (`nativeSetRoutePoints`) with titles | |
| Current position | `NaviLocationBridge` -> drape puck + accuracy circle + follow | **Estimated (amber) and lost (grey) puck colours cannot be expressed**; the drape puck stays blue (proposal: a CoMaps position-arrow colour patch, ~20 lines) |
| Tracks (APRS) | user marks, group `navi-tracks`, label = callsign | **APRS sprites** cannot be added to the symbol atlas at runtime; shown as the CoMaps transport pin |
| Repeater sites | **none, by product rule** | Repeaters are never drawn on the map (no marks, pins, bookmarks, tracks; neither drape nor MapLibre). CAT selection is shown in the CAT sheet and the dry-run log only |
| DATEX | marks (orange, label road number) + tracks for line geometry | |
| Weather symbols | marks (label `city + condition`, colour by condition) | **meteocons cannot be shown**; nearest alternative = CoMaps pin + coloured label. Proper fix: add Navi SVGs to `data/styles/default/*/symbols` and regenerate the atlas (script exists, see below) |
| Camping suggestions, break stops, long-press POI | marks | |
| GeoJSON layers | none exist on the live map (GeoJSON in Navi is import / export only) | tracks would carry them |
| Contours / 3D buildings | drape isolines / buildings from the leaf MWMs | Mapterhorn hillshade + terrain are not used on the MWM path |

The CoMaps symbol atlases (`symbols.png` / `symbols.sdf`) are **not in the pinned checkout** (they are
build outputs of the Qt `skin_generator`); without them the drape draws no icons, route pins or
marks. `scripts/generate-comaps-symbols.py` produces them with `rsvg-convert` + Pillow (no Qt6 dev
packages); `comaps-sdk/build.gradle.kts` runs it when they are missing. F-Droid recipe: needs `librsvg2-bin`
and `python3-pil` (or ship the generated files).

## 5. MapLibre code that remains only for flag OFF

`org.maplibre.gl:android-sdk` (about 4.5 MB `libmaplibre.so` compressed plus its dex share),
`CorridorMapView` / `applyRouteToStyle` / `applyTracksToStyle` / `ensureRouteAboveHillshade` in
`MainActivity.kt`, `Basemap*.kt` (11 files, style resolver, label / path / tunnel / peak / glacier
styles), `MapterhornTerrain`, `MapterhornContours`, `ContourGenerator` / `ContourGenerationCache`,
`NamedBuildingLabels`, `WeatherMapSymbols` (MapLibre layers), `DatexOverlay`, `Fu49MapDiag`,
`OfflinePmtilesBootstrap`, `WorldOverviewDownload`, `LocalDemTileServer`, `DemTileFetcher`,
`PmtilesArchiveGate`, `PmtilesLayerProbe`, the Tools region / PMTiles / DEM section, and the matching
`NaviMapTestHooks` fields. 20 Kotlin files import `org.maplibre`.

## 6. Vehicle profiles, day / night

- **Vehicle profiles** (electric car, truck, mobile home, motorcycle): routed as the CoMaps car profile;
  the route options show "Vehicle limits (height, weight, axle load) are not applied on this map type".
  A truck model would need: a truck vehicle model in CoMaps `libs/routing_common` (max weight / height /
  width / axle limits from `maxweight` / `maxheight` / `maxwidth` / `hgv` features, road-class
  restrictions), the matching routing sections in the MWM generator (a navi-server data change, the
  leaves are produced there), a UI path that hands the limits to the shim, and a parity suite. Not started.
- **Day / night**: follows the system setting by default; optional "automatic by sunrise / sunset at the
  current position" in the map settings sheet (CoMaps `IsDayTime`-style calculation, applied to the
  drape). Device check at Hamar on 16 November (sunrise 08:19, sunset 15:42 local): at 04:06 and 08:11
  `night=true`, at 08:46 `night=false`; system dark mode -> night, system light -> day.

## 7. Function matrix (`h_function_matrix`, Espa as the GPS fix)

Every route that starts at Espa starts from a GPS fix: the harness uses Navi's own debug GPS hold
(`NaviMapTestHooks.holdDebugGps`, the in-app "fake GPS" mechanism) so the app sees Espa as the device
position, then plans from that fix and plays Navi's own route simulation. No third-party mock-location app.
`plan()` / `planWith()` fail the test when a route from Espa is requested without a held fix.

**24 rows: 19 PASS, 4 PARTIAL, 1 NOT_TESTED, 0 FAIL.**

| # | Id | Result | Detail |
|---|---|---|---|
| 1 | `profile_CAR` | PASS | routed as CoMaps CAR, 43.9 km / 30 min |
| 2 | `profile_CAR_ELECTRIC` | PARTIAL | routed as CoMaps CAR (43.9 km / 30 min); vehicle dimension / weight limits are not applied by the CoMaps shim |
| 3 | `profile_TRUCK` | PARTIAL | routed as CoMaps CAR (43.9 km / 30 min); vehicle dimension / weight limits are not applied by the CoMaps shim |
| 4 | `profile_MOBILE_HOME` | PARTIAL | routed as CoMaps CAR (43.9 km / 30 min); vehicle dimension / weight limits are not applied by the CoMaps shim |
| 5 | `profile_MOTORCYCLE` | PARTIAL | routed as CoMaps CAR (43.9 km / 30 min); vehicle dimension / weight limits are not applied by the CoMaps shim |
| 6 | `profile_BICYCLE` | PASS | routed as CoMaps BICYCLE, 33.9 km / 125 min |
| 7 | `profile_BICYCLE_ELECTRIC` | PASS | routed as CoMaps BICYCLE, 33.9 km / 125 min |
| 8 | `profile_HIKING` | PASS | routed as CoMaps PEDESTRIAN, 33.4 km / 420 min |
| 9 | `avoid_motorways` | PASS | 32.6 km / 38 min vs default 43.9 km / 30 min (PREFER_NON_MOTORWAY optimise mode; options persisted to CoMaps settings) |
| 10 | `avoid_tolls` | PASS | 32.6 km / 38 min vs default 43.9 km / 30 min (no toll road on this corridor; flag accepted) |
| 11 | `avoid_ferries` | PASS | default 13.2 km / 50 min vs avoid 128.8 km / 103 min |
| 12 | `vias` | PASS | 255.0 km / 204 min (direct 252.3 km / 198 min), closest approach to via 0 m |
| 13 | `tbt` | PASS | 12 maneuvers, 12 with nb+en text, 12 with nb voice text; kinds=[u_turn, exit_right, roundabout, destination] |
| 14 | `eta` | PASS | 43.9 km / 30 min => 87 km/h average (CoMaps time estimate) |
| 15 | `speed_limits` | PASS | 828 of 835 samples carry a posted limit, distinct=[30, 40, 50, 60, 70, 80, 110] (MWM maxspeeds section via the shim; HUD + overspeed use them) |
| 16 | `elevation` | PASS | ascent 350 m / descent 381 m, 835 elevation points, sources=[comaps, sidecar], sidecar_hits=26 (climb is published to the HUD via RouteClimbSource,... |
| 17 | `wetland` | PASS | foot detour 13867 m -> 16245 m with 12 sidecars (production planner, from the in-app download) |
| 18 | `off_road_choice` | PASS | router refused (no automatic move); suggestion found=true snap=61.909746947562574,9.803558512058535 dist=5099 m, offered only as a user choice (Sna... |
| 19 | `missing_maps` | PASS | both points uncovered -> NEED_MORE_MAPS 'FAIL: Missing maps: Germany Berlin, Germany Brandenburg North'; start covered, end uncovered -> NEED_MORE_... |
| 20 | `cancel` | PASS | in-flight plan returned CANCELLED 968 ms after cancel (native router is not interruptible; result discarded at the leg boundary); the next plan rou... |
| 21 | `gpx_export` | PASS | GPX 1.1 with <rte> and 835 <trkpt> |
| 22 | `gpx_import` | NOT_TESTED | no GPX import entry point exists in the app or FFI (export only); not changed by Phase 6 |
| 23 | `datex` | PASS | data_source=server-duckdns, 2 active situations on the route (2 with geometry, drawn as drape marks); content correctness is the server's |
| 24 | `airplane_mode` | PASS | no internet (online=false); route 43.9 km / 30 min, search 30 hits; DATEX offline -> '{"active":[],"active_count":0,"attribution":null,"data_source... |

Documented decisions for the non-PASS rows:

- The four PARTIAL profile rows are the vehicle-limits note of section 6 (the route is correct for a
  car; limits are not applied and the UI says so).
- `gpx_import`: the app and the FFI have no GPX import entry point (export only). Not a Phase 6
  regression; reported, not added.

## 8. Navigation views and screenshots

Camera values from `logs/nav-views.json` (Espa GPS fix, Navi's own simulation, 1x, Samsung SM-P613):

| View | Mode | Level | Bearing | Pitch |
|---|---|---|---|---|
| north-up follow | Follow | 16 | 0 | 0 |
| heading-up follow | FollowAndRotate | 16 | follows the road | 0 |
| heading-up + tilt + 3D | FollowAndRotate | 16 | follows the road | 20 degrees |
| compass rotation mode | FollowAndRotate | 16 | follows the road | 20 degrees |

During a navigated drive (CAT / perf runs, navigation camera) the pitch is 37 degrees at level 17.

Screenshot index (`test-artifacts/phase6b/screenshots/`, gitignored; repeater map pins are intentionally
absent, product rule):

| Group | Files |
|---|---|
| a: clean install | `a_no_maps_placeholder`, `a_after_download_live_drape` |
| b: live leaf download | `b_before_live_download_hamar`, `b_after_live_download_hamar`, `b_after_live_download_stavanger` |
| c: land cover day / night | `c_land_{mjosa_farmland_forest,lillehammer_town,gjendebu_alpine}_{day,night}` |
| c: contours z12 / 14 / 16, on / off | `c_contours_z{12,14,16}_{on,off}` |
| c: 3D buildings | `c_3d_on_z17`, `c_3d_off_z17` |
| c: labels | `c_label_{e6_lillehammer,rv_dombas,al_aa,favang_aa,gjovik_oe,honefoss_oe}` |
| d: speed limit HUD | `d_speed_limit_hud_{e6,rv3,town}` |
| r: route + overlays, nav views | `r_route_overview_overlays`, `r_overlay_datex_z11`, `r_overlay_datex_z14`, `r_follow_north_up`, `r_follow_heading_up`, `r_nav_tilted_heading_up_3d`, `r_follow_compass_mode` |
| t: configuration changes | `t_baseline`, `t_landscape`, `t_after_recreate`, `t_dark_mode`, `t_light_mode`, `t_auto_sun`, `t_freeform_half_width`, `t_split_screen_single_app`, `t_vulkan_engine`, `t_gles_engine` |
| navigation runs | `espa_dombas_auto_*`, `espa_dombas_standalone_*`, `espa_litjfossen_auto_*`, `perf_nav_20min_*` |

Activity recreation and configuration changes (`recreation.json`, one process, drape READY at every step):
portrait -> landscape -> portrait, explicit `recreate()`, system dark mode and back, automatic sun theme,
freeform half width. Vulkan (default) and GLES engines both start and render on the Adreno 618.
Split screen with a second app was not run: the rules allow Navi only on the device, so only the
freeform half-width window of Navi itself was tested.

## 9. Speed limits (`d_speed_limits`)

| Route | Length | Samples with a posted limit | Distinct limits (km/h) | HUD read at 10 / 30 / 50 / 70 / 90 % |
|---|---|---|---|---|
| E6 Espa -> Hamar | 43.9 km | 99.2 % | 30 40 50 60 70 80 110 | 110 110 110 110 110 |
| Rv3 Elverum -> Rena | 34.0 km | 98.4 % | 30 50 60 70 80 90 | 60 (overspeed at 65.9 km/h) 90 80 80 80 |
| Town street, Lillehammer | 0.3 km | 21.4 % | 30 | 30 50 50 30 30 (HUD holds the last limit where the street has no tag) |

The town route is only 0.3 km because that is the place pair the harness resolved; a longer 30 / 50
route is a better test and is listed as a follow-up.

## 10. CAT dry runs (no Bluetooth, no radio)

Setup: Navi's own simulation with Espa as the GPS fix, in-process Benshi lab rig, repeater DB from the
server mirror (12,435 sites). Logs: `logcat-{n,p,o}_*.txt` (`cat_skip`, `VFO1 program`, `follow switch`).

| Run | Tuned sequence | Fixture |
|---|---|---|
| Espa -> Dombas, `auto` (12x) | LA5MR (hold) -> LA6JRR (145.6625, -0.6, CTCSS 88.5) | Run B: LA5MR -> LA6JRR (match; the earlier LA2TRR hop is gone) |
| Espa -> Dombas, `standalone` (20x) | LA6GR -> LA5ARR -> LA7GR -> LA2JRR | Run A: LA6GR, LA5ARR, LA7GR, LA2JRR (exact match, no DMR rows) |
| Espa -> Litjfossen, `auto` (20x) | LA5TRR (145.725, -0.6) -> LA9AR (145.600, -0.6) | LA5MR network corridor (LA9AR Tronfjell last, leave-network after it) |

After arrival the simulation loops, which is why a further switch to the start-area site appears at the end
of each log; it is not part of the drive.

Skips with log lines (`cat_skip ...`), identical in all runs:

- `digital_only`: 28 callsigns (29 in the Litjfossen run) with DMR / D-STAR modulation, e.g. LA2LRR, LD2HJ,
  LA2L, LA2DRR, LA7RRA-B. None was programmed.
- `invalid_shift` (band rules): LA2TRR 145.2375 MHz +287.9375, LA7XR 144.6625 +288.3875, LA6LR 144.65
  +288.375 (offset above 10 MHz) and LA2FRR 1297 MHz -6 (outside the rig's bands). None was programmed;
  the rejected +287.9375 MHz row is the one the first dry run had programmed.

Follow timers (30 s dwell, 60 s gap) now run on simulation time (`navi-cat::clock`, scale set from the
route simulator), so 12x and 20x runs switch where a 1x drive would. Two further follow defects were found
by the runs and fixed in the plugin guest (`plugins/CATS-plugin/src/follow.rs`, with unit tests):

1. Persisted follow state outlived the process while the follow clock did not. A simulated drive stored
   timestamps hours ahead of real time; the next process saw `now < site_since`, so dwell / gap never
   expired and the follow stayed on the old site for the whole run. Timestamps in the future are clamped.
2. A held site that is no longer in the followed set (standalone -> network, member out of range) kept a
   stale distance, so hysteresis blocked every switch (Litjfossen run held the standalone LA8ER for the
   whole drive). It is now dropped and the nearest member is tuned at once (not when a pin is set).

Observations, not fixed (CAT scope):

- The first follow tick can run before the lab rig is connected and logs `VFO1 program FAIL: backend not
  gated: no radio connected`. The guest has already recorded the switch, so the site is not retried when the
  rig connects (the Dombas `auto` run never programmed LA5MR for that reason). With a real rig the radio keeps
  its last programming; with a fresh rig a failed program should not commit the follow state.

Text note for navi-server:

> The MWM client now skips repeater rows that cannot be tuned on an analog FM rig: digital-only modulations
> (DMR, D-STAR, C4FM) and rows with an invalid shift. Four OSM-derived rows in the current mirror carry
> absurd shifts and are skipped: LA2TRR (osm-LA2TRR, 145.2375 MHz, +287.9375), LA7XR (144.6625, +288.3875),
> LA6LR (144.65, +288.375) and LA2FRR (1297 MHz, -6, outside 2 m / 70 cm). Please fix `repeater:shift` or
> `repeater:input` upstream (or drop the rows in the mirror). Leaves are unchanged; the MWM sidecars do not
> carry repeaters.

## 11. Performance (Samsung SM-P613, Android 14, releasetest, USB powered)

| Run | Duration | UI fps | UI janky frames / 15 s | Drape fps | PSS (MB) | Native heap (MB) | Battery temp | Thermal |
|---|---|---|---|---|---|---|---|---|
| 20 min, 1x, heading-up, tilt, 3D (`s_perf_20min`) | 19.8 min | 59.8 (59.3-60.1) | 3.7 (0-12) | **29.9** (29.7-30.0) | 616 (612-624) | 435 | 30.3-30.6 C | status 0 throughout |
| Espa -> Dombas, 12x, north-up, CAT on (`n_`) | 16.9 min | 59.6 (57.7-60.1) | 7.0 (0-34) | 60.0 (57.8-60.1) | 620 (611-675) | 439 | 30.0-30.5 C | status 0 |

Before the fix described next, the same 12x / 20x runs measured UI 48-50 fps with 80-95 janky frames per
sample and a drape that did not show the route for the first minutes: `MapState.layerEpoch` moves with
every GPS fix, and the route and mark effects of `ComapsDrapeMap` were keyed on it, so the route was removed
and re-added at fix rate (up to 5 Hz in the accelerated simulation). Those effects no longer depend on it.

Frame rate while following: **29.9 fps** with the navigation camera (the CoMaps route-following camera caps
at 30 fps, `frontend_renderer.cpp`); north-up follow without a tilt renders at **60 fps** (the cap does not
apply there). UI thread stays at 58-60 fps in both. Phase 6a for comparison: UI 52-54 fps, drape about
11 fps, PSS 650 MB (MapLibre + drape).

20 minutes of navigation did not move the thermal state (battery 100 %, plugged in, 30.3 to 30.6 C).

## 12. APK size

`:app:assembleReleasetest -PnaviAbis=arm64-v8a`: **94.3 MB**. Phase 6a (both ABIs, SDK assets duplicated
with `comaps-data`, unstripped native staging): 171.2 MB with the SDK, 134.1 MB without it. The two
numbers are not like for like (ABI set and strip state differ), so the saving per item is listed instead.

| Entry (compressed) | MB | Note |
|---|---|---|
| assets/symbols | 16.1 | new in 6b: CoMaps symbol atlases, 6 densities x 3 styles (generated, see section 4) |
| assets other (drules, countries, ICU, plugins) | 14.3 | single copy now (SDK root assets; the headless router stages from them instead of a second `comaps-data` copy) |
| classes.dex | 11.8 | |
| assets/fonts | 10.6 | |
| libnavi.so | 10.0 | |
| assets/countries-strings | 7.0 | |
| liborganicmaps.so | 6.9 | stripped (16 MB raw) |
| res | 6.1 | |
| assets/icons | 4.7 | |
| libmaplibre.so | 4.5 | still shipped: the flag-OFF UI is in the same binary |
| libhamlib.so + libc++ | 1.8 | |

Further reductions, not done (product decisions): ship only the xhdpi / xxhdpi symbol sets (about 10 MB),
drop unused style variants, build a drape-only flavor without MapLibre (4.5 MB native plus its dex share)
when the flag-OFF UI is retired in Phase 8, and trim `countries-strings` to the shipped locales (7 MB).

## 13. CI status at the end of Phase 6b

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy` (ci-manual command, workspace and `navi-ffi --features diag-bins`, `-D warnings`) | pass (three lint failures that already existed on the branch in `core/src/mwm/sidecar.rs` and two `core/tests/mwm_*_live.rs` were fixed) |
| `cargo test -p driver-break-core --lib` (ci.yml, flag on and `NAVI_MWM_CLIENT_ENABLED=0`) | pass, 819 each (14 ignored) |
| `cargo test -p navi-ffi --lib`, `-p navi-cat`, `-p navi-plugin-cats`, `-p navi-plugin-host --test isolation` | pass (21, 80, 11 lib + 22 integration, 1+) |
| `cargo test --workspace` with the ci-manual excludes | not run to completion: the integration test binaries take over an hour here; the ci.yml jobs above are the automatic gate |
| `ktlintCheck`, `detekt`, `testDebugUnitTest` (also with `-Pnavi.comapsDrape=false`) | pass |
| `scripts/check-plugin-host-gate.sh` | fails on a leftover untracked directory `tools/fdroid-check/work-vc32/` (gitignored F-Droid check work tree that vendors a second plugin-host with wasmtime 50); not a branch change |
| `cargo deny check` | **advisories FAILED** (bans, licenses, sources ok); not changed on this branch, same on `dev` |

`cargo deny` advisories (crate -> needed): `libsqlite3-sys 0.38.2` (RUSTSEC-2026-0459) -> >= 0.39.0 via
`rusqlite`; `rustls 0.23.45` (0346 to 0349) -> >= 0.23.46; `rustls-webpki 0.103.15` (0382) -> >= 0.103.16;
`tokio 1.53.1` (0386) -> >= 1.53.3; `wasmtime 48.0.5` (0353-0359, 0361-0364, 0378, 0379, 0422-0424) -> a
patched 48.x / newer release; `webpki-roots 1.0.9` (0381); `zip 8.6.0` (0367-0369, 0429); `zstd 0.13.3`
(0391) and `zstd-safe 7.3.0` (0398). `cargo update -p <crate>` fixes the first four; wasmtime / zip / zstd
need the upstream bump or an explicit ignore with a reason in `deny.toml`.

## 14. Other defects found and fixed on the way

- **Off-route false positives**: `RouteProgressTracker` measured the distance to the nearest route
  *sample point*; MWM routes have samples hundreds of metres apart on a straight road, and the simulation
  (now emitting interpolated fixes) or a real GPS fix between two samples read as off-route, which started
  automatic reroutes. It now uses the distance to the polyline segment (unit test added).
- **Reroute on the MWM path** called the legacy region planner and failed with "No region PBF available for
  replan". It now uses `MwmProductionPlanner`.
- Route line and marks re-applied at GPS fix rate (section 11).
- The repeater overlay that an earlier 6b step drew on the map was removed (product rule: repeaters are
  never map markers).

## 15. Decisions for the owner

1. **Vehicle limits**: truck / mobile home / motorcycle routes ignore limits (labelled in the UI). A truck model
   is a CoMaps model + MWM generator + navi-server change; decide whether it is in scope before Phase 8.
2. **Position puck colours** (estimated / lost) and **APRS / weather sprites** cannot be expressed on the
   drape without a small CoMaps patch (puck colour, about 20 lines) and adding Navi SVGs to the symbol atlas.
3. **North-up follow frame cap**: 60 fps outside the navigation camera. Cap at 30 fps to save battery, or keep.
4. **MapLibre in the APK** until the flag-OFF UI is removed; a drape-only flavor would save about 4.5 MB plus dex.
5. **GPX import** does not exist; add or drop from the matrix.
6. **F-Droid**: the symbol atlas needs `rsvg-convert` and Pillow at build time (or commit the generated files).
7. **cargo deny** advisories and the plugin-host-gate workspace check need separate fixes (section 13).
8. A failed VFO program should not commit the follow state (CAT observation in section 10).
