**[Dokument på Norsk](Norwegian.md)**

# AI assistance

This project was built with help from AI tools (Cursor). The author has a
neurological condition related to dyscalculia that makes programming harder in a
way similar to how dyscalculia makes maths harder. AI was used to turn design
ideas into working code and docs. The author still chose the product rules,
reviewed the work, and ran the testing.

# Testers wanted

We need people to try Navi on **real devices** — car head units and tablets.
A Samsung Galaxy Tab S6 Lite has been used for checks, but cars and other
devices still differ for GPS, maps, and speed. Checklist:
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).
How to help: [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Table of contents

1. [What this is](#what-this-is)
2. [Features](#features)
   - [What you need to download](#what-you-need-to-download)
   - [How to use](#how-to-use)
   - [How features work](#how-features-work)
3. [Settings](#settings)
4. [Break timer vs trip ETA](#break-timer-vs-trip-eta)
5. [Routing safety](#routing-safety)
6. [Minimum hardware and storage](#minimum-hardware-and-storage)
7. [Screenshots](#screenshots)
8. [Documents](#documents)
9. [Plugins](#plugins)
10. [Coding standards and contributing](#coding-standards-and-contributing)
11. [Building and installing](#building-and-installing)
12. [Where the map data comes from](#where-the-map-data-comes-from)
13. [Known issues](#known-issues)

More detail lives in linked docs (architecture, truck rest rules, map styles,
debugging, and so on). Start with [`CONTRIBUTING.md`](CONTRIBUTING.md) if you
want to help.

# What this is

**Navi** is an offline-first navigation app. You download map data once, then
plan routes on the device without needing the internet for every trip.

It can:

- Plan routes for car, bicycle, e-bike, hiking, motorcycle, truck, and motorhome
- Prefer gentler / less energy-hungry roads when **eco mode** is on (hills matter)
- Suggest rest stops and overnight places along longer trips
- Respect truck driving-time rules where it knows the country rules
- Show a simple map with your route, turns, and place names

The map picture on screen comes from MapLibre. Online it can use OpenFreeMap
Liberty tiles; offline it uses a downloaded regional map file (Protomaps).
Routing uses a separate OpenStreetMap extract you download in **Tools** — that
is the “brain” for finding roads and paths, not just the pretty map.

License: see `LICENSE` (GPL-3.0-or-later unless noted). Many small icons are
from Navit (**GPL v2**); see [`docs/icons.md`](docs/icons.md).

# Features

| Feature | In plain words | Status |
|---|---|---|
| **Travel modes** | Choose car, bike, e-bike, hiking, motorcycle, truck, or motorhome. | Done |
| **Vehicle size** | Save height/width/length/weight limits so the route skips roads that are too tight. | Done |
| **E-bike specs** | Battery size, motor torque, and wheel size help estimate battery use and steep climbs. Live cable telemetry is planned later. | Done (planning); live data later |
| **Avoidances** | You can ask to avoid motorways (not trunk/primary), tolls, or ferries. | Done |
| **Official trails** | For hiking/cycling, optionally prefer marked long-distance trails (off by default). Normal paths still work if the marked trail has a gap. | Done |
| **Eco routing** | Prefer routes that use less energy by taking hills into account. A small leaf icon shows when eco is on. | Done |
| **Offline planning** | Download a region once, then plan and see the route on the device. | Done |
| **Place search** | Search places and set From / Via / To. | Done |
| **Map mark & saved places** | Hold on the map ~4 s to mark a point; set From / Via / To or save a named place (separate from Saved routes). | Done |
| **Breaks & rest** | Reminds you when a break is due and can suggest stops. Cars use hours between breaks; hiking/cycling use rest distances; trucks use legal driving-time rules where known. | Done |
| **Drive bars** | Slim top bar (altitude) and bottom bar (zoom, break timer, trip ETA, road name, eco leaf). | Done |
| **GPS follow** | Map follows you by default. Pan away, then tap **Recenter**. | Done |
| **Map rotation** | North-up, compass, or direction of travel. | Done |
| **Moving icons** | Can draw nearby tracked markers on the map. A live radio feed is not built in yet. | Partial |
| **Seasonal road closures** | OSM `motor_vehicle:conditional` / `access:conditional` hard-filtered against the planned departure time (Car/Truck honour it; Hiking/Bicycle do not). Verified on Friisvegen (way `361797686`) on both bbox/PBF fallback and pack-hit (graph pack **v3**). Purely OSM-tag-driven — no jurisdiction pack. **v1 limitation:** multi-day trips that cross a season boundary are evaluated only at the planned departure instant (not re-evaluated day-by-day along the trip). | Done |
| **Speed camera warnings** | Point cameras use the existing approach distance-phase UX; average-speed / section-control zones use a distinct enter/exit box. `maxspeed:conditional` is evaluated against live local time. Jurisdiction-gated like EC561 / allemannsretten: Norway/UK opt-in (OSM-sourced, may be incomplete); Germany/France/Switzerland and unknown jurisdictions decline — see [`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md). First-run opt-in dialog required (not silently enabled). | Done (display/warning only — no route-avoidance toggle, by deliberate product decision) |
| **Map updates** | Only when you ask — check for OpenStreetMap updates or download a fresh region. Never silent. | Done |
| **Diagnostic logging** | Tools toggle writes a session log (GPS, camera, toggles, route plan/stages, eco, POIs, pauses, instructions, fuel, system) you can copy over USB/MTP — no adb required. Files: **Internal storage → Documents → debug** (`navi_session_*.log`). | Done |
| **Plugins** | A safe sandbox for future add-ons exists; product plugins are not shipped yet. | Host ready |

**Hardware note:** Tablet checks have started (Samsung Galaxy Tab S6 Lite). Car
head units still need more real-world testing before treating this as
ship-ready. See [Screenshots](#screenshots) and
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).

## What you need to download

Nothing useful ships pre-loaded. Use **Tools** in the app (with internet), then
you can go offline.

| Download | Need it? | What it is | Button in Tools |
|---|---|---|---|
| **Map region (roads & places)** | **Yes** for routing and search | OpenStreetMap extract from [Geofabrik](https://download.geofabrik.de/) (example path: `europe/norway/ostlandet`) | **Download region + build place index** |
| **Elevation** | Strongly recommended for eco / hills | Height data for the area | Usually comes with region provision |
| **Offline basemap** | Needed for map graphics without internet | Visual map tiles (Protomaps) | **Download basemap (PMTiles)** |
| **3D terrain** | Optional | Extra height tiles for hillshade | **Download terrain DEM (Mapterhorn)** |
| **OSM updates** | Optional | Fresher roads/POIs | **Check for OSM updates** (never automatic) |

**Minimum to plan a route:** region download + place index.  
**Minimum for a usable offline map picture:** that plus basemap PMTiles (or stay
online for Liberty).  
Prefer a **region** (not a whole huge country) on tablets with limited RAM —
see [Minimum hardware and storage](#minimum-hardware-and-storage).

## How to use

Step-by-step end-user guide (planning, Tools, breaks, saved places/routes,
per-mode options, pilgrim coverage):
**[How to use Navi](docs/how-to-use.md)**.

## How features work

**Planning a route.** Set **From** and **To** (and optional vias), pick a travel
mode, then **Plan route**. From is often set with **Use GPS**. Hiking paths
need the **Hiking** mode — planning with Car uses the road network and will not
follow foot trails properly.

**Eco vs shortest.** Shortest ignores hills. Eco makes steep climbs “cost” more.
Electric modes get some credit for downhill recovery.

**Official networks.** Optional soft preference for marked hiking/cycle routes.
Ordinary paths remain available so a gap never traps you.

**Places.** Search fills From / Via / To. What counts as a hut, rest area, and so
on is documented in [`docs/poi.md`](docs/poi.md).

**Map long-press and saved places.** Hold one finger on the map for about
**4 seconds** to mark a point, then set it as From / Via / To or save it under
**Saved places** (a single named coordinate — not a full **Saved route**).
How-to: [`docs/map-marking-saved-places.md`](docs/map-marking-saved-places.md)
(Norwegian: [`docs/kartmerking-lagrede-steder.md`](docs/kartmerking-lagrede-steder.md)).

**Rest and overnight.** Each mode has its own defaults. Long truck trips can
split into days with legal rest rules (EU or US packs where known). Long
car/bike/hiking trips can suggest overnight stops. The bottom-bar **Breaks**
toggle only shows or hides the reminder — it does not invent a new rest law.

**Map bars.** Tap the top bar for map/display settings. Tap the bottom status
area for drive/vehicle settings (mode, break interval, fuel, e-bike, and so on).

# Settings

**Language:** the app chrome is **English only** today. There is no language
menu yet. Docs may exist in Norwegian (`Norwegian.md`); that is documentation,
not an in-app language pack. A future translation plugin is described in
[`docs/plugins/i18n-translation-spec.md`](docs/plugins/i18n-translation-spec.md).
A working CSV for translators lives next to that spec:
[`docs/plugins/translations.csv`](docs/plugins/translations.csv).

Settings are saved on the device (rest/fuel/vehicle in a small database; map
display choices in app preferences).

### Map / display (tap top bar)

| Setting | Plain meaning |
|---|---|
| **Compass / Travel / N-up** | How the map rotates |
| **Snap rotation back to mode** | After a manual rotate, return to the selected mode (on by default) |
| **Trip ETA** | Show time left to the destination on the bottom bar |
| **Breaks** | Show the “Break in …” reminder line (does not change how stops are planned) |
| **Auto-zoom** | Keep a chosen zoom while moving |
| **3D (experimental)** | Optional hill shading on the map |
| **Map tilt** | Tip the camera (0° / 35° / 45° / 60°) |

### Drive / vehicle (tap bottom status)

| Setting | Plain meaning |
|---|---|
| **Travel mode** | Car, bike, hiking, truck, … |
| **Follow pilgrim routes** | Hiking only; soft preference (off by default), falls back to normal hiking |
| **Hours between breaks** | How often you *want* a break (cars), or truck mandatory break-after time |
| **Rest time** | How long a break should last (suggestion / truck continuous break) |
| **Next break as Time / Distance** | Show break countdown in minutes, or as km/mi at an assumed cruising speed |
| **Eco mode** | Hill-aware energy costing (locked on for hiking/cycling) |
| **POI search radius** | How far aside the planner may look for huts / stops |
| **Vehicle limits** | Height/width/length/axle weight for clearance |

Route planning chrome (**Route**): From / To / Via, Plan, Simulate, avoidances
(**Avoid motorways** excludes `highway=motorway` / `motorway_link` only),
saved routes. **Tools**: download region, basemap, DEM, OSM update check.

Full control lists and truck/jurisdiction detail stay in the older deep docs
linked from [Documents](#documents) when you need them.

# Break timer vs trip ETA

These two numbers on the bottom bar answer **different questions**. They are
not supposed to always match.

| Line | What it means |
|---|---|
| **Break in XXX min** (or km/mi) | “When is the *next planned break due*?” — based on your break **interval** (for example every 2 hours) minus how long you have already been driving since the last break. |
| **ETA XXX min** | “When do we expect to *arrive at the destination*?” — based on the remaining route. |

So if your trip is only **45 minutes** but breaks are set to every **2 hours**,
you can see something like **Break in 120 min** next to **ETA 45 min**. That is
expected: the break reminder is following the interval you configured, not the
end of the trip. On a short trip you may finish before the break is “due.”

Other common reasons they diverge:

1. **Interval longer than the trip** — set a shorter “hours between breaks” (or
   accept that no mid-trip break is needed).
2. **You are part-way along the route** — break time counts down from the
   interval; ETA counts down the remaining road.
3. **Break shown as distance** — minutes are turned into km/mi with a fixed
   assumed cruising speed (~80 km/h for display). That is not your live GPS
   speed, so the distance line is only a rough conversion.
4. **Truck legal clocks** — for truck modes the interval can follow driving-time
   rules (for example a break after 4.5 h of driving), which still is not the
   same thing as “time to destination.”
5. **Before you start moving** — both lines use planning estimates (posted
   speeds or a fixed walking/cycling pace). They update from real progress once
   GPS or simulation is moving.

**Tip:** choose a break interval that fits inside a typical day’s driving for
your trip. There is currently no “split this trip into N equal legs” button —
use the interval (and suggested stops) instead.

# Routing safety

Navi helps you plan; it does not replace judgement, local law, or trail
conditions.

- Hiking and off-trail segments may need care — use your eyes and local advice.
- **Leaving the planned route** (wrong turn, road closure, intentional detour)
  is detected via cross-track distance. The approach box shows **Off route**
  instead of turn distances that would be wrong. Motor profiles auto-replan
  after a short sustained debounce; **Hiking asks first** (trail wander is often
  intentional). Recalculation uses the same planner as Plan (pack-hit when packs
  are ready; otherwise the slower PBF fallback) — see [Known issues](#known-issues)
  and [`docs/route-simulation.md`](docs/route-simulation.md#guidance).
- Wild-camping distance defaults follow a Norway-oriented “right to roam” idea
  and **may not apply in other countries**.
- Truck rest packs only apply where the app can recognise the jurisdiction;
  otherwise it will not pretend to be a legal tachograph.
- Always treat map data (OpenStreetMap) as possibly incomplete or outdated until
  you refresh it yourself.

# Minimum hardware and storage

**Minimum required hardware** for the intended Automotive / embedded class of
device:

| Piece | Minimum / practical note |
|---|---|
| **CPU** | **8 cores**, about **2 GHz** class |
| **RAM** | **4 GB**. Prefer **regional** extracts on that class; whole large countries in one go are often too heavy. On the reference SM-P613 (~3.5 GB total), region **use** is fine via pack-hit or PBF fallback; **background pack conversion** still has thin system memory margin — see [Known issues](#known-issues). |
| **Storage** | Leave room for the region file, place index, offline basemap, optional DEM, and indexed packs — often several GB for a region. |
| **GPU** | MapLibre GLES is the default path used on the tested tablet. |

Mitigations already in the design: regional downloads by default, preprocess-once
indexed packs (background convert after download; region usable immediately via
bbox/PBF fallback), and worker pools that leave room for the UI. Details:
[`docs/indexed-map-format-plan.md`](docs/indexed-map-format-plan.md) and
[`docs/architecture.md`](docs/architecture.md).

**Planning latency note:** when indexed packs are ready, motor pack-hit plans are
typically ~1.5–2 s on the reference tablet. Cold / missing-pack fallback is still
dominated by sequential `.osm.pbf` loads (graph build + POI/barrier), not A*
itself — see [Known issues](#known-issues) and reproduce with
[Tools → Diagnostic logging](docs/debugging.md#3b-diagnostic-session-log-on-device-file)
(`ROUTE_PLAN` / `ROUTE_PLAN_STAGES`).

# Screenshots

Lead examples (Samsung Galaxy Tab S6 Lite **SM-P613** and route simulation):

Landscape with optional **3D** hillshade (offline Protomaps + local terrain):

![SM-P613 offline Protomaps + Mapterhorn DEM hillshade (landscape)](docs/images/Screenshot_20260731_123844.jpg)

Hiking corridor Skolla → Rondvassbu (**SIMULATING**):

![Skolla to Rondvassbu hike](docs/images/terrain/hike_eldabu_ramshogda_3d.png)

GPS follow during simulation:

![Follow while simulating](docs/images/follow_gps/01_simulating_follow.png)

### Real hardware (SM-P613)

Portrait, offline Ostlandet Protomaps, 3D off:

![SM-P613 offline Protomaps 2D (portrait)](docs/images/Screenshot_20260731_123746.jpg)

Car head-unit testing is still open —
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).

### More captures

Idle HUD:

![Idle both bars](docs/images/hud/hud_idle_both_bars.png)

Map tilt 45° (3D off / on):

![45° tilt, 3D off](docs/images/tilt45_3d_off.png)

![45° tilt, 3D on](docs/images/tilt45_3d_on.png)

Follow / pan / Recenter / rotation:

![Follow while simulating](docs/images/follow_gps/01_simulating_follow.png)

![After pan](docs/images/follow_gps/02_after_pan.png)

![After Recenter](docs/images/follow_gps/05_after_recenter.png)

![Rotation modes](docs/images/follow_gps/06_rotation_modes_ok.png)

Full gallery: [`docs/pictures.md`](docs/pictures.md) (Norwegian:
[`docs/bilder.md`](docs/bilder.md)).

# Documents

| Document | What it is for |
|---|---|
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | How to contribute |
| [`docs/architecture.md`](docs/architecture.md) | How the pieces fit together |
| [`docs/codebase-map.md`](docs/codebase-map.md) | Where to change code for a given feature |
| [`docs/pictures.md`](docs/pictures.md) / [`docs/bilder.md`](docs/bilder.md) | Screenshot galleries |
| [`docs/map-styles.md`](docs/map-styles.md) | Online vs offline map look; 3D |
| [`docs/poi.md`](docs/poi.md) | Place types and search |
| [How to use Navi](docs/how-to-use.md) | End-user how-to (planning, Tools, breaks, saved places/routes, profiles) |
| [`docs/map-marking-saved-places.md`](docs/map-marking-saved-places.md) | Map long-press (4 s) and Saved places detail (Norwegian: [`kartmerking-lagrede-steder.md`](docs/kartmerking-lagrede-steder.md)) |
| [`docs/ec-561-truck-rest.md`](docs/ec-561-truck-rest.md) | EU truck driving-time rules |
| [`docs/fmcsa-truck-rest.md`](docs/fmcsa-truck-rest.md) | US truck hours-of-service |
| [`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md) | Country/region rule packs |
| [`docs/osm-updates.md`](docs/osm-updates.md) | Opt-in map updates |
| [`docs/android-build.md`](docs/android-build.md) | Build the Android app |
| [`docs/build-linux.md`](docs/build-linux.md) | Linux / desktop build (tools, gpsd, adb) |
| [`docs/build-macos.md`](docs/build-macos.md) | macOS build (tools, Android NDK, adb) |
| [`docs/build-windows.md`](docs/build-windows.md) | Windows build (MSVC, tools, Android NDK, adb) |
| [`docs/debugging.md`](docs/debugging.md) | Debugging |
| [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md) | Physical device checklist |
| [`docs/status.md`](docs/status.md) | Which docs are live status vs historical evidence |
| [`docs/future-proofing-audit-2026-07.md`](docs/future-proofing-audit-2026-07.md) | Tracked future-proofing / open risk items |
| [`docs/indexed-map-format-plan.md`](docs/indexed-map-format-plan.md) | Phased evaluation of preprocess-once indexed routing maps |
| [`docs/plugins.md`](docs/plugins.md) | Plugin host and roadmap |

See the `docs/` folder for more specialised topics (voice, APRS, ECU, formulas,
and so on).

# Plugins

A sandboxed plugin host exists so future add-ons can run safely. **No product
plugins ship in the app yet** — that is intentional. Overview:
[`docs/plugins.md`](docs/plugins.md).

| Spec | Topic |
|---|---|
| [`docs/plugins/i18n-translation-spec.md`](docs/plugins/i18n-translation-spec.md) | Future UI languages (English-only today). Translator table: [`translations.csv`](docs/plugins/translations.csv) |
| [`docs/plugins/right-to-roam-camping-spec.md`](docs/plugins/right-to-roam-camping-spec.md) | Wild-camping suggestions (plugin, not core) |
| [`docs/plugins/safety-resupply.md`](docs/plugins/safety-resupply.md) | Fuel/water resupply ideas |
| [`docs/plugins/instrument-cluster-agl-spec.md`](docs/plugins/instrument-cluster-agl-spec.md) | Export nav state to instrument clusters |
| [`docs/plugins/animated-icons-spec.md`](docs/plugins/animated-icons-spec.md) | Animated icons |

## Icons (Navit)

POI / turn / status icons under `core/src/icons` are mostly from Navit
(**GPL v2**). Custom maintainer-authored overrides (same mechanism) include
`leaf.svg` (eco) and `speed_camera.svg` (speed-camera warnings) — see
[`docs/icons.md`](docs/icons.md).

# Coding standards and contributing

Please read **[`CONTRIBUTING.md`](CONTRIBUTING.md)**.

Short version of CI expectations:

| Area | Expectation |
|---|---|
| Rust | `cargo fmt`, Clippy with warnings denied, tests |
| Kotlin | ktlint, detekt, unit tests |
| Android | `./gradlew :app:assembleDebug` |

# Building and installing

Full guides:

- Android APK: [`docs/android-build.md`](docs/android-build.md)
- Linux (core, `navi-desktop`, gpsd, adb): [`docs/build-linux.md`](docs/build-linux.md)
- macOS (tools, NDK, adb): [`docs/build-macos.md`](docs/build-macos.md)
- Windows (MSVC, tools, NDK, adb): [`docs/build-windows.md`](docs/build-windows.md)

### Emulator (x86_64)

```bash
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/<version>}"
rustup target add x86_64-linux-android   # once
./scripts/build-android-native.sh x86_64-linux-android release
./gradlew :app:assembleDebug
./gradlew :app:installDebug
./scripts/launch-navi-emulator.sh
```

### Tablet / phone (arm64)

```bash
rustup target add aarch64-linux-android   # once
./scripts/build-android-native.sh aarch64-linux-android release
./gradlew :app:assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n no.navi.app/.MainActivity
```

Confirm the APK contains the arm64 library:

```bash
unzip -l app/build/outputs/apk/debug/app-debug.apk | grep 'lib/arm64-v8a/libnavi.so'
```

### Workspace layout

- `core/` — routing, places, rest rules, icons (Rust)
- `navi-ffi/` — bridge to Android and other hosts
- `app/` — Android UI (Kotlin)
- `plugin-host/` / `plugin-sdk/` / `plugins/` — future plugins
- [`docs/architecture.md`](docs/architecture.md) — how it fits together

### Host tests (examples)

```bash
cargo test -p driver-break-core --test planner_options_routes
cargo test -p navi-plugin-host --test isolation -- --nocapture
```

Large map-file integration tests are usually marked `#[ignore]` and need
fixtures under `core/target/integration-fixtures`. See
[`docs/test-results.md`](docs/test-results.md).

# Where the map data comes from

Navi is **offline-first**. The network is used only when you choose to download
or update.

| Data | Source | Used for |
|---|---|---|
| Roads & places | OpenStreetMap via Geofabrik `.osm.pbf` | Routing and search |
| Map updates | Geofabrik diffs / fresh extract | Opt-in refresh only |
| Elevation | Public DEM tiles | Eco / hills |
| Map picture | OpenFreeMap Liberty (online) or Protomaps PMTiles (offline) | What you see on screen |
| Position | Device GPS (or gpsd on Linux) | Where you are |
| Icons | Mostly Navit-derived SVG; custom `leaf` / `speed_camera` | Markers and turns |

Country/region visual extracts can also be prepared with
[PMT-splitter](https://github.com/Supermagnum/PMT-splitter/tree/main).

# Known issues

- **Plugins:** content add-ons are intentionally not shipped yet, as they have
  not been made.
- **UI polish:** the screens work but still need visual tidy-up on car displays.
- **Moving icons:** drawn with a Compose overlay today; native map symbol layers
  are not the primary path yet.
- **Map / GPU quirks:** some emulator and phone GPU setups have historically
  crashed or washed out hillshade; the project defaulted to MapLibre GLES after
  tablet checks. Details in [`docs/map-styles.md`](docs/map-styles.md) and
  [`docs/debugging.md`](docs/debugging.md).
- **Screenshot-only lake fringe:** a soft blue rim around water can appear in
  captures but not while using the app live — see
  [`docs/map-styles.md`](docs/map-styles.md).
- **Region-scale pack conversion has thin memory margin on lower-end 4GB-class
  hardware.** Measured on a Samsung Galaxy Tab S6 Lite (SM-P613, ~3.5GB total
  RAM — below this project's nominal 4GB floor): a full Østlandet-scale
  background conversion completed successfully with no crash and no process
  kill, but system-level available memory dropped to ~329 MiB at its lowest
  point, ~250 MiB of swap was used, and Android issued
  `TRIM_MEMORY_RUNNING_CRITICAL` to other running processes during the
  conversion's POI phase. This indicates real, if survived, system memory
  pressure — not a comfortable margin. Devices with less RAM than this
  reference device, or under heavier concurrent load (more background apps,
  less free memory at the time conversion starts), remain an open risk. Region
  download and immediate use are unaffected (bbox/PBF fallback works
  immediately regardless); this risk is specific to the background
  pack-conversion step. Further mitigation (e.g. smaller tile size, trading
  more wall-clock time for wider memory margin) has not yet been implemented.
  Cross-ref: [`docs/indexed-map-format-plan.md`](docs/indexed-map-format-plan.md)
  and [Minimum hardware and storage](#minimum-hardware-and-storage).
- **Cold / missing-pack planning is still data-loading-bound; pack-hit is not.**
  A* itself is typically sub-second. **With indexed packs ready** (Phase 4 + 4b;
  see [`docs/indexed-map-format-plan.md`](docs/indexed-map-format-plan.md)):
  motor pack-hit ~1.5–1.8 s vs cold missing-pack ~31 s on Hedmark 162 km;
  wetland load **18.6 s → 93 ms** (~199×) on SM-P613; long Hiking OD that
  previously aborted ~6 min completes (~61 s) with `wetland_pack_hit`.
  Historical pre-pack Car Espa→Atnbrufossen (SM-P613): `plan_duration_ms=26835`
  with `graph_build_ms=17571`, `poi_barrier_ms=8045`, `astar_ms=378` — that
  ~15–27 s class remains the **fallback** when packs are missing, stale, or
  still converting in the background. `.navigph` deprecated. Residual Hiking
  wall time can still include overnight/POI PBF work. Reproduce stages with
  Diagnostic logging → `ROUTE_PLAN` / `ROUTE_PLAN_STAGES`
  ([`docs/debugging.md`](docs/debugging.md#3b-diagnostic-session-log-on-device-file)).
  See also [`docs/status.md`](docs/status.md).
- **Rerouting after a detour is not instant.** Off-route (cross-track beyond
  ~75 m motor / ~100 m hiking) reuses the same planner: **pack-hit when packs
  are ready** (~seconds class matching Plan), otherwise the slower PBF
  fallback. While off-route, the approach box shows **Off route**. A
  **Recalculating route…** banner is shown during the wait (with Cancel). This
  is an expected wait, not a hang. Motor profiles auto-reroute after a short
  sustained debounce (~5 s); **Hiking prompts** first. See
  [`docs/route-simulation.md`](docs/route-simulation.md#guidance).
- **Hiking plan speed on huge areas (addressed for packs; residual PBF work):**
  overnight buildings use a 1.5 km corridor pre-filter and a single PBF scan for
  POI + buildings when packs do not cover that work (exact 150 m
  allemannsretten check unchanged). Historical DNT Åkersætra→Rondvassbu
  (~**139.9 km** corridor; `overnight_scan_bench`, debug): bbox-all
  **102 556 buildings / ~180.7 s** load → corridor **487 buildings / ~83.1 s**
  load. Graph/wetland pack-hit removes the old multi-minute wetland abort; some
  overnight/POI decode cost can remain.
- **Break timer ≠ trip ETA:** by design — see
  [Break timer vs trip ETA](#break-timer-vs-trip-eta).
- **Not implemented yet:** checking whether the code can be optimised for
  rendering.
