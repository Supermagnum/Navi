# API reference

Callable surfaces of Navi: **UniFFI** (Android / host), **plugin HostApi**
(WASM guests), and notes on what is **not** an API (Compose UI prefs, MapLibre).

Source of truth for UniFFI signatures: `navi-ffi/src/lib.rs`. Generated Kotlin:
`app/src/main/java/uniffi/navi/navi.kt` (regenerate — do not edit by hand).
Kotlin names are camelCase (`planCarRoute`); Rust is snake_case
(`plan_car_route`).

Contributor file map: [`codebase-map.md`](codebase-map.md).  
Crate wiring / databases: [`architecture.md`](architecture.md).  
Wire protocol index: [`PROTOCOLS.md`](PROTOCOLS.md).

---

## 1. UniFFI host API (`navi-ffi`)

Package / module: `uniffi.navi` on Android.

### 1.1 Logging, workers, smoke

| Rust | Kotlin | Returns | Purpose |
|---|---|---|---|
| `init_native_logging` | `initNativeLogging` | — | Enable native `log` for downloads / routing |
| `download_progress_snapshot` | `downloadProgressSnapshot` | `FfiDownloadProgress` | Bytes / phase for active download |
| `download_progress_clear` | `downloadProgressClear` | — | Clear progress slot |
| `detected_parallelism` | `detectedParallelism` | `u32` | Detected CPU count |
| `routing_worker_count` | `routingWorkerCount` | `u32` | Rayon workers reserved for routing |
| `ffi_linkage_smoke_test` | `ffiLinkageSmokeTest` | `String` | Linkage / pool smoke string |

### 1.2 Region provision and corridor planning

| Rust | Kotlin | Purpose |
|---|---|---|
| `provision_region_data(data_dir, pbf_url, pbf_filename, elevation_tar_url?)` | `provisionRegionData` | Download/parse regional PBF (+ optional DEM tar); returns report string |
| `run_car_corridor_pipeline(pbf, elev, cache, break_interval_hours)` | `runCarCorridorPipeline` | Fixture / smoke corridor → `CorridorRouteResult` |
| `run_car_corridor_smoke_test(...)` | `runCarCorridorSmokeTest` | Same pipeline; returns `report` only |
| `plan_car_route(pbf, elev, cache, start_lat/lon, end_lat/lon, use_eco, profile, avoid_*, vehicle, prefer_official_networks)` | `planCarRoute` | Motor / cycle corridor plan. **Rejects** `TravelProfile::Hiking` |
| `plan_hiking_route(pbf, elev, cache, waypoints_json, prefer_official_networks)` | `planHikingRoute` | Multi-waypoint hiking; `waypoints_json` is `[{"name","lat","lon"}, …]` (≥ 2). Promotes near-corridor named rast huts to vias when the detour fits Drive **POI cabin radius**, then replans once |

**`CorridorRouteResult` (record)** — shared plan output:

| Field | Meaning |
|---|---|
| `report` | Human / test report (often includes `PASS` / `FAIL`) |
| `distance_km` | Path length |
| `eta_minutes` | Pre-departure duration estimate |
| `cache_hit` / `cold_build_s` / `warm_load_s` | Graph cache timing |
| `route_polyline` | `"lon,lat;lon,lat;…"` for MapLibre |
| `poi_*` | Primary break / landmark pin |
| `break_pois_json` | Pause / overnight pins JSON array |
| `days_json` | Multi-day day cards JSON (`[]` if single-day) |
| `sim_samples_json` | Densified samples for debug simulation |
| `maneuvers_json` | Turn / destination maneuvers along the path |

Detail: [`route-simulation.md`](route-simulation.md), rest docs under Documents in
the README.

**`TravelProfile` (enum):** `Car`, `CarElectric`, `Truck`, `TruckElectric`,
`MobileHome`, `Bicycle`, `BicycleElectric`, `Hiking`, `Motorcycle`,
`MotorcycleElectric`.

Helpers:

| Rust | Purpose |
|---|---|
| `travel_profile_menu_focus` | Whether profile is a primary menu focus |
| `eco_mode_toggleable` | User may toggle eco |
| `eco_mode_default` | Default eco on/off for profile |

**`FfiVehicleLimits`:** height / width / length / weight / hazmat-style fields
used by truck / restriction costing (see record in `navi-ffi`).

### 1.3 Search and icons

