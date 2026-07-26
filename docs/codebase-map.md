# Codebase map (contributor orientation)

How to find the right file when fixing a bug or changing behaviour (zoom,
approach box, routing, rest, map styles, …). Pair with
[`architecture.md`](architecture.md) (crate wiring / databases),
[`rust-crates.md`](rust-crates.md) (created vs unaltered crates.io deps),
[`API.md`](API.md) (callable surfaces), and [`debugging.md`](debugging.md)
(logcat / tests).

Kotlin UniFFI names are **camelCase** (`planCarRoute`); Rust exports are
**snake_case** (`plan_car_route`). Generated bindings live in
`app/src/main/java/uniffi/navi/navi.kt` — regenerate via
[`android-build.md`](android-build.md); do not hand-edit that file.

---

## Top-level layout

| Path | Role |
|---|---|
| `core/` | Trusted Rust (`driver-break-core`): graph, eco, POI, search, rest/safety, tracks, icons, SQLite |
| `navi-ffi/` | UniFFI CDYLIB: thin exports Kotlin/Android (and other hosts) call |
| `app/` | Kotlin / Compose Android Automotive UI + MapLibre |
| `plugin-host/` / `plugin-sdk/` / `plugins/` | Sandboxed WASM host + example guests (no product plugins yet) |
| `navi-linux/` | Linux host sketch (gpsd / IMU; no full map UI) |
| `docs/` | Specs and how-tos (this file included) |
| `scripts/` | Emulator / build helpers |

Default Cargo workspace members: `core`, `plugin-host` (see root `Cargo.toml`).

---

## Android UI files (`app/src/main/java/no/navi/app/`)

| File | Owns |
|---|---|
| `MainActivity.kt` | App shell: MapLibre camera, GPS / sim `applyFix`, route planning calls, HUD state, settings sheets wiring |
| `DriveHud.kt` | Top / bottom HUD bars, map settings sheet (auto-zoom, 3D, tilt), drive settings sheet |
| `MapHudPrefs.kt` | SharedPreferences for HUD: default zoom, tilt presets, 3D opt-in, metric, Geofabrik path |
| `ApproachInstructionBox.kt` | Next-turn approach chrome (icon, distance, street / house / postcode layout) |
| `RouteGuidanceModels.kt` | Kotlin models for maneuvers, samples, approach display helpers |
| `RouteProgressTracker.kt` | Snap position → along-route progress, distance-to-maneuver, ETA, break elapsed hours |
| `RouteSimulator.kt` | Debug-only route drive-along at posted / fallback speeds |
| `BasemapStyleResolver.kt` | Online Liberty vs offline PMTiles style choice |
| `MapterhornTerrain.kt` | Optional DEM hillshade / 3D style stacking (Vulkan-gated) |
| `MultiDayPlanCards.kt` | Day-card UI from `days_json` |
| `CoordinateInput.kt` | Lat/lon entry helpers |
| `NaviAppData.kt` | Data-dir paths under the app sandbox |
| `NaviMapTestHooks.kt` | Instrumentation hooks (camera, sim, banner) — debug / tests |
| `RoutingPlanLog.kt` | Plan logging helpers |

Instrumented tests: `app/src/androidTest/java/no/navi/app/`.

---

## Rust core modules (`core/src/`)

| Module | Role | Typical bugs / changes |
|---|---|---|
| `routing/` | OSM graph, eco reweight, A*, workers, OSM updates, elevation, PMTiles jobs | Wrong route, missing roads, eco cost, cache invalidation |
| `routing/graph/` | Build / cache / network preference / road-near | Access tags, hiking network penalty |
| `routing/rest/` | Car / truck / hiking multi-day, HOS packs | Break spacing, EC 561 / FMCSA |
| `routing/guidance_path.rs` | Densified sim samples + maneuvers JSON | Sim pace, maneuver placement |
| `routing/eta.rs` | Hiking / cycling pace constants | Pre-departure ETA |
| `nav/` | Approach thresholds + `NavGuidance` / `ManeuverKind` | When the turn box appears / urgency / hides |
| `config/` | Profiles, rest/fuel/safety/vehicle defaults | Default break hours, POI radii |
| `poi/` | Categories, R-tree, OSM tag rules | Missing POI type — see [`poi.md`](poi.md) |
| `search/` | FTS5 place index | Search miss / index rebuild |
| `storage/` | SQLite schema / migrations | Settings not persisting |
| `tracks/` | Moving-station store (APRS-style) | Timeout / range filter |
| `icons/` | SVG → PNG raster | Wrong maneuver / POI glyph |
| `ecu/` | Live energy types (no live UniFFI poll yet) | See [`ECU.md`](ECU.md) |
| `download/` | Shared pause / resume / cancel progress | Provision / PMTiles job control |
| `sensors/` | Host-side sensor helpers (Linux path) | gpsd / IMU |
| `bus/` | `WorldSnapshot` (position + profile + energy) | Plugin / future live energy |

