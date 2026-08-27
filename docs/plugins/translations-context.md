# Translator context for `translations.csv`

**Status:** companion notes for human translators (not loaded by the app).  
**Catalog:** [`translations.csv`](translations.csv)  
**Plugin spec:** [`i18n-translation-spec.md`](i18n-translation-spec.md)

Use this file when a short English word could mean several things. Prefer the
**navigation / offline-maps** sense described here over a generic dictionary
sense. When unsure, keep the English proper noun or acronym and ask in a PR.

The CSV lists **words first**, then **full phrases**. Many short rows are
building blocks that also appear inside longer rows; translate consistently
across both.

---

## What Navi is (one paragraph)

Navi is an **offline-first Android navigation app** for car, truck, bicycle,
electric cycle, hiking, and similar modes. Users download **OpenStreetMap**
region extracts, plan routes (From / Via / To), follow a map, and see
reminders for **rest breaks**, fuel/battery range, and multi-day hiking or
truck stops. Map chrome is English today; this catalog is the future UI pack
source.

---

## Where strings appear

| Area | How the user opens it | Typical strings |
|---|---|---|
| **Map / display settings** | Tap the **top** status bar | Compass, Travel, N-up, Trip ETA, Breaks, Auto-zoom, 3D, tilt, hillshade |
| **Drive / vehicle settings** | Tap the **bottom** status / HUD | Travel mode, breaks/rest, units, eco, vehicle limits, e-bike specs |
| **Route panel** | Route / plan UI | From, To, Via, Plan, Simulate, avoidances, saved routes |
| **Tools** | Tools settings | Geofabrik download, PBF, place index, PMTiles basemap, DEM, OSM updates |
| **HUD / live drive** | Bottom bar while moving | ETA, Break in …, Currently on {street}, Arrived, Passed via |
| **Diagnostics** | Tools / debug | Diagnostic logging, export log under Documents/debug |

---

## Proper nouns and keep-as-is tokens

Do **not** invent local product names for these unless your language already
has a fixed conventional form. Prefer the English/OSM form in parentheses if
you must gloss.

| Token | Meaning in Navi |
|---|---|
| **OpenStreetMap** / **OSM** | Map data project and file formats derived from it |
| **Geofabrik** | Third-party host of regional OSM extracts and update feeds |
| **PBF** | Protocolbuffer Binary Format (OSM map file) |
| **PMTiles** | Cloud-optimized tiled basemap archive (Protomaps ecosystem) |
| **Protomaps** | Basemap / planet PMTiles provider |
| **MapLibre** | Map rendering engine used by the Android UI |
| **Liberty** | Default MapLibre basemap **style** name (not “freedom”) |
| **OpenFreeMap** | Alternative basemap source |
| **Mapterhorn** | Terrain **DEM** (elevation) tile provider for hillshade |
| **DEM** | Digital Elevation Model (height grid for 3D / hillshade) |
| **GLES** / **Vulkan** / **GPU** | Graphics APIs / hardware path for map rendering |
| **GPS** | Device location fix |
| **POI** | Point of interest (hut, rest stop, search hit, …) |
| **EV** | Electric vehicle |
| **SoC** | State of Charge (battery %), planning estimate — not a legal SoC meter |
| **ETA** | Estimated time of arrival (trip remaining), not “break in …” |
| **Ostlandet** / **Østlandet**, **Sørlandet**, **Vestlandet**, **Trøndelag**, **Nord-Norge** | Norwegian region names (Geofabrik paths / UI examples). Keep as place names; do not “translate” into another country’s regions |
| **Norway** | Country name in download / region UI |
| **Documents/debug** | Android Documents path segment for session logs — keep path-like form |

---

## Placeholders

Leave placeholders intact and in the same order the grammar of your language
allows. Do not translate the names inside braces.

| Form in CSV | Runtime meaning |
|---|---|
| `{minutes}`, `{meters}`, `{degrees}`, `{count}`, `{km}`, `{mi}`, `{viaName}`, `{street}` | ICU-style / host substitution |
| `$street`, `$unit`, `$mapLayerCount`, `${…}` | Legacy Kotlin-style fragments still listed for coverage — treat like `{…}` placeholders |

Example: `Break in {minutes} min` → translate “Break in” and the unit word; keep `{minutes}`.

---

## High-risk short words (ambiguous English)

These are easy to mistranslate. Use the **Navi sense**.

