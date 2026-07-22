# Testers wanted

**Testers wanted** for testing on **actual hardware** (Android Automotive / head
units). Development so far is emulator-only — real devices differ for GPS, MapLibre,
Vulkan/GLES, and performance. Checklist:
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).

# AI assistance

This project was developed with AI assistance (Claude). The author has a
neurological condition related to dyscalculia that affects programming in a way
analogous to how dyscalculia affects mathematical ability — AI assistance was
used to help translate design intent into working code and documentation. Design
decisions, requirements, and testing were directed and reviewed by the author
throughout.

## Table of contents

- [Navi](#navi)
  - [Features](#features)
  - [How features work](#how-features-work)
  - [Settings](#settings)
- [Working app (emulator screenshots)](#working-app-emulator-screenshots)
- [Documents](#documents)
- [Icons (Navit)](#icons-navit)
- [Building Android packages](#building-android-packages)
- [Performance constraints](#performance-constraints-target-8-core--2-ghz-4-gb-ram)
- [Workspace layout](#workspace-layout)
- [Host tests](#host-tests)
- [Known issues](#known-issues)

Further reading in-repo: crate wiring and SQLite layout in
[`architecture.md`](architecture.md); planned plugins in
[`docs/plugins.md`](docs/plugins.md); Android build steps in
[`docs/android-build.md`](docs/android-build.md); HUD bar/menu layout in
[`docs/hud-layout.md`](docs/hud-layout.md).

# Navi

Offline navigation core (Rust) and Android Automotive host (Kotlin/Compose) for
route planning with terrain-aware (eco) costing, POI awareness, rest/overnight
planning, and profile-based routing. Map rendering uses MapLibre (Vulkan SDK)
over an OpenFreeMap liberty basemap. The core stays offline once a region
extract and DEM tiles are on disk; network is opt-in for downloads and updates.

License of this repository: see `LICENSE` (GPL-3.0-or-later unless otherwise
noted). Icon assets under `core/src/icons` are Navit-derived (**GPL v2**); see
[`docs/icons.md`](docs/icons.md).

## Features

| Feature | What you get |
|---|---|
| **Profiles** | Car, motorcycle, cycling, hiking as primary UI modes (truck / electric variants exist in the routing enum) |
| **Eco routing** | Edge costs from elevation + vehicle physics (drag, mass, rolling resistance); optional regen on electric profiles |
| **Corridor / region routing** | OSM `.pbf` → graph → eco-reweight → cached graph → A* corridor route with POI overlay |
| **POI search** | FTS place index from OSM tags; To / Via waypoints from search hits |
| **Rest & breaks** | Profile rest intervals; car HUD shows minutes-to-break; overnight / safety checks for hiking |
| **Drive HUD** | Collapsed top (altitude; tap → map settings) + bottom (zoom −/+, break/ETA, eco; tap → drive settings) |
| **Map rotation** | Compass, direction-of-travel, or north-up camera bearing |
| **Moving icons** | APRS-style tracked stations via `TrackStore` (upsert, timeout, 50–150 km range) |
| **OSM updates** | Opt-in Geofabrik check / `.osc.gz` apply or full re-download ([`docs/osm-updates.md`](docs/osm-updates.md)) |
| **Plugins** | Sandboxed WASM HostApi; ideas for plugins: APRS, weather, road info, CAT, ECU/EV, voice guidance — [`docs/plugins.md`](docs/plugins.md) |

**Real hardware:** Development and automated checks so far use the Android
Automotive **emulator only**. The app **needs testing on real hardware** before
any shipping claim — GPS/IMU, MapLibre native layers, Vulkan/GLES, sensors, and
performance differ from the AVD. Follow
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).

## How features work

**Routing stack.** A regional `.pbf` is parsed into a road graph. With eco on,
edges are reweighted using DEM elevation and `EcoConfig` physics
(`segment_energy_joules`), then persisted (`NAVIGPH1` cache) so the next launch
skips a full reweight. A* finds a corridor; the Android host draws the polyline
and destination marker on MapLibre.

**Eco vs length.** Length-only routing ignores hills. Eco prefers lower energy
(climbs cost PE; ICE regen is 0 so descents are not “free”). Live OBD/J1939
fuel rate can refine costs later via `LiveEnergySnapshot` ([`docs/ECU.md`](docs/ECU.md));
today fuel learning uses persisted tank / fuel-added when no ECU is present.

**POI & search.** Categories and tag rules live in [`docs/poi.md`](docs/poi.md).
Search hits set To/Via and recentre the camera. Basemap POIs come from the
vector style; app-owned markers use rasterized Navit icons.

**Rest / overnight.** Rest parameters are profile-scoped (car hours between
breaks, hiking rast distances, etc.). Safety rules reject overnight candidates
too close to buildings/glaciers. The HUD “Breaks” toggle gates reminder display;
interval/duration defaults are edited in Drive settings.

**Map & HUD.** MapLibre Vulkan renders the basemap. Collapsed top HUD shows GPS
altitude; tap opens map settings (rotation, Trip ETA, Breaks, Auto-zoom level).
Collapsed bottom HUD shows zoom −/+, break time, trip ETA, and eco leaf; tap
opens drive/rest/fuel settings. Maneuver turn stubs are deferred to the
approach-instruction box ([`docs/approach-instructions.md`](docs/approach-instructions.md)).

**Tracks.** `TrackStore` upserts stations by id, expires by timeout, and filters
with Haversine range ([`docs/APRS.md`](docs/APRS.md)). RF decode is not shipped;
IQ via `rtl-sdr-rs` is planned ([`docs/APRS-SDR.md`](docs/APRS-SDR.md)).

## Settings

Settings persist in the app SQLite config store under the device data directory
(UniFFI `load*` / `save*` helpers). Apply on the Drive settings sheet writes and
dismisses; Cancel discards the sheet without saving that edit session.

### Top HUD (collapsed by default — tap to open map settings)

| Control | Behaviour |
|---|---|
| **Collapsed strip** | Shows Map label, GPS altitude, rotation hint; tap toggles map/display settings |
| **Altitude** | GPS sensor altitude in meters (`Alt --` until a fix with altitude) |
| **Compass / Travel / N-up** | In map settings sheet: camera bearing from magnetic heading, GPS course, or north-up |
| **Trip ETA** | In map settings: enables ETA line on the bottom bar |
| **Breaks** | In map settings: enables/disables break-reminder text on the bottom bar |
| **Auto-zoom** | In map settings: when on, snaps zoom to the configured level (−/+ 0.5 steps) |

### Bottom HUD (collapsed — tap status area for drive settings)

| Control | Behaviour |
|---|---|
| **Zoom − / +** | Sole app-owned map zoom (AAOS climate − 63 + in system chrome is not zoom) |
| **Break / ETA** | Time-to-break and trip ETA only (no turn stub — see approach-instructions) |
| **Eco leaf** | Shown on this bar only when eco-mode is active for the profile (`leaf.svg` via icon rasterizer) |
| **Tap status** | Opens drive / rest / fuel settings (no separate Settings link) |

### Drive settings sheet (bottom HUD tap — persisted)

| Field | Persisted as | Notes |
|---|---|---|
| Hours between breaks | Car rest defaults | Profile default for Car, not a one-trip override |
| Rest time (minutes) | Car rest defaults | Same persistence as break interval |
| Eco mode | Car rest `ecoModeEnabled` | Leaf on bottom HUD when on |
| Units liters / gallons | `FuelConfig.prefer_liters` | Display preference; storage is always litres |
| Tank capacity | `FuelConfig.tank_capacity_l` | Converted from gal→L on save when units are gallons |
| Fuel added | `FuelConfig.fuel_added_l` | Feeds adaptive consumption when live ECU is absent |

Auto-zoom level is edited in the **map settings** sheet (top bar), persisted via `MapHudPrefs`.

### Profile / vehicle panel (tools UI — persisted)

| Control | Persisted as | Notes |
|---|---|---|
| Travel profile chip | In-memory + rest load on change | Menu focus: Car, Cycling, Hiking, Motorcycle |
| Eco toggle | With rest / profile defaults | Hiking & cycling lock eco on; motor profiles can toggle |
| Vehicle limits (axle / height / width / weight) | `VehicleLimits` | Used to respect OSM access restrictions where tagged |

### Tracks (APRS-style)

| Setting | Limits | API |
|---|---|---|
| Display range | Clamped **50–150 km** (no unlimited global) | `TrackStore::set_range_km` / `visible` |
| Station timeout | Max **3600 s** | `TrackStore::set_timeout_s` / `expire` |

More detail: [`architecture.md`](architecture.md), [`docs/API.md`](docs/API.md),
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).

## Working app (emulator screenshots)

Captured on Android Automotive emulator with MapLibre + OpenFreeMap liberty
basemap. Collapsed top/bottom drive HUD (search chrome hidden):

![Idle both bars](docs/images/hud/hud_idle_both_bars.png)

All other screenshots (map zoom levels, route overlay, menus, settings
overlays, eco leaf, rotation, bearing, moving icons):
[`docs/pictures.md`](docs/pictures.md).

## Documents

| Document | Description |
|---|---|
| [`architecture.md`](architecture.md) | Crate wiring, thread tiers, SQLite / FTS / graph cache, plugins |
| [`docs/pictures.md`](docs/pictures.md) | Emulator screenshot gallery |
| [`docs/hud-layout.md`](docs/hud-layout.md) | Adjust size and placement of drive HUD bars and menus |
| [`docs/approach-instructions.md`](docs/approach-instructions.md) | Deferred: temporary maneuver approach box (icon + distance + name) |
| [`docs/poi.md`](docs/poi.md) | Searchable POI categories and OSM tag rules |
| [`docs/osm-updates.md`](docs/osm-updates.md) | Opt-in Geofabrik check / `.osc.gz` / full re-download |
| [`docs/plugins.md`](docs/plugins.md) | HostApi, isolation, and planned plugins (APRS, weather, road info, CAT, ECU, voice) |
| [`docs/icons.md`](docs/icons.md) | Icon inventory; custom SVG icons (Inkscape / Synfig); Navit GPL-v2 |
| [`docs/API.md`](docs/API.md) | UniFFI / host API overview |
| [`docs/PROTOCOLS.md`](docs/PROTOCOLS.md) | Wire protocol index (UniFFI, plugins, ECU/APRS/CAT) |
| [`docs/ECU.md`](docs/ECU.md) | ECU protocols: OBD-II, J1939, MegaSquirt + EV SoC/power |
| [`docs/APRS.md`](docs/APRS.md) | APRS fields, TrackStore range filtering, moving icons |
| [`docs/APRS-SDR.md`](docs/APRS-SDR.md) | APRS SDR DSP pipeline; RTL-SDR IF offset; planned `rtl-sdr-rs` |
| [`docs/CAT.md`](docs/CAT.md) | CAT VFO auto-tune from NFM repeaters (≤150 km); OSM network example |
| [`docs/voice-guidance.md`](docs/voice-guidance.md) | Planned voice guidance plugin (recordings + optional Piper) |
| [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md) | **Required:** physical device checklist vs emulator baseline |
| [`docs/android-build.md`](docs/android-build.md) | Compile native `libnavi.so`, UniFFI bindings, and Gradle APKs |
| [`test-results.md`](test-results.md) | Host integration test notes |
| [`android-test-results.md`](android-test-results.md) | On-device / emulator results |

## Icons (Navit)

See [`docs/icons.md`](docs/icons.md) for the full icon system notes. Summary:
POI/maneuver/status icons under `core/src/icons` are Navit-derived (**GPL v2**).
Resolution prefers user overrides, then the bundled set, then `unknown.svg`.

**Custom icons:** use **SVG** (or `.svgz`). Author static art in
[Inkscape](https://inkscape.org/); author animations in
[Synfig Studio](https://www.synfig.org/) and export SVG / frames for Navi.
Name files after the semantic key and place them in the override directory or
`core/src/icons` — step-by-step in [`docs/icons.md`](docs/icons.md#adding-custom-icons).

## Building Android packages

Full guide: [`docs/android-build.md`](docs/android-build.md).

```bash
# 1) Rust CDYLIB + UniFFI Kotlin (emulator ABI)
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/<version>}"
./scripts/build-android-native.sh x86_64-linux-android release

# 2) APK
./gradlew :app:assembleDebug          # → app/build/outputs/apk/debug/
./gradlew :app:installDebug           # install on adb device

# Device / AAOS arm64 instead of emulator:
# ./scripts/build-android-native.sh aarch64-linux-android release

./scripts/launch-navi-emulator.sh      # start MainActivity on AAOS AVD
```

Update `.cargo/config.toml` linker paths to your NDK before the first native
build. `minSdk` 26, `compileSdk` / `targetSdk` 35, JDK 17.

## Performance constraints (target: 8-core ~2 GHz, 4 GB RAM)

Planning targets (not yet measured on the target device). Reference: a Rust
OSM-graph project parsing ~9M nodes / ~18M edges in ~30 s / &lt;5 GB on an 8-core
desktop, scaled down for lower clocks and a 4 GB budget.

| Task | Data scale | Estimated time | Notes |
|---|---|---|---|
| OSM `.pbf` parse + graph build | ~1.5M nodes / ~1.26M edges | ~30–90 s | Mostly single-pass CPU + I/O |
| POI R-tree build | Low thousands of POIs | &lt; 1 s | Near-linear bulk load |
| Eco-reweighting (elevation) | ~1.26M edges, ~9 DEM tiles | ~10–60 s, once per region | Cache decompressed tiles; do not re-read per edge |
| A* single route | ~1.26M edges | &lt; 1 s (often 100–300 ms) | |
| Multi-day + hut matching | Regional graph | 1–3 s | On an already-loaded graph |

### Hard constraint: RAM

- **4 GB is the binding limit**, not CPU frequency.
- Default working set: **county/regional extracts** (~1.5M nodes).
- Country-scale extracts for large countries risk OOM on 4 GB — treat as
  opt-in with an in-app warning ("may be slow or fail on low-RAM devices").
- The 9M-node reference already needed under 5 GB on desktop; that scale is not
  a safe in-memory default on this class of device.

### Required mitigations

1. Cap default load scope at regional extracts; country-scale is opt-in + warning.
2. Persist the reweighted graph after eco-reweight (SQLite or flat binary) — do
   not recompute on every launch.
3. Stream/tile DEM lookups via an LRU tile cache; do not keep every tile fully
   decompressed at once beyond what the warm cache needs.
4. Run graph parse/build on a background (routing-tier) thread with progress UI.

Worker pools must use `std::thread::available_parallelism()` (or equivalent) and
leave headroom for audio/UI (do not saturate every detected core). Routing-tier
work runs at lower OS priority than audio/UI.

## Workspace layout

- `core/` (`driver-break-core`) — elevation, routing, POI, rest/safety, search, icons, tracks, SQLite.
- `navi-ffi/` — UniFFI CDYLIB for Android and other hosts.
- `app/` — Android host (Kotlin/Compose) linking the core via UniFFI.
- `plugin-host/` / `plugin-sdk/` / `plugins/` — sandboxed WASM plugins.
- How crates and databases connect: [`architecture.md`](architecture.md).
- Planned plugins (APRS, weather, road info, CAT, ECU): [`docs/plugins.md`](docs/plugins.md).
- `test-results.md` / `android-test-results.md` — integration reports.

## Host tests

```bash
cargo test --test kongsvinger_lillehammer_integration -- --nocapture --ignored
cargo test --test dnt_hiking_integration -- --nocapture --ignored
cargo test -p navi-plugin-host --test isolation -- --nocapture
cargo test -p driver-break-core poi::
cargo test -p driver-break-core osm_update::
```

## Known issues

- **Moving icons (APRS-style tracked markers):** fixed. Instrumented test
  `MovingIconInstrumentedTest` passes with visible yellow-halo APRS markers on the
  map (see [`docs/pictures.md`](docs/pictures.md)). Root cause was not zoom: MapLibre
  GeoJSON Circle/Symbol layers were not painting on this Automotive emulator even
  with a valid source/images; markers are drawn via a Compose screen-space overlay
  projected from the map camera (z16, after `styleReady`). Residual: native
  MapLibre symbol layers remain unused for tracks until that paint path is
  understood on-device. See
  [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md) for how to
  check whether the native-layer failure is emulator-only.
- **Map rotation SIGSEGV (emulator GLES):** fixed by switching the app dependency
  from `org.maplibre.gl:android-sdk` (OpenGL ES) to
  `org.maplibre.gl:android-sdk-vulkan` 11.8.8. On the Automotive AVD
  (`ro.hardware.egl=emulation`), any non-zero camera bearing under OpenGL crashed
  MapLibre's RenderThread (`SIGSEGV` / fault `0x30` in
  `libGLESv2_enc.so` `GL2Encoder::s_glDrawElements` → `MapRenderer::render`).
  Crash is **bearing-change alone** (no screenshot required), including small
  angles (10°). Matches upstream
  [maplibre-native#2371](https://github.com/maplibre/maplibre-native/issues/2371).
  Same underlying emulator GLES instability class as the moving-icons native
  paint failure; Vulkan avoids the bad path. Verified Compass / bearing shots:
  [`docs/pictures.md`](docs/pictures.md).
