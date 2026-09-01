**[Dokument på Norsk](docs/Norwegian.md)**

# AI assistance

This project was built with help from AI tools (Cursor). The author has a
neurological condition related to dyscalculia that makes programming harder in a
way similar to how dyscalculia makes maths harder. AI was used to turn design
ideas into working code and docs. The author still chose the product rules,
reviewed the work, and ran the testing.
It is written in rust,so as many as standard crates are used. The AI assistant has written minimum of code to "tie" them together.
The crates used in the project is listed here:
https://github.com/Supermagnum/Navi/blob/dev/docs/crates.md


# NOTE !

Navi does not currently ship with a ready-made international routing database
the way many commercial GPS / head-unit products do (those usually include
precomputed indexes from the vendor).
Computing this on device takes anywhere from 8 minutes and up to 25 minutes, it needs to be done when you download or update map data for your region.
Solving onboard convert cost may involve
a server (example: navi.app) that distributes precomputed index packs
for a region; when that server is unreachable, the natural fallback is what
Navi already does — local pack convert and PBF planning from the
downloaded extract. Precomputed town-to-town corridors (e.g. Haugesund→Bergen,
Oslo→Fredrikstad) could speed popular trips further. Direction:
[`docs/precomputed-index-and-route-cache.md`](docs/precomputed-index-and-route-cache.md).

