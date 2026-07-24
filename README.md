**[Dokument på Norsk](Norwegian.md)**

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
  - [Where data comes from](#where-data-comes-from)
  - [How features work](#how-features-work)
  - [Settings](#settings)
- [Working app (emulator screenshots)](#working-app-emulator-screenshots)
- [Documents](#documents)
- [Icons (Navit)](#icons-navit)
- [Building Android packages](#building-android-packages)
- [Performance constraints](#performance-constraints-minimum-8-core--2-ghz-4-gb-ram)
- [Workspace layout](#workspace-layout)
- [Host tests](#host-tests)
- [Known issues](#known-issues)

Further reading in-repo: how the pieces fit together in
[`docs/architecture.md`](docs/architecture.md); plugin ideas in
[`docs/plugins.md`](docs/plugins.md); Android build steps in
[`docs/android-build.md`](docs/android-build.md); Linux core build in
[`docs/build-linux.md`](docs/build-linux.md); debugging in
[`docs/debugging.md`](docs/debugging.md); HUD bar/menu layout in
[`docs/hud-layout.md`](docs/hud-layout.md); map styles / offline maps / 3D in
[`docs/map-styles.md`](docs/map-styles.md); truck driving-time rules in
[`docs/ec-561-truck-rest.md`](docs/ec-561-truck-rest.md); US FMCSA truck HOS in
[`docs/fmcsa-truck-rest.md`](docs/fmcsa-truck-rest.md); country/region rule packs in
[`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md); IMU mount calibration (deferred) in
[`docs/imu-calibration.md`](docs/imu-calibration.md).

# Navi

Offline navigation core (Rust) and Android Automotive host (Kotlin/Compose) for
route planning with terrain-aware (eco) costing, POI awareness, rest/overnight
planning, and profile-based routing. Map rendering uses MapLibre (Vulkan SDK)
over an OpenFreeMap liberty basemap. The core stays offline once a region
extract and DEM tiles are on disk; network is opt-in for downloads and updates.

License of this repository: see `LICENSE` (GPL-3.0-or-later unless otherwise
noted). Icon assets under `core/src/icons` are Navit-derived (**GPL v2**); see
[`docs/icons.md`](docs/icons.md).

This navigation app has optional awareness of terrain steepness: with eco mode
on, it tries to find the route that uses the least energy (passenger-car
baseline physics; electric profiles get descent regen credit via
`EcoConfig::for_profile`). When eco mode is on, a small leaf icon is visible in
the app’s lower-right corner. It can suggest break stops along a planned route
(amenities / huts / tent sites / lodging / rest areas depending on profile), apply
vehicle clearance limits and avoid motorway/toll/ferry preferences on plan, and
optionally **follow official hiking/cycling networks** (soft preference, off by
default). You can set car break intervals. Truck profiles apply EC 561/2006
driving-time rules with multi-day daily/weekly rest when a trip outlasts one
duty day. It includes an in-memory moving-icon store (`TrackStore`) and a
sandboxed WASM plugin host for future plugins; product plugins are not shipped
yet ([`docs/plugins.md`](docs/plugins.md)).

## Features

| Feature | What you get | Status |
|---|---|---|
| **Travel modes** | Car, motorcycle, bicycle, hiking, truck, and motorhome. Electric versions exist for later use; the main buttons are the everyday modes. | Done |
| **Vehicle size limits** | Save height, width, length, axle load, and similar limits. Routes skip roads the map says are too tight or too low for your vehicle. | Done |
| **Avoidances** | Turn on avoid motorways, tolls, or ferries and the planned path actually changes. | Done |
| **Follow official networks** | For hiking and cycling, prefer marked long-distance trails and cycle routes when that option is on (off by default). Ordinary paths stay available so a gap in the marked network never strands you. Named trails are searchable. | Done |
| **Eco routing** | Prefer routes that use less energy by taking hills into account. Electric modes get credit for downhill recovery. Formulas: [`docs/mathematical-formulas.md`](docs/mathematical-formulas.md). | Done |
| **Offline route planning** | Download a map region once, plan on the device, and see the route line plus suggested stops on the map. | Done |
| **Place search** | Search places and set From / Via / To ([`docs/poi.md`](docs/poi.md)). Includes fishing spots and hut search distance guidance ([`docs/poi-search-defaults.md`](docs/poi-search-defaults.md)). | Done |
| **Rest & breaks** | Break reminders and suggested stops along the route. Hiking and cycling use traditional Scandinavian rest distances ([background](docs/historical-background.md)). **Truck** / **TruckElectric** resolve a jurisdiction pack at start: EU EC 561/2006 ([`docs/ec-561-truck-rest.md`](docs/ec-561-truck-rest.md)) or US FMCSA property-carrying HOS ([`docs/fmcsa-truck-rest.md`](docs/fmcsa-truck-rest.md)); unknown jurisdictions decline legal tracking. Multi-day day cards and overnight pins are shown in the plan UI (live-verified on emulator GPS Norway Minnesund belt → Bodø; see [`docs/pictures.md`](docs/pictures.md)). **Car** / **motorcycle** / **cycle** / **mobile home** use soft multi-day overnight splitting when a trip exceeds a daily budget (8 h driving or 100 km cycling) with lodging/camping/rest-area suggestions ([`docs/poi.md`](docs/poi.md)). Hiking overnight pauses prefer huts/tents and keep a respectful distance from buildings and glaciers; day-by-day multi-day overnight is planned in `planHikingRoute`. Country/region rule packs: [`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md). | Done |
| **Drive bars** | Slim top bar (altitude; tap for map settings) and bottom bar (zoom, break time, trip ETA, eco leaf; tap for drive settings). | Done |
| **Map rotation** | Align the map with the compass, with your travel direction, or with north always up. | Done |
| **Moving icons** | Show nearby tracked markers on the map (for example radio station symbols) within about 50–150 km. | **Partial** — drawing works; a live radio feed is not built in yet |
| **Map data updates** | When you choose, check for OpenStreetMap updates and apply them, or download a fresh region ([`docs/osm-updates.md`](docs/osm-updates.md)). Never updates quietly in the background. | Done |
| **Plugins** | Sandboxed WASM host is ready. Product plugins are not shipped yet on purpose; several are specified for contributors ([`docs/plugins.md`](docs/plugins.md) — camping, resupply, instrument cluster/AGL, ECU, APRS, …). | Host ready; content deferred |

**Real hardware:** So far the app has been developed and checked mainly on the
Android Automotive **emulator**. It still **needs testing on real head units**
before anyone treats it as ready to ship — GPS, sensors, graphics, and speed
differ on real cars. Checklist:
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).

## Where data comes from

Navi is **offline-first**: routing, search, and eco costing run from files on
disk. Network is used only when you opt in (provision, update check/apply, or
live basemap tiles while online).

| Data | Source | How it is used |
|---|---|---|
| **Road / POI extract** | [OpenStreetMap](https://www.openstreetmap.org/) via [Geofabrik](https://download.geofabrik.de/) regional `.osm.pbf` (or a custom corridor cut) | Graph for routing; FTS place/address index; POI categories |
| **OSM updates** | Geofabrik `state.txt` + `.osc.gz` diffs or full `*-latest.osm.pbf` | Opt-in check/apply only — never silent ([`docs/osm-updates.md`](docs/osm-updates.md)) |
| **Elevation (DEM)** | Copernicus DSM / SRTM / Viewfinder-style tiles (downloaded or seeded as archives) | Eco-route energy costs and related terrain logic |
| **Basemap (visual)** | Online: [OpenFreeMap](https://openfreemap.org/) Liberty (MapLibre). Offline: regional **Protomaps PMTiles** + bundled Protomaps light style ([`docs/map-styles.md`](docs/map-styles.md)). Optional opt-in **3D**: Mapterhorn DEM hillshade (online TileJSON or local `{region}_dem.pmtiles`; Vulkan-gated) | On-screen map; not the routing graph |
| **Position / heading** | Device GPS (Android) or **gpsd** + IMU on Linux | Live location, altitude HUD, Compass / direction-of-travel |
| **Icons** | Bundled Navit-derived SVG under `core/src/icons` | Maneuver / POI / eco leaf rasterization |

Once a region extract and DEM tiles are on the device, core navigation does not
need the network. The visual basemap uses live OpenFreeMap Liberty until a
regional PMTiles file is downloaded (Tools → Download basemap); then Protomaps
tiles load offline. Optional terrain DEM is the same path with
**Download terrain DEM (Mapterhorn)** (`{region}_dem.pmtiles`).
([`docs/map-styles.md`](docs/map-styles.md)).
Country/region PMTiles extracts can be prepared with
[PMT-splitter](https://github.com/Supermagnum/PMT-splitter/tree/main).

## How features work

**Follow official networks (hiking / cycling).** Off by default. When on, the
planner prefers marked hiking and cycling networks where they exist, but it can
still use ordinary paths so a missing stretch of trail never blocks the whole
route. Trail difficulty notes may appear as extra info on the plan. Named
official routes are included in place search. Not yet: preferring higher-tier
networks over local ones, and some special “node network” styles used in parts
of Europe.

**How a route is planned.** You download a regional OpenStreetMap extract once.
Navi builds a road network from it. With eco mode on, hills change how
“expensive” each road segment is, and that result is cached so the next plan is
faster. The app finds a path and draws it on the map with the destination and
any suggested breaks.

**Eco vs shortest.** Shortest distance ignores hills. Eco prefers less energy
use, so steep climbs cost more. Petrol and diesel modes do not treat downhill as
“free”; electric modes get partial credit for energy recovered going downhill.
If a car computer (OBD / similar) is connected later, live fuel use can refine
this ([`docs/ECU.md`](docs/ECU.md)). Today, without that, the app can learn from
tank size and fuel added.

**Places and search.** What counts as a cafe, hut, fishing spot, and so on is
described in [`docs/poi.md`](docs/poi.md). Suggested search distances for network
huts and trails are in [`docs/poi-search-defaults.md`](docs/poi-search-defaults.md).
Search results set From / Via / To and move the map. The basemap shows its own
labels; app markers use the bundled icons.

**Rest and overnight.** Each travel mode has its own break defaults. Cars and
motorcycles use hours between breaks; hiking and cycling use traditional
Scandinavian rest distances
([`docs/historical-background.md`](docs/historical-background.md));
**Truck** / **TruckElectric** use jurisdiction-keyed driving-time rules: EU
EC 561/2006 ([`docs/ec-561-truck-rest.md`](docs/ec-561-truck-rest.md)) when the
corridor starts in an EC 561 / EEA-aligned country bbox, or US FMCSA
property-carrying Hours of Service
([`docs/fmcsa-truck-rest.md`](docs/fmcsa-truck-rest.md)) in the US. Unrecognized
starts decline commercial legal tracking rather than guessing. EC 561 includes
multi-day daily rest (11 h / reduced 9 h / split 3+9), weekly rest after at most
six consecutive working days when the trip does not fit the remaining daily
budget, a **compensation ledger** after reduced weekly rests (Art. 8 shortfall +
deadline, surfaced in the plan report), and **detour-weighted / facility-tier**
overnight stop scoring (`highway=services` preferred over bare rest areas
within a similar detour). Multi-day day cards appear in the plan panel when a
trip spans more than one day. **Mobile home** keeps car-style soft reminders (not
HGV legal tracking). When a
car / motorcycle / mobilehome / cycle trip exceeds the soft daily budget
(default **8 h** driving or **100 km** cycling), the planner splits into days
and suggests overnight lodging, camping, or rest-area stops near day boundaries
(informational if no POI is nearby — see [`docs/poi.md`](docs/poi.md)
**Lodging** / **RestArea**). For hiking, `planHikingRoute` places hut/tent
pauses along rast intervals, rejects overnight candidates too close to
buildings or glaciers, and when the trip exceeds the daily distance budget
(default **40 km**) splits into days with overnight hut pins near day
boundaries (`plan_hiking_multi_day` in core; same scoring spirit as the DNT
integration helper). The building-distance idea follows the Norwegian **right to roam**
(*allemannsretten*): wild camping is generally allowed if you stay a respectful
distance from houses and cultivated land. That is a Norway-oriented default and
**may not apply elsewhere** — local camping law can be stricter; country packs
follow [`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md). The “Breaks”
toggle only controls whether the reminder is shown; edit times in Drive
settings (Car vs Truck when a truck profile is selected).

**Map and on-screen bars.** The map is drawn with MapLibre. The collapsed top
bar shows altitude; tap it for map settings (rotation, trip ETA, breaks,
auto-zoom). The collapsed bottom bar shows zoom, break time, trip ETA, and the
eco leaf; tap it for drive, rest, and fuel settings. Near a turn, a short
instruction box shows the maneuver, distance, and next street
([`docs/approach-instructions.md`](docs/approach-instructions.md)).

**Altitude on the emulator.** The Automotive emulator’s GPS height is often
wrong (for example 0 m or a large offset at a known hill). That is an
**emulator GPS limitation**, not an app bug. The altitude readout prefers
terrain height from downloaded elevation files when available; on a real device,
GPS height can still be used when those files are missing.

**Moving markers.** Nearby tracked stations can appear on the map and time out
when stale ([`docs/APRS.md`](docs/APRS.md)). Live radio decoding is not included
yet; USB SDR support is planned ([`docs/APRS-SDR.md`](docs/APRS-SDR.md)).

## Settings

Settings persist in the app SQLite config store under the device data directory
(UniFFI `load*` / `save*` helpers). Apply on the Drive settings sheet writes and
dismisses; Cancel discards the sheet without saving that edit session.

### Top HUD (collapsed by default — tap to open map settings)

| Control | Behaviour |
|---|---|
| **Collapsed strip** | Shows Map label, altitude, rotation hint; tap toggles map/display settings |
| **Altitude** | DEM terrain height when a tile covers the fix; otherwise GPS altitude (`Alt --` until either is available). Emulator GNSS altitude is often wrong — that is the AVD, not the app (see note above) |
| **Compass / Travel / N-up** | In map settings sheet: camera bearing from magnetic heading, GPS course, or north-up |
| **Trip ETA** | In map settings: enables ETA line on the bottom bar |
| **Breaks** | In map settings: enables/disables break-reminder text on the bottom bar |
| **Auto-zoom** | In map settings: when on, snaps zoom to the configured level (−/+ 0.5 steps) |

**Pre-departure duration estimates** (shown before the vehicle/hiker/cyclist starts moving) are calculated estimates, not live measurements — based on posted `maxspeed` limits (with a highway-class fallback where the tag is missing) for Car/Motorcycle/Truck, and fixed average-pace figures (16 min/km hiking, ~4 min/km cycling on average terrain) for Hiking/Cycling. These are starting estimates only; actual travel time will vary with real conditions, traffic, weather, fitness, and terrain, and updates automatically once real movement/GPS speed data is available.

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
| Hours between breaks | Car **or** Truck rest defaults | Truck / TruckElectric use EC 561 `mandatory_break_after_hours`; Mobile home uses **car** rest (not EC 561) |
| Rest time (minutes) | Car **or** Truck rest defaults | Truck continuous 45 (or 15+30 when split is on) |
| Eco mode | Profile rest `ecoModeEnabled` | Leaf on bottom HUD when on |
| Truck split break / +1 h exceptional | `TruckRestParams` | Truck / TruckElectric only; exceptional arm is explicit opt-in |
| Units liters / gallons | `FuelConfig.prefer_liters` | Display preference; storage is always litres |
| Tank capacity | `FuelConfig.tank_capacity_l` | Converted from gal→L on save when units are gallons |
| Fuel added | `FuelConfig.fuel_added_l` | Feeds adaptive consumption when live ECU is absent |

Auto-zoom level is edited in the **map settings** sheet (top bar), persisted via `MapHudPrefs`.

### Profile / vehicle panel (tools UI — persisted)

| Control | Persisted as | Notes |
|---|---|---|
| Travel profile chip | In-memory + rest load on change | Menu focus: Car, Cycling, Hiking, Motorcycle, Truck, Mobile home |
| Eco toggle | With rest / profile defaults | Hiking & cycling lock eco on; motor profiles can toggle |
| **Follow official hiking/cycling networks** | `prefer_official_networks` (default off) | Hiking / Cycling only — soft cost preference; gaps fall back to ordinary paths |
| Avoid motorways / tolls / ferries | Passed into `plan_car_route` each plan | Changes the planned route (not report-only) |
| Vehicle limits (axle / bogie / height / width / length / weight) | `VehicleLimits` | Applied on plan for motor profiles; violating OSM clearance edges are excluded and an alternate is sought |

### Tracks (APRS-style)

| Setting | Limits | API |
|---|---|---|
| Display range | Clamped **50–150 km** (no unlimited global) | `TrackStore::set_range_km` / `visible` |
| Station timeout | Max **3600 s** | `TrackStore::set_timeout_s` / `expire` |

More detail: [`docs/architecture.md`](docs/architecture.md), [`docs/API.md`](docs/API.md),
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).

## Working app (emulator screenshots)

Captured on Android Automotive emulator with MapLibre + OpenFreeMap liberty
basemap. Collapsed top/bottom drive HUD (search chrome hidden):

![Idle both bars](docs/images/hud/hud_idle_both_bars.png)

Car route Helgøya → Atnbrua on the Automotive emulator (HUD shows altitude;
AVD GNSS altitude is often wrong — see note above). One rest stop is visible:

![Helgøya to Atnbrua route](docs/images/terrain/hike_eldabu_ramshogda_3d.png)

All other screenshots (map zoom levels, route overlay, menus, settings
overlays, eco leaf, rotation, bearing, moving icons):
[`docs/pictures.md`](docs/pictures.md)
(Norwegian gallery: [`docs/bilder.md`](docs/bilder.md)).

## Documents

| Document | Description |
|---|---|
| [`docs/architecture.md`](docs/architecture.md) | How the parts fit together (databases, threads, plugins) |
| [`docs/pictures.md`](docs/pictures.md) | Emulator screenshot gallery |
| [`docs/bilder.md`](docs/bilder.md) | Emulator screenshot gallery (Norwegian) |
| [`docs/historical-background.md`](docs/historical-background.md) | Rast/vei basis for hiking & cycling rest-interval defaults |
| [`docs/ec-561-truck-rest.md`](docs/ec-561-truck-rest.md) | Truck EC 561/2006: duty caps, multi-day rest, compensation ledger, overnight scoring |
| [`docs/fmcsa-truck-rest.md`](docs/fmcsa-truck-rest.md) | Truck US FMCSA property-carrying HOS pack (11 h / 14 h / 8 h break / 70 h cycle) |
| [`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md) | Pattern for country/region-dependent rules (EC 561 + FMCSA + right-to-roam precedents) |
| [`docs/horse-profile.md`](docs/horse-profile.md) | Worked example: adding a Horse profile (doc only; not implemented) |
| [`docs/hud-layout.md`](docs/hud-layout.md) | Adjust size and placement of drive HUD bars and menus |
| [`docs/map-styles.md`](docs/map-styles.md) | Online Liberty vs offline Protomaps PMTiles; 3D gate |
| [`docs/approach-instructions.md`](docs/approach-instructions.md) | Deferred: temporary maneuver approach box (icon + distance + name) |
| [`docs/poi.md`](docs/poi.md) | Searchable POI categories (Fishing, RestArea, Lodging, …), OSM tag rules, how to add types |
| [`docs/poi-search-defaults.md`](docs/poi-search-defaults.md) | Suggested hut/trail POI search radii for hiking & cycling (DNT spacing) |
| [`docs/osm-updates.md`](docs/osm-updates.md) | Opt-in Geofabrik check / `.osc.gz` / full re-download |
| [`docs/plugins.md`](docs/plugins.md) | Plugin **host** status (intentional: no content plugins yet) + HostApi, isolation, roadmap ideas |
| [`docs/plugins/right-to-roam-camping-spec.md`](docs/plugins/right-to-roam-camping-spec.md) | Spec: allemannsretten / multi-country wild-camping suggestions (plugin, not core) |
| [`docs/plugins/safety-resupply.md`](docs/plugins/safety-resupply.md) | Spec: fuel/water resupply lookahead, POI confidence, remote/arid buffers (plugin, not core) |
| [`docs/plugins/instrument-cluster-agl-spec.md`](docs/plugins/instrument-cluster-agl-spec.md) | Spec: export nav state to clusters/AGL via VSS/Kuksa + JSON fallback (plugin, not core) |
| [`docs/icons.md`](docs/icons.md) | Icon inventory; custom SVG icons (Inkscape / Synfig); Navit GPL-v2 |
| [`docs/API.md`](docs/API.md) | UniFFI / host API overview |
| [`docs/PROTOCOLS.md`](docs/PROTOCOLS.md) | Wire protocol index (UniFFI, plugins, ECU/APRS/CAT) |
| [`docs/ECU.md`](docs/ECU.md) | ECU protocols: OBD-II, J1939, MegaSquirt + EV SoC/power |
| [`docs/mathematical-formulas.md`](docs/mathematical-formulas.md) | Formulas: MAF/J1939/MegaSquirt fuel, range, eco segment energy |
| [`docs/APRS.md`](docs/APRS.md) | APRS fields, TrackStore range filtering, moving icons |
| [`docs/APRS-SDR.md`](docs/APRS-SDR.md) | APRS SDR DSP pipeline; RTL-SDR IF offset; planned `rtl-sdr-rs` |
| [`docs/CAT.md`](docs/CAT.md) | CAT VFO auto-tune from NFM repeaters (≤150 km); OSM network example |
| [`docs/voice-guidance.md`](docs/voice-guidance.md) | Planned voice guidance plugin (recordings + optional Piper) |
| [`docs/android-build.md`](docs/android-build.md) | Compile native `libnavi.so`, UniFFI bindings, and Gradle APKs |
| [`docs/build-linux.md`](docs/build-linux.md) | Linux: Rust core, integration tests, gpsd + IMU (no desktop map UI yet) |
| [`docs/imu-calibration.md`](docs/imu-calibration.md) | Deferred: vehicle-mount IMU pitch/roll zeroing for eco elevation |
| [`docs/approach-instructions.md`](docs/approach-instructions.md) | Approach-instruction box (Navit prior art + locked thresholds) |
| [`docs/debugging.md`](docs/debugging.md) | Host + Android debug loops (logcat, Studio, instrumented tests) |
| [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md) | **Required:** physical device checklist vs emulator baseline |
| [`docs/test-results.md`](docs/test-results.md) | Host integration test notes |
| [`docs/android-test-results.md`](docs/android-test-results.md) | On-device / emulator results |

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

## Performance constraints (minimum: 8-core ~2 GHz, 4 GB RAM)

**Minimum required hardware** for the intended Automotive / embedded class of
device (not a “nice to have” desktop target):

| Resource | Minimum |
|---|---|
| CPU | **8 cores**, about **2 GHz** class |
| RAM | **4 GB** |

Planning estimates below are not yet measured on that device class. Reference: a
Rust OSM-graph project parsing ~9M nodes / ~18M edges in ~30 s / &lt;5 GB on an
8-core desktop, scaled down for lower clocks and a 4 GB budget.

| Task | Data scale | Estimated time | Notes |
|---|---|---|---|
| OSM `.pbf` parse + graph build | ~1.5M nodes / ~1.26M edges | ~30–90 s | Mostly single-pass CPU + I/O |
| POI R-tree build | Low thousands of POIs | &lt; 1 s | Near-linear bulk load |
| Eco-reweighting (elevation) | ~1.26M edges, ~9 DEM tiles | ~10–60 s, once per region | Cache decompressed tiles; do not re-read per edge |
| A* single route | ~1.26M edges | &lt; 1 s (often 100–300 ms) | |
| Multi-day + hut matching | Regional graph | 1–3 s | Hiking integration / hut POI matching on a loaded graph; full day-by-day hiking segmentation is test-helper only, not UniFFI |

### Hard constraint: RAM

- **4 GB is the binding limit**, not CPU frequency.
- Default working set: **county/regional extracts** (~1.5M nodes).
- Country-scale extracts for large countries risk OOM on 4 GB — treat as
  opt-in with an in-app warning ("may be slow or fail on low-RAM devices").
- The 9M-node reference already needed under 5 GB on desktop; that scale is not
  a safe in-memory default on this class of device.

### Minimum free storage (SD card / internal drive)

Offline **routing** data (Geofabrik `.osm.pbf` + on-disk graph cache + place/FTS
index + DEM tiles + scratch for updates). Does **not** include MapLibre basemap
tiles unless you also download regional **PMTiles** (add roughly another
**1–3×** a comparable Geofabrik extract for a clipped Protomaps region, plus
bundled sprites/glyphs already in the APK). See [`docs/map-styles.md`](docs/map-styles.md).

Geofabrik `.osm.pbf` sizes (approx., mid-2026; they grow over time):

| Country / extract | `.osm.pbf` only | **Minimum free space to budget** |
|---|---|---|
| **Sweden** | ~0.8 GB | **~3–5 GB** |
| **Norway** | ~1.3 GB | **~4–6 GB** |
| **Russia** | ~4.1 GB | **~12–16 GB** |
| **Germany** | ~4.8 GB | **~14–18 GB** |
| **USA** | ~12 GB | **~36–48 GB** |

Budget rule of thumb: keep about **3–4×** the `.osm.pbf` free so graph build,
eco-reweight cache, FTS index, DEM coverage, and a temporary second copy during
OSM update/re-download all fit. Prefer a **regional** extract (e.g. Norway
Østlandet ~0.4 GB PBF, or a US state / Russian federal district) on 4 GB RAM
devices. Full **Germany**, **Russia**, or especially the **USA** are not
practical as a single in-memory country load on the minimum hardware — disk may
fit with a large card; RAM will not.

App install / APK and icon assets are small relative to country extracts.

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
- `plugin-host/` / `plugin-sdk/` / `plugins/` — sandboxed WASM host (content plugins deferred; see [`docs/plugins.md`](docs/plugins.md)).
- How the parts fit together: [`docs/architecture.md`](docs/architecture.md).
- [`docs/test-results.md`](docs/test-results.md) /
  [`docs/android-test-results.md`](docs/android-test-results.md) — integration reports.

## Host tests

```bash
cargo test -p driver-break-core --test planner_options_routes
cargo test -p driver-break-core --test truck_driving_history -- --nocapture
cargo test -p driver-break-core truck_multi_day -- --nocapture
cargo test -p driver-break-core motor_multi_day -- --nocapture
cargo test -p driver-break-core rest_area -- --nocapture
cargo test -p driver-break-core lodging -- --nocapture
cargo test -p driver-break-core --test overnight_scan_bench -- --ignored --nocapture
cargo test --test kongsvinger_lillehammer_integration -- --nocapture --ignored
cargo test --test dnt_hiking_integration -- --nocapture --ignored
cargo test -p navi-plugin-host --test isolation -- --nocapture
cargo test -p driver-break-core fishing -- --nocapture
cargo test -p driver-break-core osm_update::
```

`planner_options_routes` covers vehicle limits, avoidances, official-network soft
preference, EV regen cost, overnight filter, fishing category behaviour, and
truck break spacing vs car heuristic without a full region extract.
`truck_driving_history` covers empty history, multi-day accumulation / daily
extensions, and rolling fortnight pruning. `truck_multi_day` (unit tests in
`core/src/routing/rest/truck_multi_day.rs`) covers day segmentation, weekly
rest after six working days, and corridor RestArea attachment; `motor_multi_day`
covers soft car/cycle overnight day splits and lodging preference; `rest_area` /
`lodging` match classifier tests for those POI tags. `overnight_scan_bench` times the removed redundant
overnight PBF scan vs POI+barrier reuse on the DNT corridor bbox. Optional fishing
hit against Ostlandet:

```bash
cargo test -p driver-break-core --test planner_options_routes fishing_found -- --ignored --nocapture
```

**Live-GPS truck plan (host):** start coordinates must come from
`adb shell dumpsys location` (no hardcoded corridor starts). Set
`NAVI_START_LAT` / `NAVI_START_LON` from that fix, choose the destination only
after the start is known, then:

```bash
cargo run -p navi-ffi --bin plan-truck-live-gps --release
```

See `navi-ffi/src/bin/plan_truck_live_gps.rs`. Multi-day daily rest and history
read/write were confirmed on a live-GPS Norway run (Minnesund belt → Bodø,
~1068 km / ~16 h → two driving days with an 11 h daily rest between). Long
corridors use span-scaled trip bbox padding so the graph clip does not cut the
road network (e.g. E6 west of Trondheim).

## Known issues

- **Plugins (content):** the WASM host/sandbox is ready; shipping product plugins
  (APRS, weather, allemannsretten camping, resupply, instrument cluster/AGL,
  ECU, marine, etc.) is intentionally deferred for independent contributors —
  specs live under [`docs/plugins.md`](docs/plugins.md). Not a defect in the
  navigation core.
- **GUI polish:** the Compose HUD / search / tools UI works but still needs visual
  and UX polishing (spacing, typography, density on Automotive screens). If you
  want to improve the look-and-feel, please do — contributions welcome.
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
  **Note:** corridor / approach **route `LineLayer`** is a different path — it
  paints under the Vulkan SDK (no screen-space workaround). Missing route lines
  in early approach shots were empty polyline injection in the test, not this
  Circle/Symbol GLES issue ([`docs/approach-instructions.md`](docs/approach-instructions.md)).
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
- **TODO — optimize building load for hiking overnight-proximity check.** A
  single hiking plan currently takes ~177.6 s on a large corridor (measured:
  DNT Åkersætra→Rondvassbu, Østlandet), driven almost entirely by loading all
  buildings in the route’s bounding box (~102 556 buildings) for the 150 m
  allemannsretten distance check — well above this project’s original 30–90 s
  parse/build target for 4 GB-class hardware. Most of those buildings are far
  from the actual route corridor within a large bbox. A coarser corridor-based
  pre-filter (excluding buildings well outside a generous margin around the
  route path itself, before doing exact distance math) could likely cut this
  significantly without weakening the 150 m check’s correctness. Not yet
  implemented — worth revisiting if hiking-plan latency becomes a real
  user-facing complaint.