---

## “I want to change …”

### Map zoom (defaults and auto-zoom)

**Today:** auto-zoom is a **single user-chosen level** (default **16.5**), applied when
the auto-zoom toggle is on and the camera should snap while moving. It does
**not** yet vary by speed or distance to the next turn.

| What | Where |
|---|---|
| Default level, min/max clamp | `MapHudPrefs.DEFAULT_AUTO_ZOOM_LEVEL` / `MIN_ZOOM` / `MAX_ZOOM` in `MapHudPrefs.kt` |
| Toggle + −/+ UI | `MapSettingsSheet` in `DriveHud.kt` |
| Persist / load | `MapHudPrefs.saveAutoZoom` / `loadAutoZoomLevel` / `loadAutoZoomOn` |
| Apply to MapLibre camera | `MainActivity.kt` (handlers for `onToggleAutoZoom` / `onAutoZoomLevelChange`, and camera `LaunchedEffect` / `CameraOptions.Builder().zoom(...)`) |
| Manual zoom −/+ | Bottom HUD in `DriveHud.kt` → `onZoomIn` / `onZoomOut` in `MainActivity.kt` |
| Idle / overview fallbacks | `MainActivity.kt` (hard-coded fallbacks such as `12.0` / `6.5` when `cameraZoom` is null — search `cameraZoom`) |

**Speed- or turn-proximity adaptive zoom (not implemented):** hook where live
fixes update the camera in `MainActivity.kt` (GPS / `RouteSimulator` →
`applyFix` / progress). Inputs already available:

- Instantaneous or sim speed: sim sample `speedKmh` / GPS speed in the fix path
- Distance to next turn: `RouteProgressTracker` → `RouteProgressSnapshot.distanceToManeuverM`
- Approach phase constants (Rust): `APPROACH_*_M` in `core/src/nav/mod.rs` (also
  UniFFI `approachAppearM` / `approachUrgencyM` / `approachHideM`)

Suggested shape: a small pure function
`desiredZoom(speedKmh, distanceToManeuverM, prefs) → Double` used only when
`autoZoomWhileMoving` is true, still clamped by `MapHudPrefs.clampZoom`. Keep
user −/+ overrides from fighting the loop (debounce or “user overridden until
next maneuver”).

### Approach / next-turn box

| What | Where |
|---|---|
| Appear / urgency / hide meters | `core/src/nav/mod.rs` (`APPROACH_APPEAR_M` 750, `APPROACH_URGENCY_M` 150, `APPROACH_HIDE_M` 25) |
| UniFFI thresholds | `approachAppearM` / `approachUrgencyM` / `approachHideM` |
| Phase string for UI | `approachPhaseForDistance` → `ApproachInstructionBox` |
| Maneuver cursor advance | `RouteProgressTracker.hideDistanceM` (default `approachHideM()`) |
| Phase logic | `NavGuidance::phase` |
| Product copy / layout rules | [`approach-instructions.md`](approach-instructions.md) |
| Compose UI | `ApproachInstructionBox.kt` |
| Live distance + street | `RouteProgressTracker` + wiring in `MainActivity.kt` |
| Maneuver list from planner | `CorridorRouteResult.maneuvers_json` ← `routing/guidance_path.rs` |

### Current street (bottom bar)

| What | Where |
|---|---|
| Product rules / no-route policy | [`current-street.md`](current-street.md) |
| Sample `street` + highway | `guidance_path::build_sim_samples` |
| Class fallback labels | `eta::highway_class_display_label` (aligned with `highway_fallback_kmh`) |
| Idle GPS nearest edge | `graph/road_near.rs` (`nearest_road_label`) + UniFFI `road_label_near` |
| Place-index interim | UniFFI `nearby_places` + `streetLabelFromNearbyPlaces` |
| Bottom HUD line | `BottomDriveHud` / `DriveHudState.currentStreet` |
| Unicode pipeline notes | [`unicode-road-names.md`](unicode-road-names.md) |