| English | Navi sense | Not this |
|---|---|---|
| **Arm** | Verb/noun for **arming** the truck “+1 h exceptional extension” (EC 561-style exception). UI: `Arm +1 h exceptional extension`. | Body limb; weapon |
| **Bind** / **binding** | Link a downloaded extract to a **Geofabrik region** identity so updates work. | Tie rope; book binding |
| **Blank** | Empty URL field (`Planet PMTiles URL (blank = latest Protomaps)`). | Empty map visually |
| **Bogie** | Truck **bogie** (axle group) weight limit. | Informal “truck” slang elsewhere |
| **Break** / **Breaks** | Driver/hiker **rest break** reminder and planning (bottom-bar toggle, “Break in …”). | Shatter; coffee break as social chat; line break |
| **Build** | **Build place index** after downloading a region PBF. | Construction industry |
| **Check** | **Check for OSM updates** (Tools). | Restaurant bill; checkmark alone |
| **Clear** | **Clear vias** (remove intermediate waypoints). | Clear weather; clear cache generically |
| **Compass** | Map rotation mode: rotate map with device heading. | Magnetic compass gadget sales |
| **Continue** | **Continue from last stop** (resume multi-leg / overnight plan). | Media “play” only |
| **Could** | Only in error phrasing (`Could not delete saved route`). | Modal verb table filler |
| **Country-scale** | Whole-country Geofabrik extracts (heavy on low-RAM devices). | Political “scale” |
| **Custom** | User-supplied values / URL / path. | Customs border |
| **Desired** | Preferred hours between breaks (car comfort), not law. | Romantic desire |
| **Diagnostic** / **Diagnostics** | Optional **session logging** and export under Tools (**Diagnostic logging** debug toggle). Check session logs for `pack_hit=false` when planning feels extremely slow. | Medical diagnosis |
| **Download** / **Downloading** | Fetching PBF / PMTiles / DEM (user-started, never silent OSM weekly download). | Generic app store |
| **Drive** | **Drive / vehicle settings** panel (bottom bar), not “USB drive”. | Hard disk; motivation |
| **Eco** | **Eco mode / eco routing**: hill-aware energy / effort costing. Locked on for hiking/cycling. | Generic “ecology” label without routing sense |
| **Enter** | Type a value into a field (`Enter a Geofabrik path…`). | Go into a building |
| **Extract** / **Extracting** | Regional **map extract** (PBF) or range-extract from planet PMTiles. | Dental extract; vanilla extract |
| **Follow** | **Follow official hiking/cycling networks** (prefer waymarked networks). | Social-media follow |
| **From** / **To** / **Via** | Route endpoints and intermediate waypoints. | Mail “from”; via = “by means of” only |
| **Height** / **Width** / **Length** | **Vehicle** clearance dimensions (metres), not map zoom. | Person height alone |
| **Hillshade** | Shaded-relief overlay from DEM (3D / terrain look). | “Hill” + “shade” as two words casually |
| **Index** | **Place search index** built from the region extract. | Stock market index |
| **Layers** | MapLibre **map layers** count / stack. | Clothing layers |
| **Mandatory** | Truck **mandatory break after (hours)** (legal-style default pack). | Optional UI chrome |
| **Mode** / **Modes** / **Travel mode** | Routing profile: Car, Truck, Bicycle, Hiking, … | Airplane mode; UI dark mode |
| **Motor** | E-bike **motor torque (Nm)**. | Motorway abbreviation |
| **Motorways** | Avoidance of OSM motorway-grade roads (`highway=motorway` / `motorway_link`, `motorroad`/`expressway`, or dual carriageway at 90+ km/h). | Any busy road |
| **Networks** | Official **hiking/cycling** waymarked networks (and related cabin networks in product docs). | Cellular network |
| **Next** | **Next break** display (time vs distance). | “Next” wizard page only |
| **N-up** | Map rotation: **north up** (north always top of screen). | “N” as variable; bed “n-up” |
| **Off** / **On** | Toggle state. | Off as “discount” |
| **Official** | Official trail / cycling **networks** preference. | Government letterhead |
| **Optional** | Feature may be skipped; also opt-in updates. | Optional type in programming docs |
| **Passed** | HUD: user **passed** a via point (`Passed {viaName} — continuing`). | Died; legislation passed |
| **Path** | (1) Geofabrik **download path** (`europe/norway/ostlandet`); (2) **path/trail** OSM link required for hiking graph. | Filesystem only |
| **Pause** / **Paused** / **Resume** / **Resuming** | **PMTiles** download job control. | Media player only |
| **Pending** | Staged **OSM update** waiting for user Apply. | Pending friend request |
| **Place** | Searchable place / POI index (“build place index”). | “Place” as verb put-down |
| **Plan** | **Plan route** (compute). | Business plan |
| **Planet** | Whole-world **Protomaps planet** PMTiles source. | Astronomy alone |
| **Primary** | OSM **primary** road class (avoidance string with motorway/trunk/primary). | Primary school; primary button |
| **Profile** | Active **travel / vehicle profile** settings. | User social profile |
| **Radius** | **POI search radius** beside the route for huts/stops. | Geometry class |
| **Range** | (1) EV/bike **energy range** estimate; (2) HTTP **range** extract of PMTiles. | Mountain range |
| **Recenter** | Move map back to GPS / route. | Re-centre politically |
| **Reminder** | Weekly **OSM update reminder** (no auto-download) or break reminder line. | Calendar RSVP |
| **Require** / **Required** | **Require path / trail link** (hiking: stay on routable path edges). | HTTP required field alone |
| **Rest** | Length of a **rest break** in minutes. | Remainder; musical rest |
| **Restore** / **Restoring** | **Restore staged offline maps** after update/replace. | Backup app brand |
| **Road** | **Road link required** (routing must use road/path edges as configured). | “Road” as journey metaphor |
| **Route** | Planned line / saved route / delete route. | Router hardware |
| **Run** | **Run Check for OSM updates first…** (perform the check). | Jogging |
| **Secondary** | OSM **secondary** road class (if present in avoid/list UI). | Secondary school |
| **Services** | Map / stop services context where used; not Android system services. | Church service |
| **Set** | **Set From and To first** / **Set a To destination first**. | Mathematical set |
| **Simulate** / **Simulation** / **SIMULATING** | Replay GPS along the planned route for testing ([`../route-simulation.md`](../route-simulation.md)). | Flight sim game |
| **Splash** | Startup splash screen (if shown in catalog). | Liquid splash |
| **Split** | **Split break 15+30 min** (truck rest split pattern). | Split screen |
| **Stop** | Overnight / last **stop** on a multi-day plan (`Continue from last stop`). | Stop sign alone; halt button only |
| **Street** | Current road name on HUD (`Currently on {street}`). | Street as pave type |
| **Tank** | **Fuel tank capacity**. | Military tank |
| **Terrain** | Terrain DEM download / relief. | Terrain as “area” vaguely |
| **Tilt** | Map camera **tilt** in degrees (3D). | Pinball tilt |
| **Toll** / **Tolls** | Avoid **toll roads**. | Bell toll |
| **Tools** | Settings section for downloads and OSM maintenance. | Hand tools shop |
| **Torque** | Motor torque in **Nm** (e-bike). | Torque wrench brand |
| **Travel** | Map rotation mode: keep direction of **travel** up; also **Travel mode**. | Tourism brochure |
| **Trip** | **Trip ETA** (remaining time to destination). | Trip as stumble |
| **Trunk** | OSM **trunk** road class (high-class non-motorway). | Tree trunk; luggage trunk |
| **Unit** / **Units** | Display **unit system** (metric / US / UK) or fuel unit label. | Apartment unit |
| **Upload** | Rare/export paths if present — not silent telemetry of tracks. | Cloud backup marketing |
| **Use** | **Use GPS** (fill From/Via/To from location). | “Use” license |
| **Used** | Explainer: capacity **used for route range estimate only**. | Second-hand |
| **Vehicle** | Vehicle limits / drive settings. | Any “vehicle” metaphor |
| **Weekly** | **Weekly OSM update reminder** (opt-in nag; nothing downloaded automatically). | Weekly magazine |
| **Wheel** | **Wheel diameter (inches)** for e-bike planning. | Wheel of fortune |
| **Writes** | Diagnostic option: **Writes a dated session log under Documents/debug**. | Literary writings |
| **Zoom** | Map zoom / **Auto-zoom**. | Video call zoom product |