| Rust | Purpose |
|---|---|
| `ensure_place_index(pbf_path, index_db_path)` | Build or reuse FTS index; report string |
| `search_places(index_db_path, query, limit)` | → `Vec<PlaceHit>` (`osm_id`, `name`, `kind`, `lat`, `lon`, `sub_area`, `municipality`) |
| `nearby_places(index_db_path, lat, lon, radius_m, limit)` | Place hits near a fix (idle current-street interim) |
| `rasterize_icon_png(key, theme, …)` | PNG bytes for Navit-derived icon key |
| `rasterize_icon_check(key, theme, …)` | Validation / smoke for icon key |

`FfiIconTheme`: `Day` | `Night`. See [`icons.md`](icons.md), [`poi.md`](poi.md).

### 1.4 Saved routes, saved places, and drive config (SQLite via `data_dir`)

| Rust | Purpose |
|---|---|
| `list_saved_routes` / `save_named_route` / `delete_saved_route` | Named corridors |
| `list_saved_places` / `save_named_place` / `rename_saved_place` / `delete_saved_place` | Named single coordinates (`FfiSavedPlace`) — distinct from routes |
| `load_vehicle_limits` / `save_vehicle_limits` | `FfiVehicleLimits` |
| `load_prefer_official_networks` / `save_prefer_official_networks` | Hiking/cycle network preference |
| `load_car_rest_settings` / `save_car_rest_settings` | `FfiCarRestSettings` |
| `load_truck_rest_settings` / `save_truck_rest_settings` | `FfiTruckRestSettings` |
| `set_truck_exceptional_extension_armed` | EC 561 exceptional extension arming |
| `load_fuel_config` / `save_fuel_config` | `FfiFuelConfig` (ICE tank; Car) |
| `load_ebike_config` / `save_ebike_config` | `FfiEbikeConfig` (battery Wh, torque Nm, wheel inches; Electric Cycle) |
| `load_ev_car_config` / `save_ev_car_config` | `FfiEvCarConfig` (battery kWh; Electric Car range check) |
| `load_profile_poi_radii` / `save_profile_poi_radii` | `FfiProfilePoiRadii` per travel profile (search/cabin/network radii + road-link flag; hiking cabin radius also sets auto-via detour) |

Truck / jurisdiction behaviour: [`ec-561-truck-rest.md`](ec-561-truck-rest.md),
[`fmcsa-truck-rest.md`](fmcsa-truck-rest.md),
[`jurisdiction-rules.md`](jurisdiction-rules.md).

How to use map mark + saved places:
[`map-marking-saved-places.md`](map-marking-saved-places.md).

### 1.5 Elevation and GPS slot

| Rust | Purpose |
|---|---|
| `elevation_at(elev_dir, lat, lon)` | Optional DEM height (metres) |
| `update_gps_fix(lat, lon, available, speed_kmh?)` | Push last fix into native slot (optional km/h) |
| `last_gps_fix` | Read `FfiGpsFix` (includes optional `speed_kmh`) |
| `current_speed_kmh` | Live GPS speed from last push, or null |
| `current_speed_limit_kmh(pbf, cache, elev, profile, max_m)` | Sticky nearest-edge limit at last GPS |
| `road_near_info(...)` | Sticky label + applicable `speed_limit_kmh` (+ flags) |
| `resolve_speed_limit_kmh(posted?, conditional?, highway?)` | Conditional → posted → highway fallback |
| `overspeed_delta_kmh(speed?, limit?)` | `speed − limit` when both known (HUD convenience) |

The Android UI primarily drives MapLibre from Kotlin location / simulation; the
GPS slot is for native consumers and tests.

Bottom-HUD overspeed **chrome** is not UniFFI: Kotlin
`OverspeedHud.isOverspeed(speedKmh, limitKmh, speedAccuracyKmh?)` requires a
positive delta above the hybrid margin
`max(limit × 0.05, speedAccuracyKmh, 3.0 km/h)`
([`current-street.md`](current-street.md), `OverspeedHud.kt`).
`overspeed_delta_kmh` is a raw subtraction for tests/HUD helpers; it does
**not** apply that hybrid floor.

Spoken escalating overspeed (`overPct` tiers, arm/disarm) is **not** a UniFFI
export — planned HostApi `road_speed_state_read` + `voice_speak` in
[`plugins/adaptive-speed-warning-spec.md`](plugins/adaptive-speed-warning-spec.md).

