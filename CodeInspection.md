# Code inspection and CI tests

Catalog of what [`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs on
every push to **`main`** / **`dev`** and every pull request, plus the Rust /
Kotlin test suites those jobs exercise.

For how to run the same gate locally before opening a PR, see
[`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md#ci-expectations-github-actions).

**Not** in the per-PR gate (local or `workflow_dispatch` only):

- Android instrumented suite — [`.github/workflows/android-instrumented.yml`](.github/workflows/android-instrumented.yml)
  (see [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md#github-hosted-instrumented-ci))
- Rust `#[ignore]` OSM/DEM integrations that need ~GB fixtures under
  `core/target/integration-fixtures/` (listed under [Dispatch / local only](#dispatch--local-only-not-in-per-pr-ci))

---

## CI jobs (per-PR)

| Job | What it does |
|---|---|
| **`rust-checks`** | `cargo fmt --check`; Clippy (`-D warnings`, excludes wasm guests + `navi-desktop`); `cargo test --workspace` (excludes `navi-desktop` and wasm example guests; does **not** pass `--ignored`); plugin-host `isolation` tests; `scripts/check-plugin-host-gate.sh`; `cargo deny` (+ Android-target advisories); `cargo audit` (+ aarch64/android filters) |
| **`plugin-host-android-aarch64`** | Path-filtered on PRs (always on push to main/dev): NDK cross-compile of plugin-host for `aarch64-linux-android` + QEMU Cranelift smoke (`scripts/plugin-host-android-aarch64-smoke.sh --qemu`) |
| **`linux-build`** | Workspace `cargo build` (excludes WebKit desktop + wasm guests); headless `navi-desktop` with `--features gpsd,linux-imu` |
| **`kotlin-checks`** | `./gradlew :app:ktlintCheck`, `:app:detekt`, `:app:testDebugUnitTest` |
| **`regression-guards`** | Curated host JVM guards for past download/restore bugs + three Rust mini-PBF regressions (parallel; no emulator) |
| **`android-build`** | `./gradlew :app:assembleDebug` (uses committed `jniLibs/*/libnavi.so` when present) |

### `regression-guards` (explicit subset)

**JVM** (`:app:testDebugUnitTest --tests …`):

| Test class | Purpose |
|---|---|
| `OfflinePmtilesBootstrapTest` | mz12 / undersize staged PMTiles must not pass as a full Ostlandet extract (download-completion gap) |
| `OfflineDataIntegrityRestoreTest` | mz12 fixtures must not surface as a production restore offer |
| `CoordinateInputTest` | Lat/lon query parse (comma/space); rejects place names and bare integers |
| `PlaceSearchHintTest` | Place-index empty/building messages; skips live graph work while a foreground plan is active |

**Rust** (checked-in mini PBFs under `core/tests/fixtures/`):

| Test binary | Purpose |
|---|---|
| `motor_access_barrier` | Torggata `motor_vehicle=no` + Kirkebyskogen bollard — Car excluded; Foot/Bike retained |
| `wetland_apply_identity` | Wetland Soft/Hard apply identity on Atnbrufossen mini extract |
| `wetland_pack_identity` | Pack vs PBF wetland Soft/Hard + boardwalk carve-out counters |

---

## Rust — default `cargo test` set (in `rust-checks`)

Integration crates under `core/tests/` that are **not** `#[ignore]` (plus library
unit tests inside `driver-break-core`, `navi-ffi`, etc.). Names are cargo
`--test` binaries unless noted.

| Test | What it checks |
|---|---|
| `bike_suitability_route` | Road-bike profile avoids a short MTB-scaled leg when a longer paved detour exists |
| `friisvegen_seasonal_closure` | Friisvegen seasonal motor closure (needs Ostlandet under integration-fixtures when run fully; may skip if missing) |
| `glacier_overnight_edge` | Glacier overnight edge-distance (not centroid) near Gjende |
| `graph_pack_v5_to_v6_regen` | Planted v5 FlatGraphPack → `VersionMismatch` → `convert_region_packs` rebuilds v6 with vehicle physical limits (Stai bru fixture) |
| `innlandet_real_world_limits` | Indexed-pack path: Fokholgutua maxheight, Atna maxlength/maxweight, Stai maxwidth/maxaxleload, Liabrue maxbogieweight (with under-limit positive controls) |
| `lillehammer_tretten_avoid_motorways` | Avoid-motorways on/off Lillehammer→Tretten (motorway-grade tags) |
| `maneuver_icon_assets` | Every maneuver icon key has SVG in Android lean pack and core icon set |
| `motor_access_barrier` | Same as regression-guards (Torggata / Kirkebyskogen mini PBF) |
| `motorcycle_eco_soft_break` | Motorcycle eco ≠ car Passat; soft pause vs truck HOS spacing |
| `planner_options_routes` | Planner options (avoid motorways/tolls/ferries, vehicle height, network preference, …) change path/cost on synthetic graphs |
| `raufoss_approach_route` | Host plan Grimåsfeltet → Nysethvegen / Tollerud |
| `road_sign_icon_assets` | Rasterize vendored Norwegian road-sign icons (NLOD) |
| `slow_road_osterdalen` | Hiking/cycling slow-road preference when Ostlandet fixture present |
| `truck_driving_history` | TruckDrivingHistory / EC 561 duty-state rolling window |
| `wetland_apply_identity` | Same as regression-guards |
| `wetland_pack_identity` | Same as regression-guards (+ boardwalk tag / hard-over-soft unit cases) |

**Plugin host**

| Test | What it checks |
|---|---|
| `navi-plugin-host` `--test isolation` | Wasm guests: fuel/timeout sandbox, capability deny, log-hello / busy-loop smokes |

**Library unit tests** (examples called out in CI comments; not exhaustive):
`download::pbf_priority` (foreground plan / cone skip),
`basemap::extract::validate_rejects_mz12_large_region_fixture`,
`routing::indexed::graph_pack` round-trips (shape, motorway-grade tags, vehicle
limits, height-limited plan after rkyv), wetland Soft/Hard tag logic.

`navi-ffi/tests/brastein_boardwalk_pack.rs` is **not** in the default gate when
its large Rogaland fixture is absent (skips); it validates boardwalk carve-out
through indexed wetland + foot packs when
`core/target/integration-fixtures/brastein-boardwalk-corridor.osm.pbf` exists.

---

## Dispatch / local only (not in per-PR CI)

Run with `cargo test -p driver-break-core --test <name> -- --ignored --nocapture`
(and fixtures under `core/target/integration-fixtures/`).

| Test | What it checks |
|---|---|
| `dnt_hiking_integration` | Multi-day DNT Aakersaetra → Jammerdalsbu → Rondvassbu |
| `falletvegen_atnbrufossen_eco` | Eco on/off Falletvegen → Atnbrufossen; Atnosen observation |
| `fondsbu_spiterstulen_polyline` | Foot plan Fondsbu → Spiterstulen; writes MapLibre polyline fixture |
| `kongsvinger_lillehammer_integration` | Kongsvinger → Lillehammer corridor |
| `overnight_scan_bench` | Bbox-wide overnight buildings vs corridor pre-filter timing |
| `pilgrim_route_pref` | Ostlandet pilgrim soft-preference + FTS name |
| `place_name_search_check` | FTS place-name checks for common Norwegian queries |
| `route_geometry_audit` | Geometry / road-audit / Atnosen helpers for eco validation |
| `turn_icon_roundabout_audit` | Turn-tier + roundabout maneuvers on Innlandet corridors |
| `venabygdsfjellet_ebike_climb` | Electric Cycle climb + range (and Electric Car pack range) on Venabygdsfjellet |

---

## Kotlin JVM unit tests (`kotlin-checks`)

All under `app/src/test/java/no/navi/app/` via `./gradlew :app:testDebugUnitTest`.

| Class | What it checks |
|---|---|
| `BasemapPoiStyleTest` | Offline Protomaps `pois` kind whitelist, zoom floor, sprite fallback (Vinmonopolet / `shop=alcohol`) |
| `CoordinateInputTest` | Lat/lon parse; reject place names |
| `DiagnosticLogTest` | Diagnostic session log format, categories, toggle, GPS rate-limit, retention |
| `DisplayUnitsTest` | Metric / US / UK distance, speed, fuel formatting |
| `GeofabrikDownloadCatalogTest` | Region granularity notes (Sweden country-only, US states paths, …) |
| `GpsWaypointResolveTest` | GPS → waypoint resolution helpers |
| `MapHudPrefsTiltTest` | Map HUD tilt preference persistence |
| `MapLongPressTest` | Map long-press coordinate / waypoint behaviour |
| `OfflineDataIntegrityRestoreTest` | mz12 must not offer production restore (also in regression-guards) |
| `OfflinePmtilesBootstrapTest` | mz12 must not complete as full extract (also in regression-guards) |
| `OffRouteCoordinatorTest` | Off-route coordinator state machine |
| `OsmUpdateUserCopyTest` | User-facing OSM update copy strings |
| `PlaceSearchHintTest` | Empty/building place-index hints; plan-active skip (also in regression-guards) |
| `RegionCoverageTest` | Norway/Sweden border identity; Ostlandet download coverage |
| `RouteManeuverIconKeyTest` | Full turn-tier `RouteManeuver.iconKey` table (`_1`/`_2`/`_3`) |
| `RouteProgressTrackerOffRouteTest` | Multi-km lateral deviation → off-route; suppress wrong approach guidance |

---

## Lint / supply-chain (no functional tests)

| Step | Job | Purpose |
|---|---|---|
| `cargo fmt --check` | rust-checks | Rust formatting |
| Clippy `-D warnings` | rust-checks | Rust lints as errors |
| `cargo deny` / `cargo audit` | rust-checks | License/advisory policy; Android-target advisories |
| `check-plugin-host-gate.sh` | rust-checks | No premature plugin-host link; wasmtime feature pin |
| ktlint / detekt | kotlin-checks | Kotlin style and static analysis |
| `assembleDebug` | android-build | APK compiles (smoke that resources + JNI stubs link) |

---

## Regenerating mini routing fixtures

See [`core/tests/fixtures/README.md`](core/tests/fixtures/README.md) and the
per-test Overpass/cut comments in `innlandet_real_world_limits.rs` /
`graph_pack_v5_to_v6_regen.rs`.