---

## Units and abbreviations

| Token | Meaning |
|---|---|
| **km** / **mi** / **Miles** / **Meters** (via `{meters}`) | Distance; follow the active **Units** setting in surrounding phrases |
| **min** / **Minutes** / **Hours** / **Seconds** | Time |
| **kg** | Mass (axle / bogie weight) |
| **kWh** / **Wh** | Battery energy |
| **Nm** | Newton-metres (torque) |
| **Gallons** / **Liters** | Fuel volume (US vs metric wording) |
| **Imperial** / **Metric** | Unit-system names in the Units picker (UK is a third option in product docs) |
| **Alt** | Short **altitude** label on HUD (`Alt {meters} m`, `Alt --`) |

Altitude in UK display mode stays **metres** (product rule); do not force feet into UK strings unless the English source does.

---

## Phrase groups (translate with the same story)

### Map rotation and 3D

- **Compass** / **Travel** / **N-up** — mutually exclusive map orientation modes.
- **3D (experimental)** / **Hillshade** / **Map tilt** / **Vulkan** / **Liberty** — optional relief; may fall back to 2D Liberty when Vulkan/GPU path is unavailable.
- Strings like `3D requires Vulkan renderer`, `Tilt locked at 0 without Vulkan`, `Unavailable on this GPU path` are **capability errors**, not user mistakes.