### 1.6 Approach / avoidance formatting

| Rust | Purpose |
|---|---|
| `approach_appear_m` / `approach_urgency_m` / `approach_hide_m` | Threshold metres (see `core/src/nav/mod.rs`) |
| `approach_phase_for_distance(active, distance_m)` | Phase name string |
| `format_approach_distance(distance_m, prefer_metric)` | HUD distance text |
| `highway_class_display_label(highway?)` | Human class label when name/ref missing |
| `format_current_road_label(name?, ref?, highway?)` | Bottom-HUD current-road string |
| `road_label_near(pbf, cache_dir, elev_dir, lat, lon, profile, max_m)` | Idle-GPS nearest-edge street label (bbox graph); thin wrapper over `road_near_info` |
| `road_near_info(...)` | Same sticky snap as `road_label_near`, plus applicable speed limit |
| `format_avoid_motorways_report` / `format_route_avoidance_report` | Avoidance summary strings |

Product rules: [`approach-instructions.md`](approach-instructions.md),
[`current-street.md`](current-street.md).

### 1.6b Road signs, children-zone proximity, speed cameras

| Rust | Purpose |
|---|---|
| `road_sign_jurisdiction_allows(lat, lon)` | Norway-only gate for `NO:` catalogue warnings |
| `load_road_signs_json(pbf_path)` | One-time PBF scan → catalogue-matched signs JSON |
| `nearest_road_sign_warning_json(signs_json, lat, lon)` | Nearest tagged-sign approach warning (`phase`, `distance_m`, `icon_key`, `code`, `label`, …) or `{}` |
| `load_school_pois_json(pbf_path)` | School / kindergarten / playground POIs for corridor fallback |
| `schools_near_route_corridor_json(schools_json, sim_samples_json, margin_m)` | Keep POIs within corridor band (app uses 200 m) |
| `nearest_school_proximity_warning_json(schools_json, lat, lon)` | Children-zone fallback warning (`code` `142`, `source=children_proximity`) or `{}` |
| `speed_camera_jurisdiction_allows(lat, lon)` | Jurisdiction pack gate |
| `load_speed_cameras_json(pbf_path)` | Point / average-speed cameras from PBF |
| `nearest_speed_camera_warning_json(cameras_json, lat, lon, opted_in)` | Camera approach / section warning or `{}` |