### Break countdown / trip ETA

| What | Where |
|---|---|
| Live break “minutes remaining” | `MainActivity.kt` + `RouteProgressSnapshot.elapsedDrivingHours` (integrate planned segment times — do **not** use `along_m / instantaneous_speed` alone) |
| Car / truck / hiking defaults | `core/src/config/defaults.rs` |
| Truck HOS packs | `core/src/routing/rest/` + [`ec-561-truck-rest.md`](ec-561-truck-rest.md) / [`fmcsa-truck-rest.md`](fmcsa-truck-rest.md) |
| HUD break / ETA chrome | `DriveHud.kt` |
| Pref: show break as distance | `MapHudPrefs.BREAK_DISPLAY_SPEED_KMH` + `loadBreakAsDistance` |

### Routing / planning

| What | Where |
|---|---|
| Motor corridor plan | UniFFI `plan_car_route` → `navi-ffi` → core graph A* |
| Hiking multi-waypoint | UniFFI `plan_hiking_route` (rejects using `plan_car_route` with Hiking) |
| Fixture corridor smoke | `run_car_corridor_pipeline` |
| Graph build / eco | `core/src/routing/graph/` |
| Official hiking/cycle networks | `network_pref.rs` + `prefer_official_networks` flag |
| Kotlin call sites | `MainActivity.kt` (search `planCarRoute` / `planHikingRoute`) |

Hiking foot routes require **Hiking** travel mode in drive settings; other
profiles use the road graph.

### Map style, 3D, tilt

| What | Where |
|---|---|
| Liberty vs PMTiles | `BasemapStyleResolver.kt`, [`map-styles.md`](map-styles.md) |
| 3D hillshade | `MapterhornTerrain.kt`, `MapHudPrefs` opt-in 3D |
| Camera tilt presets | `MapHudPrefs.CAMERA_TILT_PRESETS` (0 / 35 / 45 / 65) |
| Route line above hillshade | `MainActivity` / terrain helpers (`ensureRouteAboveHillshade`-style re-apply after style reload) |

### Search / POI / icons

| What | Where |
|---|---|
| FTS search API | UniFFI `ensure_place_index` / `search_places` |
| Categories / tags | `core/src/poi/`, [`poi.md`](poi.md) |
| Icon raster | UniFFI `rasterize_icon_png`, `core/src/icons/` |

### Simulation (debug builds)

| What | Where |
|---|---|
| Behaviour | [`route-simulation.md`](route-simulation.md) |
| Engine | `RouteSimulator.kt` + `sim_samples_json` from FFI |
| Banner / hooks | `MainActivity.kt`, `NaviMapTestHooks.kt` |

### Plugins

| What | Where |
|---|---|
| Host + capabilities | `plugin-host/src/abi.rs`, [`plugins.md`](plugins.md) |
| Guest helpers | `plugin-sdk/` |
| Specs (not built) | `docs/plugins/*.md` |

---

## Data on device (quick)

Under the app data directory (see `NaviAppData.kt`):

| Artifact | Typical use |
|---|---|
| Region `.osm.pbf` + graph cache | Routing |
| Elevation / DEM tiles | Eco costing + altitude HUD |
| Place FTS DB | Search |
| SQLite `app_config` / routes DB | Rest, fuel, vehicle limits, saved routes |
| `{dataDir}/pmtiles/*.pmtiles` | Offline basemap / terrain DEM |
| SharedPreferences `navi_map_hud` | Zoom, tilt, 3D, metric (not UniFFI) |

---

## Tests to run after a change

| Change area | Start here |
|---|---|
| Core routing / rest | `cargo test -p driver-break-core …` (see [`debugging.md`](debugging.md)) |
| UniFFI surface | `cargo test -p navi-ffi` + rebuild `libnavi.so` |
| HUD / zoom / approach / sim | Matching `*InstrumentedTest` under `app/src/androidTest/` |
| Map screenshots | Allowlisted images only — [`pictures.md`](pictures.md) |

---

## Related docs

- Architecture (threads, DB roles): [`architecture.md`](architecture.md)
- Callable APIs: [`API.md`](API.md)
- Wire / protocol index: [`PROTOCOLS.md`](PROTOCOLS.md)
- HUD geometry: [`hud-layout.md`](hud-layout.md)
- Approach product rules: [`approach-instructions.md`](approach-instructions.md)