### Breaks vs trip ETA

These answer **different** questions on the bottom bar:

| Phrase | Meaning |
|---|---|
| **Trip ETA** / `ETA {minutes} min` / `ETA --` / `ETA off` | Time left to **destination** |
| **Breaks** toggle / `Break in {minutes} min` / `Break in {km} km` / `Break reminders off` | Time or distance until the next **rest break reminder** |
| **Desired hours between breaks** | Comfort interval (car) |
| **Mandatory break after (hours)** | Truck default after-driving hours |
| **Rest time (minutes)** | How long the break should last |
| **Split break 15+30 min** | Truck split-rest pattern |
| **Arm +1 h exceptional extension** | Opt into exceptional +1 hour extension (truck rules pack) |
| **Next break shown as** | Show break countdown as time or as distance |

Do not merge “break” wording into “ETA” wording in languages where one word covers both.

### Routing and modes

- **Plan route**, **Simulate route**, **Saved routes**, **Delete route** / **Delete planned route** — planning and storage of the active line.
- **Avoid ferries** / **Avoid toll roads** / **Avoid motorways** — costing flags (OSM motorway class, `motorroad`/`expressway`, or dual carriageway with `lanes>=2` and `maxspeed>=90`), not map filters that hide geometry.
- **Eco mode** / **Eco routing** — energy/effort-aware costing; may be locked on for some profiles.
- **Follow official hiking/cycling networks** — soft preference for waymarked networks.
- **Use networked cabins** — allow network (DNT/STF-style) huts as auto-via /
  waypoint candidates only; off by default; does not grant membership.
- **Network hut member (DNT/STF/…)** — hiking overnight preference; off by
  default prefers non-network cabins and labels network overnight as
  membership-required.
- **Require path / trail link** — hiking graph must use path/trail-linked edges.
- **POI search radius (active profile)** — how far aside the planner may search for stops/huts.
- **Multi-day** — overnight / day-split plans (truck or long hiking), not “multi-day calendar UI”.
- **Continue from last stop** — resume after an overnight or saved last stop.
- **Arrived at destination** / **Passed {viaName} — continuing** — live guidance state.

### Vehicle, fuel, and e-bike

- **Vehicle limits** — height, width, length, axle/bogie weights for clearance.
- **Fuel tank capacity** / **Fuel added** — ICE range planning inputs.
- **Electric car** / **Battery capacity** / **Example default 60 kWh…** — EV pack size for **estimate only**, not a measured SoC instrument.
- **Electric cycle** / **Motor torque** / **Wheel diameter** / legal-assist caps text — planning specs; EU/US legal assist limits are **not enforced** by the router (as the English sentence states).

### Tools: downloads and OSM updates

- **Download scope (Geofabrik)** / **Geofabrik path** / **Region in country** / **Download region + build place index** — offline routing data install.
- **Download basemap (PMTiles)** / **Basemap (PMTiles) — range-extract…** / pause-resume-cancel PMTiles strings — visual basemap, separate from routing PBF.
- **Download terrain DEM (Mapterhorn)** — elevation for hillshade.
- **Check for OSM updates** / **Apply pending OSM update** / **Weekly update reminder (no auto-download)** — **opt-in**; weekly reminder must never be translated as if the app already downloaded something.
- **Restore staged offline maps** — recover staged data after an update flow.
- **This extract has no Geofabrik binding…** — region identity missing; user must bind or re-download.
- **Country-scale extracts may be slow or fail on low-RAM devices** — warning, not an error code name.

### Status and errors

Keep severity: **Failed**, **Unavailable**, **Could not…**, **No region PBF —…**, **Set From and To first** are blocking or actionable. **Pending**, **Paused**, **Resuming…**, **Finished**, **Saved** are progress/state.

---

## Consistency checklist

1. Same English word → same chosen term in your language across the CSV (especially **Break**, **Route**, **Via**, **Extract**, **Update**).
2. Do not translate **placeholder names** or **file/format acronyms** listed above.
3. Norwegian region names stay region names.
4. Prefer natural short HUD labels where the English is already abbreviated (**Alt**, **ETA**, **N-up**), but keep meaning obvious on first use in settings phrases.
5. When a phrase embeds a short word that has its own CSV row, reuse your translation of that row.

---

## Related reading

- Product overview: [`../../README.md`](../../README.md) (Settings, Break timer vs trip ETA)
- Norwegian product docs: [`../Norwegian.md`](../Norwegian.md)
- Route simulation: [`../route-simulation.md`](../route-simulation.md)
- Map styles / Liberty / PMTiles: [`../map-styles.md`](../map-styles.md)
- POI meanings: [`../poi.md`](../poi.md)