Product rules: [`road-signs.md`](road-signs.md), README Features (speed cameras).
UI chrome: `RoadSignWarningBox`, speed-camera box (same 750 / 150 / 25 m phases
as maneuvers). Explicit tagged signs **outrank** children-zone proximity in the
host merge. Cluster export of the merged warning:
[`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md).
Audio consumers: [`plugins/custom-alert-sounds-spec.md`](plugins/custom-alert-sounds-spec.md).

### 1.7 OSM updates (opt-in)

| Rust | Purpose |
|---|---|
| `bind_geofabrik_region(data_dir, geofabrik_region, …)` | Bind region metadata |
| `check_osm_updates` / `apply_osm_update` | Check / apply; report strings |
| `set_osm_weekly_reminder` / `osm_weekly_reminder_due` | Reminder opt-in |
| `osm_update_staleness_days` | Full re-download staleness constant |

Never silent: [`osm-updates.md`](osm-updates.md).

### 1.8 Tracks (APRS-style moving icons)

**Object `FfiTrackStore`:**

| Method | Purpose |
|---|---|
| `new(timeout_s, range_km)` | Construct |
| `upsert(...)` | Insert/update station |
| `expire(now_unix)` | Drop stale; returns removed ids |
| `visible(center_lat, center_lon)` | In-range stations |
| `all` / `len` / `timeout_s` / `range_km` | Introspection |

Helpers: `station_timeout_max_s`, `display_range_min_km`, `display_range_max_km`,
`offset_lat_lon_m`, `haversine_km`.

See [`APRS.md`](APRS.md). Live SDR ingest is not UniFFI yet
([`APRS-SDR.md`](APRS-SDR.md)).

### 1.9 Offline basemap / terrain PMTiles

| Rust | Purpose |
|---|---|
| `pmtiles_default_base_url` / `pmtiles_planet_url` / `pmtiles_fallback_planet_url` | Source URLs |
| `pmtiles_region_key` / `pmtiles_region_bbox` | Region id / bbox from Geofabrik path |
| `pmtiles_queue_region` | Queue vector basemap extract job |
| `pmtiles_queue_dem_region` | Queue Mapterhorn DEM → `{region}_dem.pmtiles` |
| `pmtiles_run_job` / `pmtiles_pause_job` / `pmtiles_resume_job` / `pmtiles_cancel_job` | Job control |
| `pmtiles_get_job` / `pmtiles_list_jobs` / `pmtiles_list_covering` / `pmtiles_delete_job` | Job query / cleanup |

Job record: `FfiPmtilesJob`. Style behaviour: [`map-styles.md`](map-styles.md).

---

## 2. Plugin HostApi (WASM)

Implemented capabilities today (`plugin-host/src/abi.rs`):

| Capability string | HostApi method | Purpose |
|---|---|---|
| `position_read` | `position()` | Last known lat/lon |
| `poi_query` | `poi_query(lat, lon, radius_m)` | Nearby POIs |
| `poi_write` | `poi_write(poi)` | Upsert POI |
| `log` | `log(message)` | Host-visible log line |

Guest wrappers: `plugin-sdk` (`host_log`, `host_position`, `host_poi_query`, …).

**Not implemented yet** (roadmap only — see [`plugins.md`](plugins.md)
capability sketch): `track_upsert`, `weather_read`, `incident_*`, `cat_vfo_set`,
`ecu_read`, `voice_speak` / `voice_pack_query`, `route_read`, `nav_guidance_read`,
`vehicle_signal_publish`, i18n, `warning_event_subscribe`, `alert_sound_play` /
`alert_sound_catalog`, `road_speed_state_read` (speed + applicable limit + HUD
overspeed flag for
[`plugins/adaptive-speed-warning-spec.md`](plugins/adaptive-speed-warning-spec.md)),
etc. Specs under `docs/plugins/`.

Guests must not open raw network or WASI filesystem; pack downloads are
host/Tools actions.

---

## 3. What is not UniFFI

| Surface | Location | Notes |
|---|---|---|
| Map HUD prefs (auto-zoom, tilt, 3D, metric) | `MapHudPrefs.kt` SharedPreferences | See [`codebase-map.md`](codebase-map.md) |
| Compose HUD layout | `DriveHud.kt`, [`hud-layout.md`](hud-layout.md) | UI only |
| Overspeed chrome | `OverspeedHud.isOverspeed` in `OverspeedHud.kt` | Display-only; not an alert engine ([`current-street.md`](current-street.md)) |
| Adaptive speed warning | Spec only | [`plugins/adaptive-speed-warning-spec.md`](plugins/adaptive-speed-warning-spec.md) |
| MapLibre camera / style | `MainActivity.kt`, `BasemapStyleResolver.kt` | Host rendering |
| Live ECU polling | `core/src/ecu` types only | [`ECU.md`](ECU.md) — no UniFFI poll yet |
| Voice guidance | Spec only | [`voice-guidance.md`](voice-guidance.md) |
| Documentation languages | `README.md` / `docs/Norwegian.md` | Not in-app i18n ([`plugins/i18n-translation-spec.md`](plugins/i18n-translation-spec.md)) |

---

## 4. Related indexes

| Doc | Covers |
|---|---|
| [`PROTOCOLS.md`](PROTOCOLS.md) | UniFFI + plugins + ECU/APRS/CAT wire notes |
| [`plugins.md`](plugins.md) | Host status, capability sketch, design rules |
| [`current-street.md`](current-street.md) | HUD street label, speed/limit, overspeed chrome |
| [`road-signs.md`](road-signs.md) | Road-sign catalogue, children-zone proximity, approach phases |
| [`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md) | Cluster export of guidance + merged warnings |
| [`plugins/adaptive-speed-warning-spec.md`](plugins/adaptive-speed-warning-spec.md) | Planned spoken overspeed (`overPct`, `road_speed_state_read`) |
| [`ECU.md`](ECU.md) | OBD-II / J1939 / MegaSquirt / EV |
| [`CAT.md`](CAT.md) | Repeater / VFO auto-tune (planned) |
| [`mathematical-formulas.md`](mathematical-formulas.md) | Eco / fuel formulas behind costing |

When adding a UniFFI export: implement in `navi-ffi`, regenerate Kotlin bindings,
document it in this file, and prefer a focused `cargo test -p navi-ffi` or
instrumented call before shipping.