Estimated server space needed:
[https://github.com/Supermagnum/Navi/blob/main/docs/indexed-map-format-plan.md](https://github.com/Supermagnum/Navi/blob/main/docs/indexed-map-format-plan.md)

Background indexing is still slow on region-scale extracts, but improved
(Østlandet convert on SM-P613 ~14.8 → ~10.6 → **~7.4 min**). Of the reduced
~7.4 min total: graph build is the largest share (~58%), wetland extraction
~32%, and a shared POI + barrier/danger-geometry two-pass parallel PBF walk
~9% (~38 s; was ~3.6 min of mutex-wrapped re-reads). Those percentages are
shares of the new total, not a comparison with the old ~41/~24/~34 split of
10.6 min; POI+barrier dropped in absolute time. You can still plan while
indexing runs; plans are much faster once packs are ready. Cold /
missing-pack long-distance planning is still slow (PBF graph build). Pack-hit
planning is much faster — see [Known issues](#known-issues).

# Testers wanted

We need people to try Navi on **real devices** — car head units, tablets, and
phones. Reference checks so far: Samsung Galaxy Tab S6 Lite (**SM-P613**) and
Google Pixel 9a (**tegu**, phone cutout / API 36+). Cars and other shapes still
differ for GPS, maps, GPU, and layout. Checklist:
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md).
On-device and emulator results:
[`docs/android-test-results.md`](docs/android-test-results.md).

**Install the signed release APK.** Testers should download and sideload
[`compiled/navi-release.apk`](compiled/navi-release.apk) — a **properly signed,
installable release APK** (upload keystore; not the debug build). Current build:
**v0.2.0-alpha** (`versionName` 0.2.0, `versionCode` 2). Download from the
[`v0.2.0-alpha` tag](https://github.com/Supermagnum/Navi/tree/v0.2.0-alpha)
 Android
validates the APK signature on install; the separate GPG files
([`compiled/SHA256SUMS`](compiled/SHA256SUMS),
[`compiled/SHA256SUMS.asc`](compiled/SHA256SUMS.asc)) are optional provenance
only — not a substitute for APK signing. Install steps:
[Install a prebuilt APK](#install-a-prebuilt-apk).

**Translators wanted.** UI language packs are specified but not shipped
(English-only chrome today). Fill or review the working table and follow the
spec: [`docs/plugins/i18n-translation-spec.md`](docs/plugins/i18n-translation-spec.md)
(catalog: [`docs/plugins/translations.csv`](docs/plugins/translations.csv);
word/phrase context:
[`docs/plugins/translations-context.md`](docs/plugins/translations-context.md)).
The English column header lists countries/regions; dialect columns use
`country, - area, - dialect` (see the spec). Do not add a language toggle
until that plugin exists.

**How to help:** testers, docs, translations, and code all start in
[`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md). That page is the contribution
guide: how to **fork the repo and work on `dev`** (not `main`), basic GitHub
usage (clone your fork, sync with upstream, open a pull request against
`dev`), and which kinds of help are useful. Hardware testers can follow the
checklist above and file an issue; you do not need to write code.

## Table of contents

1. [What this is](#what-this-is)
2. [Support Navi](#support-navi)
3. [Features](#features)
   - [What you need to download](#what-you-need-to-download)
   - [Indexing (background after download)](#indexing-background-after-download)
   - [Leaving a downloaded region](#leaving-a-downloaded-region)
   - [How to use](#how-to-use)
   - [How features work](#how-features-work)
4. [Settings](#settings)
5. [Break timer vs trip ETA](#break-timer-vs-trip-eta)
6. [Routing safety](#routing-safety)
7. [Minimum hardware and storage](#minimum-hardware-and-storage)
8. [Screenshots](#screenshots)
9. [Documents](#documents)
10. [Code inspection / CI tests](#code-inspection--ci-tests)
11. [Plugins](#plugins)
    - [Icons (where they live)](#icons-where-they-live)
12. [Coding standards and contributing](#coding-standards-and-contributing)
13. [Building and installing](#building-and-installing)
    - [Install a prebuilt APK](#install-a-prebuilt-apk)
    - [Release build (APK / AAB)](#release-build-apk--aab)
14. [Where the map data comes from](#where-the-map-data-comes-from)
15. [Known issues](#known-issues)
16. [TODO](#todo)

More detail lives in linked docs (architecture, truck rest rules, map styles,
debugging, and so on). To contribute, start with
[`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md): fork from **`dev`**, GitHub
basics, and what to work on.

# What this is

**Navi** is an offline-first navigation app. You download map data once, then
plan routes on the device without needing the internet for every trip.

It can:

- Plan routes for car, bicycle, e-bike, hiking, motorcycle, truck, and motorhome
- For bicycle / e-bike, pick **Road / Gravel / MTB** so unsuitable tracks are skipped
- Prefer gentler / less energy-hungry roads when **eco mode** is on (hills matter)
- Suggest rest stops and overnight places along longer trips
- Respect truck driving-time rules where it knows the country rules
- Show a simple map with your route, turns, and place names
- Optional elevation **contours** and **hillshade** from Mapterhorn terrain (independent toggles)

The map picture on screen comes from MapLibre. Online it can use OpenFreeMap
Liberty tiles; offline it uses a downloaded regional map file (Protomaps).
Routing uses a separate OpenStreetMap extract you download in **Tools** — that
is the “brain” for finding roads and paths, not just the pretty map.

License: see `LICENSE` (GPL-3.0-or-later unless noted). Many small icons are
from Navit (**GPL v2**); see [`docs/icons.md`](docs/icons.md).

# Support Navi

Navi is free and open source, developed independently. If you'd like to
support its development, donations are welcome via direct bank transfer:

- **IBAN:** NO02 1802 0334 084
- **BIC/SWIFT:** SHEDNO22

Please include "Navi donation" as the payment reference/message, so it's
identifiable on your statement.

This is entirely optional support, not a paywall — Navi is and will remain free.

# Features

| Feature | In plain words | Status |
|---|---|---|
| **Travel modes** | Choose car, bike, e-bike, hiking, motorcycle, truck, or motorhome. | Done |
| **Vehicle size** | Save height/width/length/weight limits so the route skips roads that are too tight. | Done |
| **E-bike specs** | Battery size, motor torque, and wheel size help estimate battery use and steep climbs. Live cable telemetry is planned later. | Done (planning); live data later |
| **Avoidances** | You can ask to avoid motorways, tolls, or ferries. Motorways here means OSM `highway=motorway` / `motorway_link`, `motorroad=yes` / `expressway=yes`, or a dual carriageway with `lanes>=2` and `maxspeed>=90` — not every E-road or urban arterial. | Done |
| **Official trails** | For hiking/cycling, optionally prefer marked long-distance trails (off by default). Normal paths still work if the marked trail has a gap. | Done |
| **Bike surface suitability** | Bicycle / e-bike Drive setting: **Road / Gravel / MTB**. Unsuitable OSM surfaces and tracks are hard-excluded after the graph loads (does not rebuild packs). Default is Gravel (trekking). | Done |
| **Motor surface preference** | Car, truck, motorhome, and motorcycle: soft preference for good driveable surfaces (`surface` / `tracktype`) on connector snaps and along the route; untagged `highway=track` is treated cautiously. Internal costing only — no warnings in the UI. Default **Car**; **Offroad** / 4×4 relaxes the weighting (stored in config; no Drive-menu toggle yet). | Done |
| **Basemap POI icons** | Offline Protomaps amenity icons (fuel, hospital, alcohol shops, cycle/repair, and the rest of the allow-list) use dedicated sprites, not a generic dot. Kind list: [`docs/poi-icon-whitelist.md`](docs/poi-icon-whitelist.md). | Done |
| **Peak heights** | Named mountain peaks show OSM elevation on the label (metres, or feet in the US unit profile — UK stays metres, same as HUD altitude). | Done |
| **Glacier outlines** | Ice polygons get a teal dashed outline so they stay visible against pale fill (same dash as nature reserves, different colour). | Done |
| **Elevation contours** | Opt-in brown isolines from the same Mapterhorn DEM as hillshade (not OSM contour vectors). Independent of **3D**. Spacing follows Kartverket map-series intervals (minor / index 5×). Index lines labelled with elevation (metres, or feet in the US unit profile). Needs DEM: online tiles, or **Download terrain DEM** for offline. Off by default. Detail: [`docs/map-styles.md`](docs/map-styles.md#elevation-contour-lines-opt-in-independent-of-hillshade). | Done |
| **3D hillshade** | Optional hill shading from Mapterhorn DEM. Independent of contours — either, both, or neither. Offline needs **Download terrain DEM**. | Done |
| **Eco routing** | Prefer routes that use less energy by taking hills into account. A small leaf icon shows when eco is on. | Done |
| **Offline planning** | Download a region once, then plan and see the route on the device. | Done |
| **Indexing** | After a region download, a background job turns the OSM extract into compact routing packs so later plans are fast. You can plan while it runs; convert and place-index **pause** during a foreground plan so the PBF fallback is not starved. | Done |
| **Place search** | Search places and set From / Via / To. While the place index is still empty/building, search shows a building hint (coordinates and map tap still work). | Done |
| **Use GPS** | Fill From / Via / To from the live fix: coordinates appear immediately, then an optional nearby road-name upgrade. The field is the chip active when you tap — not whichever chip is selected after resolution finishes. | Done |
| **Map mark & saved places** | Hold on the map ~4 s to mark a point; set From / Via / To or save a named place (separate from Saved routes). | Done |
| **Off-route / reroute** | Sustained deviation shows **Off route**; motor profiles auto-replan from the live position (resolved start label); hiking prompts first. | Done |
| **Cancel planning** | While **Planning route…** or **Recalculating route…** is shown, **Cancel** stops the in-flight native plan. Recalc cancel keeps the original route. | Done |
| **Breaks & rest** | Reminds you when a break is due and can suggest stops. Cars use hours between breaks; hiking/cycling use rest distances; trucks use legal driving-time rules where known. What is searched and used as pause POIs: [`docs/poi.md`](docs/poi.md). | Done |
| **Drive bars** | Top: altitude (cutout-aware padding; metres, or feet in the US unit profile). Bottom: zoom, live GPS speed, posted limit when known, break timer, trip ETA, current street, eco leaf. Speed and distance follow **Display units**. Speed line turns the error colour when GPS speed is over the applicable limit (display only — not a spoken nag). | Done |
| **Display units** | Drive settings: **Metric** (km, km/h), **US** (ft / mi, mph, altitude ft), or **UK** (yd then mi, mph, altitude m). First install infers once from SIM/network country (GB → UK, US/LR/MM → US, else metric; emulators stay metric). The chips always override; the choice is never re-inferred on travel. Internal values stay metric. | Done |
| **GPS follow** | Map follows you by default. Pan away, then tap **Recenter**. | Done |
| **Map rotation** | North-up, compass, or direction of travel. | Done |
| **Moving icons** | Can draw nearby tracked markers on the map. A live radio feed is not built in yet. | Partial |
| **Seasonal road closures** | OSM `motor_vehicle:conditional` / `access:conditional` hard-filtered against the planned departure time (Car/Truck honour it; Hiking/Bicycle do not). Verified on Friisvegen (way `361797686`) on both bbox/PBF fallback and pack-hit (graph pack **v3**). Purely OSM-tag-driven — no jurisdiction pack. **v1 limitation:** multi-day trips that cross a season boundary are evaluated only at the planned departure instant (not re-evaluated day-by-day along the trip). | Done |
| **Norwegian road-sign warnings** | Vendored `NO:` catalogue approach icons in Norway; explicit OSM `traffic_sign` / `hazard` tags. Same 750 / 150 / 25 m approach phases as maneuvers. See [`docs/road-signs.md`](docs/road-signs.md). | Done |
| **Look forward** | Without a planned route, GPS position + heading look ahead **300 m** (±60°) for catalogue road signs, speed humps (`NO:109`), children facilities (generic **142**), opted-in speed cameras, and an upcoming posted speed-limit plate from the existing road-label cell graph — same approach box (750 / 150 / 25 m) and jurisdiction gates as the route-corridor path. Dedicated 362 plates cover every-5 km/h values including 12 generated plates; odd OSM speeds still snap to the nearest shipped plate. Compact points load once per region. Detail: [`docs/road-signs.md`](docs/road-signs.md). | Done |
| **Children facilities nearby** | When no tagged children / school sign is active, schools, kindergartens, and playgrounds still trigger a generic **142 Children** approach warning (nearest facility wins; tagged `NO:142` outranks this fallback). **With a planned route:** facilities within **200 m** of the corridor. **Without a route:** covered by **Look forward** (300 m cone). Detail: [`docs/road-signs.md`](docs/road-signs.md). | Done |
| **Speed camera warnings** | Point cameras use the existing approach distance-phase UX; average-speed / section-control zones use a distinct enter/exit box. `maxspeed:conditional` is evaluated against live local time. Jurisdiction-gated like EC561 / allemannsretten: Norway/UK opt-in (OSM-sourced, may be incomplete); Germany/France/Switzerland and unknown jurisdictions decline — see [`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md). First-run opt-in dialog required (not silently enabled). Works on both planned-route corridor and **Look forward**. | Done (display/warning only — no route-avoidance toggle, by deliberate product decision) |
| **Map updates** | Only when you ask — check for OpenStreetMap updates or download a fresh region. Never silent. On-screen copy is plain language (no internal planner dumps). | Done |
| **Cross-region / cross-country prompts** | Destinations outside downloaded data (including another country, e.g. Sweden) show **Map data needed** with the correct Geofabrik extract — not a silent partial route. Evidence: [`android-test-results.md` Item 10](docs/android-test-results.md#item-10--osm-update-copy-cross-region-prompts-expanded-catalog-2026-08-19). | Done |
| **Diagnostic logging** | **Tools → Diagnostic logging** (off by default). When on, writes a dated session log under **Internal storage → Documents → debug** (`navi_session_*.log`) for copy over USB/MTP — no adb required. Covers GPS, camera, toggles, route plan/stages, eco, POIs, pauses, instructions, fuel, system. Not uploaded. **Export diagnostic log** shares the latest file. Detail: [Settings → Tools](#tools-downloads-and-diagnostic-logging) and [`docs/debugging.md`](docs/debugging.md#3b-diagnostic-session-log-on-device-file). | Done |
| **Plugins** | A safe sandbox for future add-ons exists; product plugins are not shipped yet. | Host ready |

**Hardware note:** Real-device checks include Samsung Galaxy Tab S6 Lite
(**SM-P613**) and Google Pixel 9a. Car head units still need more real-world
testing before treating this as ship-ready. See [Screenshots](#screenshots),
[`docs/real-hardware-testing.md`](docs/real-hardware-testing.md), and
[`docs/android-test-results.md`](docs/android-test-results.md).

## What you need to download

Nothing useful ships pre-loaded. Use **Tools** in the app (with internet), then
you can go offline.

| Download | Need it? | What it is | Button in Tools |
|---|---|---|---|
| **Map region (roads & places)** | **Yes** for routing and search | OpenStreetMap extract from [Geofabrik](https://download.geofabrik.de/) (example path: `europe/norway/ostlandet`) | **Download region + build place index** |
| **Elevation** | Strongly recommended for eco / hills | Height data for the area | Usually comes with region provision |
| **Offline basemap** | Needed for map graphics without internet | Visual map tiles (Protomaps) | **Download basemap (PMTiles)** |
| **3D terrain** | Optional | Height tiles for hillshade **and** elevation contours | **Download terrain DEM (Mapterhorn)** |
| **OSM updates** | Optional | Fresher roads/POIs | **Check for OSM updates** (never automatic) |

**Minimum to plan a route:** region download + place index.  
**Minimum for a usable offline map picture:** that plus basemap PMTiles (or stay
online for Liberty).  
Prefer a **region** (not a whole huge country) on tablets with limited RAM —
see [Minimum hardware and storage](#minimum-hardware-and-storage).

After the region file is on disk, Navi **indexes** it in the background so later
plans are fast — see [Indexing (background after download)](#indexing-background-after-download).

## Indexing (background after download)

**Be patient — this takes time.** After you **download a new region** or **update
map data from the internet** (for example **Check for OSM updates** or a fresh
region download), Navi must build two things from the OpenStreetMap extract:
the **place index** (search names for From / Via / To) and, in the background,
the **indexed routing packs**. Both scan the full region file and can run for
many minutes on a large extract (Østlandet on a tablet is often on the order of
tens of minutes, sometimes longer). That is expected; leave the app open or
return to it later. You can still plan routes while work continues — planning
is just slower until indexing finishes.

When **Download region + build place index** has saved the OpenStreetMap
extract, Navi starts a **background indexing** job. That is not the map picture
on screen (basemap tiles) and not the raw `.osm.pbf` file itself — it is a
one-time conversion of that extract into compact **indexed packs** the planner
can load quickly instead of scanning the whole extract on every trip.

You can search and tap **Plan route** as soon as the download finishes. Until
indexing is done, planning uses the slower raw `.osm.pbf` path. Tools shows
progress as **Indexed maps (background)**; when it says **Indexed maps: ready
(pack-hit)**, the next plan uses the packs — typically about 1.5–2 seconds on
the reference tablet instead of tens of seconds.

While a **Plan route** (or auto-reroute) is running on that PBF fallback,
background convert and place-index **yield** so they do not contend for the
same extract. GPS-triggered speed-limit cone / road-near bbox builds **skip**
for that window (one missed HUD update) and resume on the next fix after the
plan finishes. Plan progress uses its own channel so convert/cone labels do
not move the plan bar. The UI may also remind you that planning is faster once
background indexing finishes.

What the background job writes:

| Pack | What it is for |
|---|---|
| **Road / path graph** | The network A* follows, per travel mode (car, foot, bicycle, and so on). Large regions are split into spatial tiles so a 4 GB-class device does not have to hold the whole region in RAM at once. |
| **POIs and barriers** | Huts, rest stops, lodgings, overnight buildings, glacier polygons, and similar features used for rest / overnight planning and hiking safety filters. |
| **Wetland** | Marsh and water polygons hiking uses to stay out of bogs (boardwalks stay on the graph). |

A separate **place index** (names for From / Via / To search) is built as part
of the download button, before this background job starts.

If packs are missing, stale, or still converting, planning still works via the
PBF fallback. Rebuild from a file already on the device with **Rebuild indexed
maps (local PBF, background)** — no re-download. More detail:
[`docs/indexed-map-format-plan.md`](docs/indexed-map-format-plan.md). Memory
margin on lower-end 4 GB devices during conversion: [Known issues](#known-issues).

## Leaving a downloaded region

A region download (OSM extract + indexed packs) and the offline basemap only
cover that extract’s area. Navi does **not** silently invent roads or tiles
outside it.

**Planning a trip that leaves your data.** Before **Plan route**, From / Via /
To are checked against the bounding boxes of downloaded Geofabrik extracts.

- If any waypoint is outside every downloaded area, planning is **blocked**
  (no partial or guessed route).
- A **Map data needed** dialog offers a suggested download (for example
  Vestlandet or Nord-Norge). You can download from there, or dismiss and pick
  another destination.
- If From and To need **different** landsdels (or similar splits), the prompt
  prefers a **country** extract (e.g. Norway). The planner uses a **single**
  region file and does not stitch two extracts into one trip.
- Cross-border destinations (e.g. Sweden) get a **country-specific** suggestion
  when the waypoint lies in another catalog entry — see
  [`android-test-results.md` Item 10](docs/android-test-results.md#item-10--osm-update-copy-cross-region-prompts-expanded-catalog-2026-08-19).

**Already navigating.** There is no continuous “you left the map” fence while
you drive.

- **Basemap:** tiles stop where the downloaded Protomaps region ends (or you
  fall back to online Liberty if the network is available).
- **Guidance:** keeps following the route you already planned while you stay
  on it.
- **Off-route recalculation:** uses the local region extract again. Outside
  that extract, snap / pathfinding can fail; you do **not** get the planning
  download dialog on auto-reroute. Download the covering region in Tools
  before you need to replan there.

Indexed packs match the extract they were built from. Leaving that area means
no offline graph for new plans — not a soft fade-out.

## How to use

Step-by-step end-user guide (planning, Tools, breaks, saved places/routes,
per-mode options, pilgrim coverage):
**[How to use Navi](docs/how-to-use.md)**.

A worked example of a multi-region road trip (OSM cider producers, south to
north): [`docs/cider-route.md`](docs/cider-route.md).

## How features work

**Planning a route.** Set **From** and **To** (and optional vias), pick a travel
mode, then **Plan route**. From is often set with **Use GPS** (select the
**From** / **To** / **Via** chip first; the button label follows the chip).
Hiking paths need the **Hiking** mode — planning with Car uses the road network
and will not follow foot trails properly. A **Planning route…** banner includes
**Cancel** if you want to stop an in-flight plan.

**Eco vs shortest.** Shortest ignores hills. Eco makes steep climbs “cost” more.
Electric modes get some credit for downhill recovery.

**Official networks.** Optional soft preference for marked hiking/cycle routes
(**Follow official hiking/cycling networks**). Ordinary paths remain available
so a gap never traps you. Separate toggles control **networked cabins** as
waypoints and whether you are a **network hut member** for overnight planning
(see [Drive / vehicle](#drive--vehicle-tap-bottom-status)).

**Motor surface quality.** For car-like profiles (car, truck, motorhome,
motorcycle), the planner prefers paved or well-graded connectors when a
better-surfaced snap point exists within budget, and applies soft edge costs
plus transition penalties when the route drops onto poor or unknown tracks.
This is silent — no pop-ups or log lines about surface class. Default mode is
**Car** (strict); **Offroad** / 4×4 turns the weighting off. Today the mode is
stored via the config API (`surface_routing_mode`); a Drive-settings chip is
not shipped yet.

**Contours and hillshade.** Tap the top bar: **Contours** draws elevation
isolines; **3D (experimental)** draws hillshade. They are independent — either,
both, or neither. Both use the same Mapterhorn DEM (online tiles, or
**Download terrain DEM** for offline). Interval ladder, index labels, and
flat-terrain limits: [`docs/map-styles.md`](docs/map-styles.md#elevation-contour-lines-opt-in-independent-of-hillshade).

**Places.** Search fills From / Via / To. What counts as a hut, rest area, and so
on is documented in [`docs/poi.md`](docs/poi.md).

**Map long-press and saved places.** Hold one finger on the map for about
**4 seconds** to mark a point, then set it as From / Via / To or save it under
**Saved places** (a single named coordinate — not a full **Saved route**).
How-to: [`docs/map-marking-saved-places.md`](docs/map-marking-saved-places.md)
(Norwegian: [`docs/kartmerking-lagrede-steder.md`](docs/kartmerking-lagrede-steder.md)).

**Rest and overnight.** Each mode has its own defaults. Long truck trips can
split into days with legal rest rules (EU or US packs where known). Long
car/bike/hiking trips can suggest overnight stops. Hiking overnight sites are
filtered away from buildings and glaciers (1 km to the glacier **polygon edge**);
rejected pins show a clear reason (for example `Excluded: within 1 km of a
glacier`). With **Network hut member** off (default), overnight prefers
non-network cabins; a DNT/STF-style network hut is only a last resort and is
labelled as membership-required. Membership does **not** grant entry — it only
changes overnight preference. The bottom-bar **Breaks** toggle only shows or
hides the reminder — it does not invent a new rest law.

**Map bars.** Tap the top bar for map/display settings. Tap the bottom status
area for drive/vehicle settings (mode, break interval, fuel, e-bike, and so on).
The bottom bar also shows **live GPS speed / posted limit** when a fix
and an applicable limit are known (km/h or mph per **Display units**); overspeed is a colour change only today
([`docs/current-street.md`](docs/current-street.md)). Spoken escalating
warnings are a **plugin spec**, not shipped:
[`docs/plugins/adaptive-speed-warning-spec.md`](docs/plugins/adaptive-speed-warning-spec.md).

**Children facilities nearby.** If OSM has no tagged children / school warning
sign, Navi still warns when a school, kindergarten, or playground is nearby —
generic sign **142**, same approach box timing as other road-sign warnings.
Along a **planned route** that means within **200 m** of the corridor; on a
**live drive without a route**, **Look forward** covers them inside the
**300 m** heading cone. An explicit tagged `NO:142` (or equivalent) wins over
this fallback. See [`docs/road-signs.md`](docs/road-signs.md).

**Look forward.** Without a planned route, GPS position + heading look ahead
**300 m** (±60°) and drive the same approach chrome for catalogue signs, speed
humps, children zones, opted-in cameras, and an upcoming posted speed-limit
plate from the existing road-label cell graph. Compact point sets load once per
region (not re-parsed every GPS tick). Detail:
[`docs/road-signs.md`](docs/road-signs.md),
[`docs/route-simulation.md`](docs/route-simulation.md).

# Settings

**Language:** the app chrome is **English only** today. There is no language
menu yet, and Navi does **not** pick UI language from GPS or SIM country (that
would override the language already set on the phone). Docs may exist in
Norwegian (`docs/Norwegian.md`); that is documentation, not an in-app language
pack. A future translation plugin (selectable packs, **fallback to English**
when a key is missing) is described in
[`docs/plugins/i18n-translation-spec.md`](docs/plugins/i18n-translation-spec.md).
A working CSV for translators lives next to that spec:
[`docs/plugins/translations.csv`](docs/plugins/translations.csv).
Sense notes for words and phrases:
[`docs/plugins/translations-context.md`](docs/plugins/translations-context.md).
English source strings list countries/regions in the column header; dialect
headers use `country, - area, - dialect` (documented in the i18n spec).

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
| **3D (experimental)** | Optional hill shading on the map (independent of contours) |
| **Contours** | Opt-in elevation isolines from the Mapterhorn DEM (independent of 3D; off by default) |
| **Map tilt** | Tip the camera (0° / 35° / 45° / 60°) |

### Drive / vehicle (tap bottom status)

| Setting | Plain meaning |
|---|---|
| **Travel mode** | Car, bike, hiking, truck, … |
| **Bike type** | Bicycle / e-bike: **Road / Gravel / MTB** surface capability (hard-excludes unsuitable tracks) |
| **Surface routing mode** | Car / truck / motorhome / motorcycle only: **`car`** (default) prefers good surfaces; **`offroad`** / **`4x4`** relaxes surface costing. Config/API today — not a Drive-menu chip yet |
| **Follow official hiking/cycling networks** | Hiking / bicycle / e-bike: soft preference for waymarked networks (ordinary paths still usable) |
| **Use networked cabins** | Hiking / bicycle / e-bike: allow DNT/STF-style **network** huts as auto-via / waypoint candidates (off by default). Does **not** change overnight membership rules |
| **Network hut member (DNT/STF/…)** | Hiking only: when on, overnight may prefer network huts; when off (default), prefer non-network cabins and flag network stops as membership-required |
| **Follow pilgrim routes** | Hiking only; soft preference (off by default), falls back to normal hiking |
| **Hours between breaks** | How often you *want* a break (cars), or truck mandatory break-after time |
| **Rest time** | How long a break should last (suggestion / truck continuous break) |
| **Next break as Time / Distance** | Show break countdown in minutes, or as km/mi at an assumed cruising speed |
| **Units** | Metric, US (ft / mph), or UK (mi / mph). First-install default from SIM/network country; always overridable. UK altitude stays metres (not US feet). |
| **Eco mode** | Hill-aware energy costing (locked on for hiking/cycling) |
| **POI search radius** | How far aside the planner may look for huts / stops |
| **Vehicle limits** | Height/width/length/axle weight for clearance |

Route planning chrome (**Route**): From / To / Via, Plan, Simulate, avoidances
(**Avoid motorways** excludes `highway=motorway` / `motorway_link`, `motorroad=yes` / `expressway=yes`, and dual carriageways with `lanes>=2` and `maxspeed>=90`; E-road `ref` is display-only),
saved routes.

### Tools (downloads and diagnostic logging)

Open **Tools** from the planning panel (same screen as region / basemap
downloads).

| Setting / action | Plain meaning |
|---|---|
| **Download region / basemap / DEM** | Offline map data (see [What you need to download](#what-you-need-to-download)) |
| **Pause / Resume / Cancel** | Control an in-progress download. Region downloads also **resume after a force-stop** (HTTP Range from the `.partial` file) instead of starting over |
| **Check for OSM updates** / **Apply pending** | Opt-in refresh; never silent auto-download |
| **Weekly update reminder** | Optional nag only — does not download by itself |
| **Diagnostic logging** | **Debug toggle** (off by default). When **on**, Navi appends a pipe-delimited **session log** on the device so you can diagnose planning, GPS, and setting changes without `adb logcat`. When **off**, no new session file is written and native per-stage route-plan timing stays gated off |
| **Export diagnostic log** | Opens the Android share sheet for the latest session file (or tells you to turn logging on first) |

**What the diagnostic log is for:** bug reports, planning timing (`ROUTE_PLAN` /
`ROUTE_PLAN_STAGES`), and confirming toggles/settings on real hardware. It is
**not** a crash dump mirror of logcat, and it is **not** uploaded by Navi.

**Where the file lives** (USB file transfer / MTP — no adb required):

```text
Internal storage → Documents → debug → navi_session_YYYY-MM-DD_HH-mm-ss.log
```

(Fallback: `Download/debug`, then app-private storage if Documents is not
writable.) Older sessions are rotated (last 10 kept). Full categories and
retrieval steps: [`docs/debugging.md`](docs/debugging.md#3b-diagnostic-session-log-on-device-file).

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
- Hiking overnight exclusion (buildings / glaciers) uses the downloaded OSM
  region and barrier pack, not the visual basemap tiles — those can disagree;
  see [Known issues](#known-issues).
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
| **GPU** | MapLibre GLES is the default path (verified on SM-P613 Adreno and Pixel 9a Mali). |

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

Offline Protomaps military landuse (muted red) and glacier fill, Rena / Gjende:

![SM-P613 offline Protomaps military landuse at Rena (z12)](docs/images/military-glacier/offline_pm_military_rena_z12.png)

![SM-P613 offline Protomaps glacier fill near Gjende (z12)](docs/images/military-glacier/offline_pm_glacier_gjende_z12.png)

More zoom ladder shots: [`docs/images/military-glacier/`](docs/images/military-glacier/).
Offline Protomaps also labels glacier **names** from ~z12 (`pois.kind=glacier`);
online Liberty still has fill-only ice (upstream OpenMapTiles gap). Details:
[`docs/map-styles.md`](docs/map-styles.md).

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
| [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) | How to help: fork from **`dev`**, GitHub basics (PR against `dev`), testing, docs, plugins, CI |
| [`CodeInspection.md`](CodeInspection.md) | Complete list of per-PR CI jobs and what each Rust/Kotlin test suite checks |
| [`docs/crates.md`](docs/crates.md) | First-party Rust crates created here, and unaltered crates.io dependencies |
| [`docs/architecture.md`](docs/architecture.md) | How the pieces fit together |
| [`docs/codebase-map.md`](docs/codebase-map.md) | Where to change code for a given feature |
| [`docs/pictures.md`](docs/pictures.md) / [`docs/bilder.md`](docs/bilder.md) | Screenshot galleries |
| [`docs/icons.md`](docs/icons.md) | Where icon files live, licensing, and how to add SVGs |
| [`docs/map-styles.md`](docs/map-styles.md) | Online vs offline map look; 3D hillshade; elevation contours |
| [`docs/poi-icon-whitelist.md`](docs/poi-icon-whitelist.md) | Which offline POI kinds draw, and which shop kinds are held back |
| [`docs/poi.md`](docs/poi.md) | Place types and search |
| [How to use Navi](docs/how-to-use.md) | End-user how-to (planning, Tools, breaks, saved places/routes, profiles) |
| [`docs/cider-route.md`](docs/cider-route.md) | Suggested **Norwegian Cider Route** (Siderveien): OSM `brewery=cider` stops south→north, with Navi region/leg notes |
| [`docs/road-signs.md`](docs/road-signs.md) | Norwegian road-sign catalogue, children-zone proximity, Look forward (300 m cone), approach phases |
| [`docs/map-marking-saved-places.md`](docs/map-marking-saved-places.md) | Map long-press (4 s) and Saved places detail (Norwegian: [`kartmerking-lagrede-steder.md`](docs/kartmerking-lagrede-steder.md)) |
| [`docs/ec-561-truck-rest.md`](docs/ec-561-truck-rest.md) | EU truck driving-time rules |
| [`docs/fmcsa-truck-rest.md`](docs/fmcsa-truck-rest.md) | US truck hours-of-service |
| [`docs/jurisdiction-rules.md`](docs/jurisdiction-rules.md) | Country/region rule packs |
| [`docs/osm-updates.md`](docs/osm-updates.md) | Opt-in map updates |
| [`docs/android-build.md`](docs/android-build.md) | Build/install Android APK (Linux, macOS, Windows hosts) |
| [`docs/build-linux.md`](docs/build-linux.md) | Linux host: tools, gpsd, adb, Android install |
| [`docs/build-macos.md`](docs/build-macos.md) | macOS host: tools, Android NDK, adb, Android install |
| [`docs/build-windows.md`](docs/build-windows.md) | Windows host: MSVC, tools, Android NDK, adb, Android install |
| [`docs/debugging.md`](docs/debugging.md) | Debugging |
| [`docs/real-hardware-testing.md`](docs/real-hardware-testing.md) | Physical device checklist |
| [`docs/android-test-results.md`](docs/android-test-results.md) | Chronological on-device / emulator instrumented evidence |
| [`docs/status.md`](docs/status.md) | Which docs are live status vs historical evidence |
| [`docs/future-proofing-audit-2026-07.md`](docs/future-proofing-audit-2026-07.md) | Tracked future-proofing / open risk items |
| [`docs/indexed-map-format-plan.md`](docs/indexed-map-format-plan.md) | Phased evaluation of preprocess-once indexed routing maps |
| [`docs/precomputed-index-and-route-cache.md`](docs/precomputed-index-and-route-cache.md) | Server / mirror of precomputed packs, commercial DB contrast, town-to-town route cache (direction; not shipped) |
| [`docs/plugins.md`](docs/plugins.md) | Plugin host and roadmap (enable/disable; USB/Bluetooth I/O) |
| [`docs/plugins/lora-convoy-spec.md`](docs/plugins/lora-convoy-spec.md) | LoRa convoy status over Meshtastic (Meshstick USB / BLE radio; not shipped) |

See the `docs/` folder for more specialised topics (voice, APRS, ECU, formulas,
and so on).

# Code inspection / CI tests

Per-PR GitHub Actions jobs and what each Rust / Kotlin test suite checks:
[`CodeInspection.md`](CodeInspection.md). Local commands before a PR:
[`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md#ci-expectations-github-actions).

# Plugins

A sandboxed plugin host exists so future add-ons can run safely. **No product
plugins ship in the app yet** — that is intentional. Overview:
[`docs/plugins.md`](docs/plugins.md). The system requires a per-plugin
**enable/disable** control, and host-mediated **USB** / **Bluetooth** I/O for
hardware-facing plugins.

| Spec | Topic |
|---|---|
| [`docs/plugins/i18n-translation-spec.md`](docs/plugins/i18n-translation-spec.md) | Future UI languages (English-only today). Translator table: [`translations.csv`](docs/plugins/translations.csv); context: [`translations-context.md`](docs/plugins/translations-context.md) |
| [`docs/plugins/right-to-roam-camping-spec.md`](docs/plugins/right-to-roam-camping-spec.md) | Wild-camping suggestions (plugin, not core) |
| [`docs/plugins/safety-resupply.md`](docs/plugins/safety-resupply.md) | Fuel/water resupply ideas |
| [`docs/plugins/traffic-information.md`](docs/plugins/traffic-information.md) | Traffic data sourcing research (DATEX II limits, RTL-SDR TMC/TPEG; not shipped) |
| [`docs/plugins/weather-plugin.md`](docs/plugins/weather-plugin.md) | Weather overlay (Meteocons icons vendored; guest not shipped) |
| [`docs/plugins/weather-icons-reference.md`](docs/plugins/weather-icons-reference.md) | What each weather icon slug means (fill style) |
| [`docs/plugins/instrument-cluster-agl-spec.md`](docs/plugins/instrument-cluster-agl-spec.md) | Export nav state + approach warnings to instrument clusters |
| [`docs/plugins/animated-icons-spec.md`](docs/plugins/animated-icons-spec.md) | Animated icons |
| [`docs/plugins/custom-alert-sounds-spec.md`](docs/plugins/custom-alert-sounds-spec.md) | Short alert tones (road signs, cameras, overspeed earcon) |
| [`docs/plugins/horse-trekking-spec.md`](docs/plugins/horse-trekking-spec.md) | Equestrian lookahead and access guidance (Hiking is the interim stopgap) |
| [`docs/plugins/adaptive-speed-warning-spec.md`](docs/plugins/adaptive-speed-warning-spec.md) | Spoken escalating overspeed (percentage tiers; not shipped) |
| [`docs/plugins/lora-convoy-spec.md`](docs/plugins/lora-convoy-spec.md) | LoRa convoy status over Meshtastic — Meshstick USB SX1262 stick or BLE node; location/speed/fuel/charge (not shipped) |
| [`docs/plugins/voice-command.md`](docs/plugins/voice-command.md) | Spoken navigate / save-place / nearest-POI alternative (on-device ASR/TTS; not shipped). Distinct from turn-by-turn [`docs/voice-guidance.md`](docs/voice-guidance.md) |

## Icons (where they live)

Map, turn, POI, and status icons are files in the repo — they are not generated
at runtime. Authoring, resolution order, and licences:
[`docs/icons.md`](docs/icons.md).

| Path | What is there |
|---|---|
| [`core/src/icons/`](core/src/icons/) | **Full set** (source of truth for desktop / core). Mostly Navit (**GPL v2**). Custom Navi files here include `leaf.svg` (eco) and `speed_camera.svg`. |
| [`app/src/main/assets/icons/`](app/src/main/assets/icons/) | Android **lean pack** — a size-trimmed copy shipped in every APK. Keys missing here fall back to `unknown.svg` on device. |
| [`core/src/icons/road-signs/`](core/src/icons/road-signs/) | Norwegian traffic-sign SVGs (**NLOD 2.0**, not Navit). Android copy: [`app/src/main/assets/icons/road-signs/`](app/src/main/assets/icons/road-signs/). |
| [`core/src/icons/aprs/`](core/src/icons/aprs/) | APRS moving-icon symbols. Android copy: [`app/src/main/assets/icons/aprs/`](app/src/main/assets/icons/aprs/). |
| [`plugins/weather/icons/`](plugins/weather/icons/) | Meteocons **static** weather SVGs (**MIT**). Animated SMIL set: [`plugins/weather/animated-icons/`](plugins/weather/animated-icons/). Spec: [`docs/plugins/weather-plugin.md`](docs/plugins/weather-plugin.md). |
| [`app/src/main/res/mipmap-*`](app/src/main/res/) | Android **launcher** (home screen / app drawer). Separate Navi brand art, not from Navit. |
| [`docs/icons/open-app.svg`](docs/icons/open-app.svg) | Splash / open-app brand mark (Inkscape source). Android drawables: `app/src/main/res/drawable/ic_splash*.xml`. |

To add or override a map/POI icon, put an SVG in `core/src/icons/` (and copy it
into the Android lean pack if it must appear on device). See
[`docs/icons.md`](docs/icons.md).

# Coding standards and contributing

Please read **[`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md)** first. It covers
how to **fork** [github.com/Supermagnum/Navi](https://github.com/Supermagnum/Navi),
check out **`dev`**, keep your fork in sync, and open a pull request with
**base branch `dev`**. Clone steps in [Building and installing](#building-and-installing)
are for a local build; they are not a substitute for a fork if you want to send
a change.

Short version of CI expectations:

| Area | Expectation |
|---|---|
| Rust | `cargo fmt`, Clippy with warnings denied, tests |
| Kotlin | ktlint, detekt, unit tests |
| Android | `./gradlew :app:assembleDebug` |

# Building and installing

Clone from GitHub first. Development is on **`dev`** (newest features); **`main`**
is the default clone target (a plain `git clone` checks out `main`). Install Git
if needed (`sudo apt install git` on Debian/Ubuntu — other systems in
[`docs/build-linux.md`](docs/build-linux.md#getting-the-code)).

**To build or install locally** (no pull request):

```bash
git clone https://github.com/Supermagnum/Navi.git
cd Navi
git checkout dev
```

**To contribute a change:** do not only clone this URL. Fork the repository,
clone **your** fork, and work on **`dev`** — step-by-step in
[`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md#fork-from-dev-and-basic-github-usage).

## Install a prebuilt APK

### Release APK (for testers)

Use the **signed release** build — this is what hardware testers should
install. The APK is signed with the project upload keystore so Android accepts
it as a normal install (not an unsigned or debug-only package).

| Artifact | Role |
|---|---|
| [`compiled/navi-release.apk`](compiled/navi-release.apk) | **Install this** — signed release APK (arm64, `versionName` 0.2.0 / tag **v0.2.0-alpha**) |
| [`compiled/SHA256SUMS`](compiled/SHA256SUMS) | SHA-256 checksum for integrity checks |
| [`compiled/SHA256SUMS.asc`](compiled/SHA256SUMS.asc) | Detached GPG provenance signature (not Android APK signing) |

You do not need a Rust/NDK toolchain to install it.

1. On the device: enable **Developer options** and allow installs from your
   browser or file manager (USB debugging only needed for `adb`).
2. Download
   [`navi-release.apk`](https://github.com/Supermagnum/Navi/raw/v0.2.0-alpha/compiled/navi-release.apk)
   (pinned tag) or the latest
   [`dev` copy](https://github.com/Supermagnum/Navi/raw/dev/compiled/navi-release.apk).
3. Optional integrity check on a PC:

```bash
cd compiled
sha256sum -c SHA256SUMS
gpg --verify SHA256SUMS.asc SHA256SUMS
```

4. If an older Navi build with a **different signature** is already installed,
   uninstall it first (`adb uninstall no.navi.app` or Settings → Apps).
5. Install and launch:

```bash
adb install -r compiled/navi-release.apk
adb shell am start -n no.navi.app/.MainActivity
```

On the device you can also open the raw download link in the browser and tap
the downloaded APK (allow installs from the browser if prompted).

This APK is signed with the project **upload** keystore (local sideload key —
not Play production signing). It is the intended tester build, not the debug
keystore.

### Debug APK (developers only)

A **debug-signed** APK remains in [`compiled/navi-debug.apk`](compiled/navi-debug.apk)
(arm64, same package as `./gradlew :app:assembleDebug`). Use it only for quick
local smoke tests when you are not exercising the release signing path.

```bash
adb install -r compiled/navi-debug.apk
adb shell am start -n no.navi.app/.MainActivity
```

Browser download:
[`compiled/navi-debug.apk`](https://github.com/Supermagnum/Navi/blob/dev/compiled/navi-debug.apk)
or
[`raw/dev/compiled/navi-debug.apk`](https://github.com/Supermagnum/Navi/raw/dev/compiled/navi-debug.apk).

To rebuild from source, follow the sections below.

## Android app (all host platforms)

The product APK is built the same way on **Linux**, **macOS**, and **Windows**:
compile `libnavi.so` with the NDK, then assemble/install with Gradle. Host-specific
setup (SDK paths, NDK clang, `adb`) lives in the OS guides; the shared recipe is
[`docs/android-build.md`](docs/android-build.md).

| Host OS | Install tools, NDK, `adb` | Then follow |
|---|---|---|
| Linux | [`docs/build-linux.md`](docs/build-linux.md) (SDK/`adb` sections + [Android install](docs/build-linux.md#build-and-install-the-android-app)) | [`docs/android-build.md`](docs/android-build.md) |
| macOS | [`docs/build-macos.md`](docs/build-macos.md) | same |
| Windows | [`docs/build-windows.md`](docs/build-windows.md) (use **Git Bash** for `scripts/*.sh`; `.\gradlew.bat` from PowerShell is fine) | same |

**Once per machine:** Rust Android targets, JDK 17, Android SDK (API 36), NDK,
and `ANDROID_HOME` / `ANDROID_NDK_HOME` (see the OS guide). Point
`.cargo/config.toml` linkers at your NDK’s host prebuilt (`linux-x86_64`,
`darwin-arm64` / `darwin-x86_64`, or `windows-x86_64`).

### Emulator (x86_64 image)

```bash
# From the repo root (bash: Linux/macOS Terminal, or Git Bash on Windows)
export ANDROID_HOME=…                 # see OS guide for typical path
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<version>"
rustup target add x86_64-linux-android   # once
./scripts/build-android-native.sh x86_64-linux-android release
./gradlew :app:assembleDebug          # Windows PowerShell: .\gradlew.bat …
./gradlew :app:installDebug
./scripts/launch-navi-emulator.sh
```

### Tablet / phone (arm64)

```bash
export ANDROID_HOME=…
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<version>"
rustup target add aarch64-linux-android   # once
./scripts/build-android-native.sh aarch64-linux-android release
./gradlew :app:assembleDebug
./gradlew :app:installDebug
# Equivalent:
#   adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n no.navi.app/.MainActivity
```

Confirm the APK contains the arm64 library:

```bash
unzip -l app/build/outputs/apk/debug/app-debug.apk | grep 'lib/arm64-v8a/libnavi.so'
```

On Windows PowerShell you can use Gradle install (`.\gradlew.bat :app:installDebug`)
or `adb install -r app\build\outputs\apk\debug\app-debug.apk`.

### Release build (APK / AAB)

Debug installs use the Android **debug** keystore. A **release** package is what
you sideload as release, hand to F-Droid-style checks, or smoke-test as an AAB.

A prebuilt upload-key-signed release APK for testers is committed at
[`compiled/navi-release.apk`](compiled/navi-release.apk) (tag **v0.2.0-alpha**;
see [Install a prebuilt APK](#install-a-prebuilt-apk)). To rebuild locally:

1. **Native library** for every ABI you ship (store AABs usually need both):

```bash
export ANDROID_HOME=…
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<version>"
./scripts/build-android-native.sh aarch64-linux-android release
./scripts/build-android-native.sh x86_64-linux-android release
```

2. **Local upload keystore** (optional but recommended for installable release
   APKs / AAB smoke). Creates gitignored `app/keystore/navi-upload.jks` — for
   **local testing only**, not Play production signing:

```bash
./scripts/make-upload-keystore.sh
```

   If that file exists, the `release` build type signs with it. Override
   passwords/alias with Gradle properties `navi.upload.storePassword`,
   `navi.upload.keyAlias`, and `navi.upload.keyPassword` if needed. Without a
   keystore, Gradle still produces release outputs, but they may be unsigned.

3. **Assemble**:

```bash
# Signed/unsigned release APK
./gradlew :app:assembleRelease
# → app/build/outputs/apk/release/app-release.apk

# Play-style app bundle
./gradlew :app:bundleRelease
# → app/build/outputs/bundle/release/app-release.aab
```

4. **Install a release APK** (signatures differ from debug — uninstall the debug
   build first if `adb` refuses the upgrade):

```bash
adb uninstall no.navi.app   # only if a debug/other-signed build is present
adb install -r app/build/outputs/apk/release/app-release.apk
adb shell am start -n no.navi.app/.MainActivity
```

5. **AAB smoke** (optional): validate / split / install with
   [bundletool](https://github.com/google/bundletool) as in
   [`docs/android-api36-plan.md`](docs/android-api36-plan.md#aab-smoke-host).

Current `versionName` / `versionCode` live in `app/build.gradle.kts`
(`0.2.0` / `2` at time of writing). Bump those before a real store or tagged
release. F-Droid-style Podman reproducibility:
[`tools/fdroid-check/README.md`](tools/fdroid-check/README.md). Full shared
recipe: [`docs/android-build.md`](docs/android-build.md).

## Desktop / core (optional)

| Host | Guide |
|---|---|
| Linux (`navi-desktop`, gpsd, core tests) | [`docs/build-linux.md`](docs/build-linux.md) |
| macOS (tools + optional desktop shell) | [`docs/build-macos.md`](docs/build-macos.md) |
| Windows (MSVC, tools + optional desktop shell) | [`docs/build-windows.md`](docs/build-windows.md) |

### Workspace layout

- `core/` — routing, places, rest rules, icons (Rust). Icon SVGs: `core/src/icons/`
- `navi-ffi/` — bridge to Android and other hosts
- `app/` — Android UI (Kotlin). On-device icon pack: `app/src/main/assets/icons/`
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
| Elevation | Public DEM tiles | Eco / hills; Mapterhorn DEM also drives hillshade and contours |
| Map picture | OpenFreeMap Liberty (online) or Protomaps PMTiles (offline) | What you see on screen |
| Position | Device GPS (or gpsd on Linux) | Where you are |
| Icons | Mostly Navit-derived SVG; custom `leaf` / `speed_camera` | Markers and turns |

Country/region visual extracts can also be prepared with
[PMT-splitter](https://github.com/Supermagnum/PMT-splitter/tree/main).

# Known issues

- **Background indexing is still slow on region-scale extracts, but improved.**
  On the reference SM-P613, full Østlandet convert dropped from about **14.8 min
  to about 10.6 min** after wetland single-pass tile assignment and a shared
  car+foot PBF parse (host ~143 s → ~110 s), then to about **7.4 min** after
  folding POI and barrier/danger-geometry extraction into one shared
  blob-parallel two-pass walk (`poi_ms≈38 s`, same 10870 POIs / 1073435
  barrier segs as before; peak RSS **1569 MB**, vs 1563 MB). Of the reduced
  ~7.4 min total: graph ~58%, wetland ~32%, POI+barrier walk ~9% (shares of
  the new total; the old ~41/~24/~34 split was of 10.6 min — POI+barrier
  dropped in absolute time, not relative to a slower extract). You can still
  plan while indexing runs; plans are much faster once packs are ready.
  Longer-term relief
  (precomputed packs from a mirror such as navi.app, plus optional town-to-town
  caches — Navi does not ship a commercial vendor routing DB today):
  [`docs/precomputed-index-and-route-cache.md`](docs/precomputed-index-and-route-cache.md).
- **Indexed-map progress bar is phase-based, not continuous.** Tools shows
  `Indexed maps (background): …` with a percentage that jumps once when each
  convert phase **starts** (graph tiling, POI + barriers at ~90%, wetlands at
  ~95%), not as work proceeds inside the phase. A new percentage means that
  phase has begun, not that it is actively advancing; a wedged phase leaves
  the bar frozen with no visual distinction from “almost done”. On the reference
  SM-P613 Østlandet convert, typical phase walls are roughly **POI + barriers
  ~38 s** and **wetlands a couple of minutes** (see the background-indexing
  bullet above); much longer at the same label usually means memory pressure or
  a competing PBF walk — force-stop and relaunch, or wait for pack-hit before
  planning.
- **Cold / missing-pack long-distance planning is still slow** (PBF graph build).
  **Pack-hit planning is much faster:** parallel tile mmap/deserialize cut host
  warm `graph_build_ms` by roughly **35–47%** on short/medium/long Ostlandet
  routes; on SM-P613 pack-hit short/medium/long warm walls were about **4.1 /
  8.4 / 5.4 s** (not the multi-minute PBF fallback). Details and older
  pack-hit vs pack-miss baselines are in the planning-latency bullet below.
- **Plugins:** content add-ons are intentionally not shipped yet, as they have
  not been made.
- **UI polish:** the screens work but still need visual tidy-up on car displays.
- **Moving icons:** drawn with a Compose overlay today; native map symbol layers
  are not the primary path yet.
- **Map / GPU quirks:** some emulator and phone GPU setups have historically
  crashed or washed out hillshade; the project defaulted to MapLibre GLES after
  SM-P613 and Pixel 9a checks. Details in [`docs/map-styles.md`](docs/map-styles.md)
  and [`docs/debugging.md`](docs/debugging.md).
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
  pack-conversion step. On ~4GB-class devices the tiled graph builder now spills
  filtered ways to the app data dir and keeps only routing-relevant tags in RAM
  (instead of holding every highway + full OSM tag map until the first tile),
  which previously caused LMK with zero `.rkyv` written. Further margin work
  (e.g. smaller tile cells) may still help on the tightest hardware.
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
  still converting in the background. `.navigph` deprecated (ignored on every
  plan — do not expect `.navigph` files to speed anything up).
  **Host re-check (release, Ostlandet, 2026-08-24):** without packs, Car
  Espa→Atnbrufossen ~**54 s** (`graph_build_ms≈29 s` + `poi_barrier_ms≈25 s`,
  `astar_ms≈0.3 s`, `pack_hit=false` both back-to-back runs); with packs
  ~**2.7 s** (`pack_hit=true`, `graph_build_ms≈1.5 s`, `poi_barrier_ms≈0.3 s`).
  **Host pack-hit after parallel tile load (2026-08-26):** warm `graph_build_ms`
  ~758 / ~1484 / ~983 ms on short / medium / long Ostlandet car routes
  (was ~1398 / ~2222 / ~1862 ms sequential).
  Hiking Skolla→Rondvassbu ~**104 s** without packs vs ~**25 s** with packs
  (`pack_hit` / `wetland_pack_hit` / `poi_pack_hit` true; remaining time is
  mostly `network_pref_ms` + `wetland_ms` + `multiday_ms`, not A*). Cabin
  prefs (`use_networked_cabins`) do **not** invalidate the graph cache.
  Region-scale packs now include tiled wetland + overnight buildings
  (POI/barrier v2): short Atnbrufossen hike on SM-P613 **159 s → ~3.1 s** with
  `wetland_pack_hit` and `overnight_buildings_pack_hit`. If planning feels
  extremely slow across modes, check Diagnostic logging for `pack_hit=false`
  and use **Tools → Rebuild indexed maps** (or wait for background convert
  after download). Plan-button session logs now include `pack_hit` /
  `poi_pack_hit` and `ROUTE_PLAN_STAGES` (same as off-route recalculation).
  Pixel UI multi-minute Espa→Atnbrufossen stalls (36 min / 193 s / 294 s /
  314555 ms) were a fixture-PBF pack-dir miss, not a planner bug — see
  [`docs/status.md`](docs/status.md) (2026-08-28). The UI status also warns
  when a completed plan used the PBF fallback. Concurrent convert / place-index / GPS cone PBF readers are
  coordinated during a foreground plan (pause / skip — see
  [Indexing](#indexing-background-after-download)); without that, pack-miss
  plans on a large extract can stretch into many minutes. Reproduce stages
  with Diagnostic logging → `ROUTE_PLAN` / `ROUTE_PLAN_STAGES`
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
- **Hiking plan speed on huge areas (addressed for packs):** overnight buildings
  use a 1.5 km corridor pre-filter over pack-stored centroids when POI/barrier
  v2 packs are ready (exact 150 m allemannsretten check unchanged). Historical
  DNT Åkersætra→Rondvassbu
  (~**139.9 km** corridor; `overnight_scan_bench`, debug): bbox-all
  **102 556 buildings / ~180.7 s** load → corridor **487 buildings / ~83.1 s**
  load. Graph/wetland/overnight pack-hit removes that PBF path when packs are
  ready (fallback remains if packs are missing or still converting).
- **Break timer ≠ trip ETA:** by design — see
  [Break timer vs trip ETA](#break-timer-vs-trip-eta).
- **Overnight glacier safety vs basemap can disagree.** Hiking overnight
  exclusion (`SAFETY_MIN_GLACIER_DISTANCE_M`, 1 km to glacier **polygon edge**)
  reads Geofabrik `.osm.pbf` / the `.navi-poi-barrier` pack. Offline map ice fill
  and glacier **name** labels come from a separately downloaded Protomaps
  PMTiles extract. Those pipelines update independently (different Tools actions
  and build dates). Confirmed on SM-P613 Ostlandet: PBF mtime vs PMTiles content
  date differed, and at Gjende (OSM way `380644665`) pack geometry and basemap
  `landuse.kind=glacier` tiles did not cover the same footprint. A tent site may
  therefore be excluded near ice the map does not show (or vice versa). The plan
  UI surfaces the reason explicitly (e.g. `Excluded: within 1 km of a glacier`)
  so the decision stays legible when map and pack disagree. Future consideration
  (not built): optional “refresh basemap after OSM update” prompt. See
  [`docs/poi.md`](docs/poi.md) and [`docs/map-styles.md`](docs/map-styles.md).
- **Finest contour interval on flat terrain is not fully verified.** At close
  zoom the 5 m / 25 m ladder can look sparse or miss rings on genuinely flat
  sites (DEM sample spacing vs isoline threshold). Mountainous areas (for
  example Gjendebu) are the regression site. Detail:
  [`docs/map-styles.md`](docs/map-styles.md#elevation-contour-lines-opt-in-independent-of-hillshade).
- **Online Liberty has no named glacier labels.** OpenFreeMap Liberty /
  OpenMapTiles expose ice as fill only (`landcover_ice`); there is no glacier
  POI name path to style. Navi adds a dashed ice outline for visibility.
  Offline Protomaps labels `pois.kind=glacier` from ~z12. Not a Navi Liberty
  regression — see [`docs/map-styles.md`](docs/map-styles.md).
- **Pilgrim stamp / credential offices have no stable OSM tag.** Official
  pilgrim centers (pilegrimspass / credencial stamp points) are tagged
  inconsistently: `tourism=information`+`information=office`, bare
  `building=office`, guideposts, or (Camino) `office=company`. Navi therefore
  has **no dedicated POI category** for them — a name matcher would mostly hit
  guideposts, and `information=office` would pull generic tourist offices.
  Pilgrim lodgings (`tourism=hostel` / `guest_house`) already match Lodging /
  Cabin. Named centers remain searchable via the place index when present.
  Upstream proposal under discussion:
  [proposal: pilgrimage=stamp_office and network=pilgrim](https://community.openstreetmap.org/t/proposal-pilgrimage-stamp-office-and-network-pilgrim/146371).
  Related `tourism=checkpoint` / `checkpoint:type=stamp` tagging does not close
  the gap (no reliable route-relation link, tourism-key collision, no
  pilgrimage semantics). Revisit a `PilgrimCenter` category if OSM settles on a
  consistent, machine-readable scheme. See [`docs/poi.md`](docs/poi.md) and
  [`docs/how-to-use.md`](docs/how-to-use.md#pilgrim-stops-and-stamp-centers-poi-coverage).
- **Not implemented yet:** checking whether the code can be optimised for
  rendering.

# TODO

(Future work only — shipped features are listed in the Features table above.)

**UI language packs** — chrome is English-only today. A future `i18n` /
`ui_translation` plugin will add selectable languages with **fallback to
English** when a translation or pack is missing. Spec:
[`docs/plugins/i18n-translation-spec.md`](docs/plugins/i18n-translation-spec.md).
Do **not** infer UI language from GPS or SIM country. Do not add a language
toggle until that plugin exists.

Figure out why railway lines is not showing.

Display **units** (metric / US / UK) are shipped (Drive settings), including
peak-height labels on the basemap. Norwegian speed-limit **pictograms** stay
official km/h plates; mph *plate artwork* is still future (`new-signs/`).

**Historical Norwegian/Norse distance units** as a selectable display option
(e.g. rast, dagsvei, fjerdingvei), alongside the existing Metric / US / UK
profiles. Display-only historical/cultural mode, not a default or a serious
alternative to metric/imperial for navigation — a novelty or
regional-flavor toggle. Needs a design pass before implementation (which
distance and speed fields it applies to, how it interacts with
`DisplayUnits`, whether it is era-specific or one canonical conversion set).

**Speed:** Old Norse **mil per hour** (using the younger Norse mile /
rast ≈ 9,100.8 m) could replace kilometre per hour in HUD / limit chrome for
this profile. **Smaller distances** (approach box, remaining distance under
a mil, etc.) should use the finer traditional units — stone's throw,
arrow's flight, fjerdingvei — not only mil / rast / dagsvei.

Context for whoever picks this up:

- A **rast** was the distance traveled on foot before needing a rest ("rast,"
  "pause"). It corresponded to a *mil* and was tied to the length of the ell,
  and varied by region and era. In the 900s, a rast was about 192 stone
  throws, divided into four **fjerdingvei** (quarter-ways), roughly
  **9,100.8 m**. By the 12th century it was expressed as 16,000 ells (four
  quarters of 8,000 feet), the same order of magnitude.
- A **dagsvei** (day's journey) was the traditional distance walkable in a
  day, commonly reckoned at about **40 km**.
- A **stone's throw** was 120 ells (also called a "great hundred") — about
  **56.88 m** (200 feet).
- An **arrow's flight** was 4 stone's throws, ~480 ells — about **227.52 m**
  (800 feet) around the year 900. Later in the Middle Ages, 10 arrow shots
  made up a **fjerding** of a mile — **2,275.2 m** (8,000 feet), a quarter of
  the younger Norse mile.
- The **younger Norse mile** (rast / vei) was **9,100.8 m** (32,000 feet) —
  the same order of magnitude as the 12th-century 16,000-ell figure above.
  Speed display in this mode: **mil/h** derived from that mile length
  (≈ 9.1008 km/h per mil/h), with sub-mil distances shown as stone's throw,
  arrow's flight, and so on.

Investigate if lake names can be displayed along the lake shore.


