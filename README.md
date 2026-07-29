**[Dokument på Norsk](Norwegian.md)**

# Testers wanted

**Testers wanted** for testing on **actual hardware** (Android Automotive / head
units or tablets). Development so far is emulator-only — real devices differ for GPS, MapLibre,
Vulkan/GLES, and performance. Checklist:
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).
How to contribute (testing and more): [`CONTRIBUTING.md`](CONTRIBUTING.md).

# AI assistance

This project was developed with AI assistance (Cursor). The author has a
neurological condition related to dyscalculia that affects programming in a way
analogous to how dyscalculia affects mathematical ability — AI assistance was
used to help translate design intent into working code and documentation. Design
decisions, requirements, and testing were directed and reviewed by the author
throughout.

## Table of contents

- [Navi](#navi)
  - [Features](#features)
  - [Where data comes from](#where-data-comes-from)
  - [What you need to download](#what-you-need-to-download)
  - [How features work](#how-features-work)
  - [Settings](#settings)
  - [Debugging](#debugging)
- [Working app (emulator screenshots)](#working-app-emulator-screenshots)
- [Documents](#documents)
- [Icons (Navit)](#icons-navit)
- [Building Android packages](#building-android-packages)
- [Minimum hardware and storage capacity](#minimum-hardware-and-storage-capacity)
- [Workspace layout](#workspace-layout)
- [Host tests](#host-tests)
- [Known issues](#known-issues)

Further reading in-repo: how to contribute in
[`CONTRIBUTING.md`](CONTRIBUTING.md); how the pieces fit together in
[`docs/architecture.md`](docs/architecture.md); Rust crates (first-party vs
crates.io) in [`docs/rust-crates.md`](docs/rust-crates.md); plugin ideas in
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
| **Travel modes** | Car, bicycle, **electric cycle**, hiking, motorcycle, truck, and mobile home as primary chips. Electric car/truck/motorcycle variants exist in the enum for routing/rest; they are not primary menu chips today. | Done |
| **Vehicle size limits** | Save height, width, length, axle load, and similar limits. Routes skip roads the map says are too tight or too low for your vehicle. | Done |
| **Electric cycle specs** | Battery Wh, motor torque (Nm), and wheel diameter (inches) persist like car fuel tank settings. Plan reports estimated % of battery used and climb-capability warnings when a segment exceeds torque/wheel-derived max grade. Live DIY wired telemetry (USB-serial `$NAVIPWR`) is specified for custom BMS/display builders — not implemented yet ([`docs/ebike-telemetry-diy.md`](docs/ebike-telemetry-diy.md)). | Done (physics); live telemetry deferred |
| **Electric car pack** | Battery capacity (kWh, default 60) persists via `EvCarConfig`; plan reports estimated % of pack used (regen included). No climb-capability model for cars. Primary UI chips do not include Electric car yet (enum / FFI ready). | Done (config); chip deferred |
| **Avoidances** | Turn on avoid motorways, tolls, or ferries and the planned path actually changes. | Done |
| **Follow official networks** | For hiking and cycling, prefer marked long-distance trails and cycle routes when that option is on (off by default). Ordinary paths stay available so a gap in the marked network never strands you. Named trails are searchable. | Done |
| **Eco routing** | Prefer routes that use less energy by taking hills into account. Electric modes get credit for downhill recovery. Formulas: [`docs/mathematical-formulas.md`](docs/mathematical-formulas.md). | Done |
| **Offline route planning** | Download a map region once, plan on the device, and see the route line plus suggested stops on the map. | Done |
| **Place search** | Search places and set From / Via / To ([`docs/poi.md`](docs/poi.md)). Includes fishing spots and hut search distance guidance ([`docs/poi-search-defaults.md`](docs/poi-search-defaults.md)). | Done |
| **Rest & breaks** | Break reminders and suggested stops along the route. Hiking and cycling use traditional Scandinavian rest distances ([background](docs/historical-background.md)). **Truck** / **TruckElectric** resolve a jurisdiction pack at start: EU EC 561/2006 ([`docs/ec-561-truck-rest.md`](docs/ec-561-truck-rest.md)) or US FMCSA property-carrying HOS ([`docs/fmcsa-truck-rest.md`](docs/fmcsa-truck-rest.md)); unknown jurisdictions decline legal tracking. Multi-day day cards and overnight pins are shown in the plan UI (live-verified on emulator GPS Norway Minnesund belt → Bodø; see [`docs/pictures.md`](docs/pictures.md)). **Car** / **motorcycle** / **cycle** / **mobile home** use soft multi-day overnight splitting when a trip exceeds a daily budget (8 h driving or 100 km cycling) with lodging/camping/rest-area suggestions ([`docs/poi.md`](docs/poi.md)). Hiking overnight pauses prefer huts/tents and keep a respectful distance from buildings and glaciers; day-by-day multi-day overnight is planned in `planHikingRoute`. Country/region rule packs: [`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md). | Done |
| **Drive bars** | Slim top bar (altitude; tap for map settings) and bottom bar (zoom, break time, trip ETA, current road/street name, eco leaf; tap for drive settings). | Done |
| **GPS follow / recenter** | Map follows the GPS (or simulation) fix by default. Panning or pinching pauses follow; **Recenter** re-enables it. | Done |
| **Map rotation** | Align the map with the compass, with your travel direction, or with north always up. | Done |
| **Moving icons** | Show nearby tracked markers on the map (for example radio station symbols) within about 50–150 km. | **Partial** — drawing works; a live radio feed is not built in yet |
| **Map data updates** | When you choose, check for OpenStreetMap updates and apply them, or download a fresh region ([`docs/osm-updates.md`](docs/osm-updates.md)). Never updates quietly in the background. | Done |
| **Plugins** | Sandboxed WASM host is ready. Product plugins are not shipped yet on purpose; several are specified for contributors ([`docs/plugins.md`](docs/plugins.md) — camping, resupply, instrument cluster/AGL, UI translation, ECU, APRS, …). | Host ready; content deferred |

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

## What you need to download

Nothing ships with a ready-to-route map region. Use **Tools** (map settings /
tools sheet) on a network connection, then work offline. Prefer a **regional**
Geofabrik path on low-RAM devices (see
[Minimum hardware and storage capacity](#minimum-hardware-and-storage-capacity)).

| Download | Required? | What it is | In-app action |
|---|---|---|---|
| **OSM region (`.osm.pbf`)** | **Yes** — for routing, place search, and POIs | OpenStreetMap extract from [Geofabrik](https://download.geofabrik.de/) (path like `europe/norway/ostlandet`) | Set **Geofabrik path** → **Download region + build place index** |
| **Elevation (DEM) tiles** | **Strongly recommended** for eco / hill-aware costing; optional if you only need flat shortest-distance plans | Copernicus / SRTM / Viewfinder-style height tiles (often pulled with region provision, or seeded as an archive) | Included with region provision when available; otherwise seed DEM into the app data dir |
| **Basemap (PMTiles)** | **Optional** while online (OpenFreeMap Liberty works); **needed for offline map graphics** | Regional Protomaps visual tiles | **Download basemap (PMTiles)** |
| **Terrain DEM (Mapterhorn)** | **Optional** — only for 3D hillshade | `{region}_dem.pmtiles` | **Download terrain DEM (Mapterhorn)** |
| **OSM updates** | **Optional** — only when you want fresher roads/POIs | Geofabrik diffs or a fresh `*-latest.osm.pbf` | **Check for OSM updates** / apply (never automatic) |

**Minimum to plan a route:** one Geofabrik region download + place-index build.
**Minimum for a usable offline map screen:** that plus a regional PMTiles
basemap (or stay online for Liberty). **Eco routing:** also need DEM coverage
for the area you drive.

Sizes and free-space budgets: [Minimum free storage](#minimum-free-storage-sd-card--internal-drive).
Update policy: [`docs/osm-updates.md`](docs/osm-updates.md). Basemap detail:
[`docs/map-styles.md`](docs/map-styles.md).

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
labels; app markers use the bundled icons. **From must be set before Plan route
works** — typically use **Use GPS as from** (current GPS position). With From
unset, Plan shows “Set From and To first” and does not compute a corridor.

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
auto-zoom, 3D hillshade, camera tilt). The collapsed bottom bar shows zoom,
**Recenter** (when follow is paused), break time, road/street name, trip ETA,
and the eco leaf; tap the status area for travel mode, rest, fuel, and electric
cycle settings. **Hiking routes require the Hiking travel mode** — planning with Car
(or another motor profile) uses the road graph and will fail or produce a
useless path for foot trails. Near a turn, a short instruction box shows the
maneuver, distance, and next street
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

**UI language:** the in-app chrome is **English only** today. There is **no
language-switching control** in map or drive settings. Parallel markdown
(`Norwegian.md`, `docs/bilder.md`, …) is documentation, not an in-app locale
system. A future UI translation plugin is specified in
[`docs/plugins/i18n-translation-spec.md`](docs/plugins/i18n-translation-spec.md).

Settings persist under the app data directory: rest / fuel / e-bike / EV / vehicle
/ network preference via SQLite (UniFFI `load*` / `save*`); map HUD prefs
(auto-zoom, 3D, tilt, break display mode, Geofabrik path, PMTiles base URL,
diagnostic logging) via `MapHudPrefs` SharedPreferences. **Save** on a sheet
writes and dismisses; **Close** dismisses without requiring a save for controls
that already apply immediately (map sheet toggles, many Tools fields).

Primary travel-mode chips: **Car**, **Bicycle**, **Electric cycle**, **Hiking**,
**Motorcycle**, **Truck**, **Mobile home**. `CarElectric` / `TruckElectric` /
`MotorcycleElectric` exist in the enum for routing and rest packs but are **not**
primary menu chips today.

### Map / display settings (tap top HUD)

| Setting | What it does |
|---|---|
| **Compass** | Rotate the map from the magnetic / compass heading feed |
| **Travel** | Rotate the map from GPS (or simulated) direction of travel |
| **N-up** | Keep map north-up (bearing 0°) |
| **Trip ETA** | Show remaining trip ETA on the bottom bar (pre-departure estimate until live progress updates it) |
| **Breaks** | Show the break-reminder line on the bottom bar (`Break in …` / off). Does not change planned stop spacing by itself |
| **Auto-zoom** | When on, snap camera zoom to the set level while moving |
| **Auto-zoom − / +** | Change the target zoom in 0.5 steps (about z 3–20; default 16.5) |
| **3D (experimental)** | Opt-in Mapterhorn DEM **hillshade** on the basemap (Vulkan-gated). Independent of camera tilt; see [`docs/map-styles.md`](docs/map-styles.md) |
| **Map tilt** | Snap camera pitch to **0° / 35° / 45° / 60°** (Vulkan-gated; locked at 0° without Vulkan; 60° is MapLibre’s max). Works with 3D on or off |
| **Save / Close** | Persist map HUD prefs and close, or dismiss |

**Pre-departure duration estimates** (before real movement) use posted `maxspeed`
(with highway-class fallback) for motor profiles, and fixed pace (about 16 min/km
hiking, ~4 min/km cycling) for Hiking / Bicycle / Electric cycle. They are
starting estimates only; live progress updates once GPS / simulation speed is
available.

### Bottom HUD chrome

| Control | What it does |
|---|---|
| **Zoom − / +** | App-owned map zoom (AAOS climate − / + in system chrome is not zoom) |
| **Recenter** | Shown after you pan or pinch away from GPS follow; re-enables follow and recenters on the fix |
| **Currently on …** | Current road / street name when known ([`docs/current-street.md`](docs/current-street.md)) |
| **Break / ETA lines** | Time (or distance) to next break when a route is planned, and trip ETA when enabled |
| **Eco leaf** | Visible when eco mode is on for the active profile |
| **Tap status area** | Opens drive / vehicle settings (not the zoom / Recenter buttons) |

**GPS follow:** on by default while a fix exists. Manual pan or pinch pauses
follow so you can look around; **Recenter** (or the test hook equivalent) turns
it back on. Rotation modes (Compass / Travel / N-up) still apply while following.

### Drive / vehicle settings (tap bottom HUD)

| Setting | What it does |
|---|---|
| **Travel mode** | Primary chips above. Selects the planner and rest pack. **Hiking must be selected for hiking routes** — otherwise planning uses the motor/road graph and fails or misroutes foot paths |
| **Hours between breaks** | Soft car-style interval (label: “Desired hours between breaks”), or truck **mandatory break after (hours)**. Saved as profile defaults, not a one-trip override |
| **Rest time (minutes)** | Suggested rest duration (truck: continuous break length) |
| **Split break 15+30 min** | Truck only — prefer split break metadata instead of one continuous break |
| **Arm +1 h exceptional extension** | Truck only — explicit opt-in for exceptional driving-time extension |
| **Next break shown as Time / Distance** | Bottom-bar break line as minutes or as km/mi at an assumed cruise speed (~80 km/h for display only) |
| **Distance units km / miles** | When break-as-distance is on, choose metric or imperial for that line |
| **Eco mode** | Terrain-aware energy costing; leaf on bottom HUD. Hiking / Bicycle / Electric cycle lock eco on; motor profiles can toggle |
| **Electric cycle specs** | Shown for **Electric cycle**: battery capacity (Wh), motor torque (Nm), wheel diameter inches (presets 20 / 26 / 27.5 / 29 or custom). Persist like fuel settings; used for plan % of capacity and climb-capability warnings |
| **Fuel units liters / gallons** | Display preference for non-electric motor profiles; stored values are litres |
| **Fuel tank capacity** | Tank size for adaptive consumption heuristics |
| **Fuel added** | Last fill amount for the same heuristics |
| **Save / Close** | Write rest / fuel / e-bike (and EV pack when that profile is active) to SQLite and dismiss, or dismiss without that save |

**Break interval vs trip ETA.** Set the time (or distance) to the next break so it
does **not exceed the trip ETA** (or remaining trip distance). If the break
interval is longer than the whole trip, reminders and suggested stops get
wonky. There is currently **no** control to auto-split a corridor into N equal
parts (for example “cut this distance into 6 legs”); choose a break interval
that fits inside the planned duration/distance instead.

### Route planning chrome (search / Profile / Vehicle / Saved routes)

Opened from the map when planning chrome is visible (**Route** reopens it if
collapsed). Tools is a separate panel (below).

| Control | What it does |
|---|---|
| **From / To / Via** | Search targets; Place vs Address mode chips |
| **Use GPS** | Set From (or the active target) from the current GPS fix. Resolves a nearby place/address or road name within **12 m** when the place index / region PBF can supply one; otherwise labels as `GPS (lat, lon)` |
| **Plan route / Delete route** | Run the planner for the active profile, or clear the planned corridor |
| **Simulate route** | Drive the planned polyline with the in-app simulator (emulator-friendly) |
| **Continue from last stop** | Resume planning from the last break / overnight when available |
| **Profile → Eco routing** | Same eco on/off as drive settings (locked on for hiking/cycling) |
| **Profile → Follow official hiking/cycling networks** | Soft cost preference for marked networks (default **off**). Hiking / Bicycle / Electric cycle only; persists via UniFFI |
| **Profile → Avoid motorways/trunk/primary** | Changes the next motor plan (not report-only) |
| **Profile → Avoid toll roads** | Same for tolls |
| **Profile → Avoid ferries** | Same for ferries |
| **Vehicle limits** | **Car:** height (m). **Truck / Mobile home:** axle weight (kg), max bogie weight (kg), height / width / length (m). Motor plans exclude OSM edges that violate clearance. (Total weight exists in the FFI model but has no UI field yet.) |
| **Saved routes** | List / refresh / delete named saved corridors; save current plan |

### Tools panel (main chrome → Tools)

Region provision, basemap / DEM downloads, and opt-in OSM updates. See also
[`docs/map-styles.md`](docs/map-styles.md) and [`docs/osm-updates.md`](docs/osm-updates.md).

| Setting | What it does |
|---|---|
| **Download scope** | **Country** vs **Region in country** chips (Norway presets: Østlandet, Vestlandet, Trøndelag, Nord-Norge, Sørlandet). Country-scale warns about low-RAM devices |
| **Geofabrik path** | Editable path (e.g. `europe/norway/ostlandet`); persisted in `MapHudPrefs` |
| **Download region + build place index** | Fetch Geofabrik `.osm.pbf`, bind the region, build the place/FTS index |
| **Planet PMTiles URL** | Optional Protomaps planet URL (blank = latest); persisted |
| **Download basemap (PMTiles)** | Range-extract regional visual tiles for offline Protomaps style |
| **Download terrain DEM (Mapterhorn)** | Optional `{region}_dem.pmtiles` for 3D hillshade |
| **Pause / Resume / Cancel** | Control an in-flight PMTiles / DEM job |
| **Check for OSM updates** | Opt-in Geofabrik freshness check — never downloads silently |
| **Apply pending OSM update** | Apply the plan from a prior Check (user-confirmed) |
| **Weekly update reminder** | Reminder only (no auto-download) |
| **Diagnostic logging** | Off by default. When on, writes a date-stamped session log under app-private storage (see [Debugging](#debugging)) |
| **Export diagnostic log** | Share the latest session log via Android's share sheet |
| **Save / Close** | Persist Geofabrik path + PMTiles URL, or dismiss |

### Tracks (APRS-style)

There is **no** dedicated in-app settings sheet for tracks yet. Runtime clamps
are fixed in the track store API:

| Limit | Value |
|---|---|
| **Display range** | Show stations within **50–150 km** (clamped; no unlimited global) |
| **Station timeout** | Drop stale stations after at most **3600 s** |

Live radio decoding is not built in; drawing of injected markers works (see
Features). Details: [`docs/APRS.md`](docs/APRS.md).

More detail: [`docs/architecture.md`](docs/architecture.md),
[`docs/rust-crates.md`](docs/rust-crates.md),
[`docs/codebase-map.md`](docs/codebase-map.md), [`docs/API.md`](docs/API.md),
[`docs/hud-layout.md`](docs/hud-layout.md), [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).

### Debugging

**Diagnostic logging** (off by default, toggle in **Tools**) writes a structured,
date-stamped log file to the device covering GPS status, settings changes, route
planning, eco climb/descent energy (logged separately), POIs found, planned rest
stops, turn-by-turn instruction events, fuel/battery updates, and basic system
resource status. This is intended for debugging and troubleshooting — turn it on
if you are reporting a bug and want to include real diagnostic evidence, or if
you are testing the app yourself. It has no effect on navigation behavior and is
not enabled by default.

The log stays **local on the device** (app-private storage). Navi does not upload
or transmit these session files anywhere by default. Use **Export diagnostic log**
in Tools to share a file when you choose to, or pull it with adb (path documented
in [`docs/debugging.md`](docs/debugging.md)). Older session files are rotated
automatically (last 10 kept).

## Working app (emulator screenshots)

Captured on Android Automotive emulator with MapLibre + OpenFreeMap liberty
basemap. Collapsed top/bottom drive HUD (search chrome hidden):

![Idle both bars](docs/images/hud/hud_idle_both_bars.png)

Car route Helgøya → Atnbrua on the Automotive emulator (HUD shows altitude;
AVD GNSS altitude is often wrong — see note above). One rest stop is visible:

![Helgøya to Atnbrua route](docs/images/terrain/hike_eldabu_ramshogda_3d.png)

Map camera tilt presets (0° / 35° / 45° / 60°) are independent of opt-in 3D
hillshade. Finstad → Søndre Ommang → Ådalsbruk motormuseum at **45°** — flat
2D (N-up), then 3D on with Mapterhorn DEM hillshade.
These shots demonstrate tilt/3D. Older captures in this gallery may show a blue
hydro soft-edge fringe at river/lake edges; that fringe has been confirmed **not
visible during live interactive use** and is treated as a
[screenshot-capture artifact](docs/map-styles.md#hydro-soft-edge-fringe-screenshot-artifact)
(instrumented `screencap` / UiAutomation timing), not a user-visible rendering
limitation:

![45° tilt, 3D off](docs/images/tilt45_3d_off.png)

![45° tilt, 3D on](docs/images/tilt45_3d_on.png)

GPS follow: simulation while following, after pan / zoom (follow paused), then
**Recenter**, and rotation-mode check:

![Follow while simulating](docs/images/follow_gps/01_simulating_follow.png)

![After pan](docs/images/follow_gps/02_after_pan.png)

![After Recenter](docs/images/follow_gps/05_after_recenter.png)

![Rotation modes](docs/images/follow_gps/06_rotation_modes_ok.png)

All other screenshots (map zoom levels, route overlay, menus, settings
overlays, eco leaf, rotation, bearing, moving icons):
[`docs/pictures.md`](docs/pictures.md)
(Norwegian gallery: [`docs/bilder.md`](docs/bilder.md)).

## Documents

| Document | Description |
|---|---|
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | How to contribute (testing, plugins, jurisdictions, code expectations) |
| [`docs/architecture.md`](docs/architecture.md) | How the parts fit together (databases, threads, plugins) |
| [`docs/future-proofing-audit-2026-07.md`](docs/future-proofing-audit-2026-07.md) | Canonical 2026-07 future-proofing findings + risk-prioritized follow-up list |
| [`docs/status.md`](docs/status.md) | Which doc is canonical for live status vs historical evidence (anti-sprawl map) |
| [`docs/android-api36-plan.md`](docs/android-api36-plan.md) | Plan to raise compileSdk/targetSdk to API 36 (not executed yet) |
| [`docs/rust-crates.md`](docs/rust-crates.md) | Rust crates: first-party created vs crates.io used unaltered |
| [`docs/codebase-map.md`](docs/codebase-map.md) | Contributor file map: where to fix bugs, zoom, approach, routing, HUD |
| [`docs/pictures.md`](docs/pictures.md) | Emulator screenshot gallery |
| [`docs/bilder.md`](docs/bilder.md) | Emulator screenshot gallery (Norwegian) |
| [`docs/historical-background.md`](docs/historical-background.md) | Rast/vei basis for hiking & cycling rest-interval defaults |
| [`docs/ec-561-truck-rest.md`](docs/ec-561-truck-rest.md) | Truck EC 561/2006: duty caps, multi-day rest, compensation ledger, overnight scoring |
| [`docs/fmcsa-truck-rest.md`](docs/fmcsa-truck-rest.md) | Truck US FMCSA property-carrying HOS pack (11 h / 14 h / 8 h break / 70 h cycle) |
| [`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md) | Pattern for country/region-dependent rules (EC 561 + FMCSA + right-to-roam precedents) |
| [`docs/horse-profile.md`](docs/horse-profile.md) | Worked example: adding a Horse profile (doc only; not implemented) |
| [`docs/hud-layout.md`](docs/hud-layout.md) | Adjust size and placement of drive HUD bars and menus |
| [`docs/map-styles.md`](docs/map-styles.md) | Online Liberty vs offline Protomaps PMTiles; 3D gate |
| [`docs/approach-instructions.md`](docs/approach-instructions.md) | Temporary maneuver approach box (icon + distance + name) |
| [`docs/current-street.md`](docs/current-street.md) | Bottom-HUD “Currently on …” road name + no-route policy |
| [`docs/unicode-road-names.md`](docs/unicode-road-names.md) | UTF-8 pipeline for æ/å/ø/ä/ü in names (OSM → FTS → UniFFI → Compose) |
| [`docs/poi.md`](docs/poi.md) | Searchable POI categories (Fishing, RestArea, Lodging, …), OSM tag rules, how to add types |
| [`docs/poi-search-defaults.md`](docs/poi-search-defaults.md) | Suggested hut/trail POI search radii for hiking & cycling (DNT spacing) |
| [`docs/osm-updates.md`](docs/osm-updates.md) | Opt-in Geofabrik check / `.osc.gz` / full re-download |
| [`docs/plugins.md`](docs/plugins.md) | Plugin **host** status (intentional: no content plugins yet) + HostApi, isolation, roadmap ideas |
| [`docs/plugins/right-to-roam-camping-spec.md`](docs/plugins/right-to-roam-camping-spec.md) | Spec: allemannsretten / multi-country wild-camping suggestions (plugin, not core) |
| [`docs/plugins/safety-resupply.md`](docs/plugins/safety-resupply.md) | Spec: fuel/water resupply lookahead, POI confidence, remote/arid buffers (plugin, not core) |
| [`docs/plugins/instrument-cluster-agl-spec.md`](docs/plugins/instrument-cluster-agl-spec.md) | Spec: export nav state to clusters/AGL via VSS/Kuksa + JSON fallback (plugin, not core) |
| [`docs/plugins/i18n-translation-spec.md`](docs/plugins/i18n-translation-spec.md) | Spec: offline UI language packs (plugin; app UI is English-only today, no language toggle) |
| [`docs/plugins/animated-icons-spec.md`](docs/plugins/animated-icons-spec.md) | Spec: Synfig-authored animated icons / frame packs (plugin; static SVG stays in icons.md) |
| [`docs/icons.md`](docs/icons.md) | Icon inventory; custom static SVG (Inkscape); Navit GPL-v2 |
| [Supermagnum/road-signs](https://github.com/Supermagnum/road-signs) | Norwegian road-sign artwork (NLOD; separate repo) |
| [`docs/API.md`](docs/API.md) | UniFFI host API + plugin HostApi reference |
| [`docs/PROTOCOLS.md`](docs/PROTOCOLS.md) | Wire protocol index (UniFFI, plugins, ECU/APRS/CAT) |
| [`docs/ECU.md`](docs/ECU.md) | ECU protocols: OBD-II, J1939, MegaSquirt + EV SoC/power |
| [`docs/ebike-telemetry-diy.md`](docs/ebike-telemetry-diy.md) | Open wired DIY e-bike telemetry (`$NAVIPWR` over USB-serial; CAN optional) — spec only |
| [`docs/mathematical-formulas.md`](docs/mathematical-formulas.md) | Formulas: MAF/J1939/MegaSquirt fuel, range, eco segment energy |
| [`docs/APRS.md`](docs/APRS.md) | APRS fields, TrackStore range filtering, moving icons |
| [`docs/APRS-SDR.md`](docs/APRS-SDR.md) | APRS SDR DSP pipeline; RTL-SDR IF offset; planned `rtl-sdr-rs` |
| [`docs/CAT.md`](docs/CAT.md) | CAT VFO auto-tune from NFM repeaters (≤150 km); OSM network example |
| [`docs/voice-guidance.md`](docs/voice-guidance.md) | Planned voice guidance plugin (recordings + optional Piper) |
| [`docs/android-build.md`](docs/android-build.md) | Compile native `libnavi.so`, UniFFI bindings, and Gradle APKs |
| [`docs/build-linux.md`](docs/build-linux.md) | Linux: Rust core, `navi-desktop` map shell, integration tests, gpsd + IMU |
| [`docs/imu-calibration.md`](docs/imu-calibration.md) | Deferred: vehicle-mount IMU pitch/roll zeroing for eco elevation |
| [`docs/debugging.md`](docs/debugging.md) | Host + Android debug loops (logcat, Studio, instrumented tests) |
| [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md) | **Required:** physical device checklist vs emulator baseline |
| [`docs/test-results.md`](docs/test-results.md) | Host integration evidence (chronological; see [`status.md`](docs/status.md)) |
| [`docs/android-test-results.md`](docs/android-test-results.md) | On-device / emulator evidence (chronological; see [`status.md`](docs/status.md)) |

## Icons (Navit)

See [`docs/icons.md`](docs/icons.md) for the full icon system notes. Summary:
POI/maneuver/status icons under `core/src/icons` are Navit-derived (**GPL v2**).
Resolution prefers user overrides, then the bundled set, then `unknown.svg`.

**Custom icons:** use **SVG** (or `.svgz`). Author static art in
[Inkscape](https://inkscape.org/); name files after the semantic key and place
them in the override directory or `core/src/icons` — step-by-step in
[`docs/icons.md`](docs/icons.md#adding-custom-icons). Author animations in
[Synfig Studio](https://www.synfig.org/) and export SVG / frames — see
[`docs/plugins/animated-icons-spec.md`](docs/plugins/animated-icons-spec.md).

Related (not bundled in Navi): Norwegian road-sign artwork from the government
database, released under NLOD —
[Supermagnum/road-signs](https://github.com/Supermagnum/road-signs).

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

## Minimum hardware and storage capacity

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
- Where to edit features / fix bugs: [`docs/codebase-map.md`](docs/codebase-map.md).
- Callable APIs: [`docs/API.md`](docs/API.md).
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
  UI translation, ECU, marine, etc.) is intentionally deferred for independent
  contributors — specs live under [`docs/plugins.md`](docs/plugins.md). Not a
  defect in the navigation core.
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
- **Hydro soft-edge fringe in screenshots (lakes / rivers / creeks):** not a
  live user-visible rendering limitation. Direct comparison of the live app vs
  instrumented captures on the Automotive emulator shows the blue rim **only in
  screenshots** (`screencap` / UiAutomation), not during interactive use.
  Instrumented helpers now wait for MapLibre fully-rendered + idle before
  capture (`InstrumentedMapCapture` / `NaviMapTestHooks.renderSettleRequestId`) —
  same hygiene class as the moving-icons `styleReady` wait — but
  re-verification of the lake/river/creek matrix **still** shows the rim in
  fresh captures, so settle-wait alone is incomplete. `navi-hills` still
  inserts **below** hydro layers (real 3D stacking win: less hillshade-on-water
  darkening; earlier ~47% fringe-pixel drop on old captures remains a valid
  reorder benefit). Style paint experiments (`fill-antialias`, etc.) were
  no-ops for the capture artifact. Details:
  [`docs/map-styles.md`](docs/map-styles.md#hydro-soft-edge-fringe-screenshot-artifact).
- **Nightly instrumented crash on phone AOSP (not Automotive):**
  [Android instrumented run 30334233545](https://github.com/Supermagnum/Navi/actions/runs/30334233545)
  (`api-level: 30`, `target: default` → `system-images;android-30;default;x86_64`,
  SwiftShader) aborted at
  `ApproachInstructionInstrumentedTest.approachAppearAndUrgencyScreenshots`
  (position 3/50) with an empty `<failure/>` and
  `Instrumentation run failed due to Process crashed`. Emulator log showed
  MapLibre Vulkan `VkInstance`/`VkDevice` create then destroy within ~1 s
  immediately before the failure — consistent with a **native MapLibre Vulkan
  abort on that phone AOSP + SwiftShader profile**, not an empty-report-only
  mystery. Suite order: (1–2)
  `ApproachHideDistanceInstrumentedTest` (JNI / UniFFI only, no MapLibre), then
  (3) this first MapLibre Compose test. Local AAOS: tests 1–3 in that order and
  the full `ApproachInstructionInstrumentedTest` class **pass**; no fresh
  `/data/tombstones` from the passing repro (post-run Zygote `signal 9` is
  normal instrumentation teardown). **Not classified as “flaky under full
  suite load”** — predecessors leave no GL/MapLibre state, and the CI crash is
  on a device class this project’s Vulkan/hillshade work never validated.
  Nightly was retargeted to **API 33 `android-automotive`**. First Automotive
  confirmation run
  ([30363333076](https://github.com/Supermagnum/Navi/actions/runs/30363333076)):
  the **process-crash abort did not recur** (suite continued after approach);
  approach instead failed a width-fraction assert on the default ~320px-wide
  CI skin (`209px of 320px`). Nightly now uses `-memory 4096`, sets
  `wm size 1920x1080` after boot (do not use emulator `-skin` on this AVD),
  runs evidence collection via `scripts/ci-connected-android-test.sh` (the
  emulator-runner executes each script line as a separate `sh -c`), and the
  compact check follows the `420.dp` product cap (fraction only on wide
  screens). Phone-AOSP coverage remains an open, separate surface if we want
  broader CI later. See
  [`docs/debugging.md`](docs/debugging.md#nightly-instrumented-phone-aosp-crash).
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
