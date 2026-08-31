# How to use Navi

End-user guide for the current Android app. Verified against the Compose UI in
`MainActivity.kt`, `DriveHud.kt`, and `MapLongPress.kt` (not older planning docs).

The on-screen language is **English only**. Navi does not pick UI language from
GPS or SIM country. Tap the **top bar** for map/display
settings; tap the **bottom status area** (street / speed / break / ETA text, not
the zoom −/+) for **Drive / vehicle** settings. Speed and posted limit live on
that bar ([`current-street.md`](current-street.md)) and follow **Units** in Drive
settings; spoken overspeed nags are
not in the app yet
([`plugins/adaptive-speed-warning-spec.md`](plugins/adaptive-speed-warning-spec.md)).
**Contours** (elevation isolines) and **3D (experimental)** (hillshade) are
independent toggles in map/display settings; both use the Mapterhorn DEM
([`map-styles.md`](map-styles.md#elevation-contour-lines-opt-in-independent-of-hillshade)).

---

## Plan a route

### Set From, Via, and To

1. Open route planning (search chrome). If it was closed, tap **Plan** (or the
   compact reopen control) so the panel with **From** / **To** / **Via** chips
   is visible.
2. Select the chip for the field you want to fill (**From**, **To**, or
   **Via**).
3. Choose one of:

| Method | How |
|---|---|
| **Keyboard search** | Type in the search field. Use **Place** (place, hut, or `lat, lon`) or **Address** (road, settlement, or `lat, lon`). Tap a result to apply it to the selected field. While the place index is still empty/building, the list shows a building hint instead of zero hits — use coordinates, map tap, or **Use GPS**. |
| **Use GPS** | Tap **Use GPS as from** / **as to** / **as via** (label follows the selected chip). Needs a device fix; otherwise status shows `GPS unavailable`. Coordinates appear immediately; an optional nearby road-name upgrade may follow. |
| **Map long-press** | Hold one finger on the map for **about 4 seconds** (blue ring). The **Marked location** sheet offers **Set as From / Start**, **Set as Via**, **Set as To / Destination**, **Save this place**, or **Cancel**. |
| **Saved place** | Open **Saved places**, then tap **From**, **Via**, or **To** on a row. |

The summary line under the chips shows the current From / To / Via names.
**Clear vias (N)** removes all vias.

Named hiking / cycle / pilgrim **routes** can appear in place search when they
are in the local place index — search by route name the same way as a hut or
town.

A suggested multi-region example (OSM `brewery=cider` producers, Rogaland to
Trondheim) is [`cider-route.md`](cider-route.md): paste `lat, lon` from that
table into Place search, and split the trip into legs per downloaded Geofabrik
region.

### Plan route

Tap **Plan route**.

- If From or To is missing, status becomes **`Set From and To first`** and
  nothing is planned.
- You need a downloaded region PBF (Tools). If none is found, status asks you
  to download a region (e.g. Østlandet) first.
- While planning, the button shows **Planning…** and a progress line/bar may
  appear (plan-only progress — convert / cone work use separate channels).
  A **Planning route…** banner includes **Cancel**, which stops the in-flight
  native planner.
- If the place index is still empty, searching a name shows
  **Place index is still building…**; use coordinates, map tap, or **Use GPS**
  meanwhile.
- **Use GPS** fills coordinates immediately, then may upgrade to a nearby
  road name when resolution finishes.

### Simulate route

**Simulate route** appears only on **debuggable** builds, and only when a
planned corridor with simulation samples is present. It walks the camera along
the planned path. A full-screen **SIMULATING** banner is shown; tap
**Stop simulation** (same button) to stop.

Release / store builds do not expose this control.

### Delete the planned route

With a corridor on the map, tap **Delete route** in the planning panel (or
**Delete planned route** under **Saved routes**). That clears the active
corridor so you can start over. It does **not** delete entries from the Saved
routes list unless you use **Delete route** on a saved row.

---

## Breaks: map toggle vs Drive settings

Two different controls — easy to confuse:

| Control | Where | What it does |
|---|---|---|
| **Breaks** switch | Top bar → map/display settings | Only shows or hides the bottom-bar **“Break in …”** reminder line. **Does not** change how often breaks are planned. |
| Break interval / rest fields | Bottom status → Drive / vehicle settings | The actual hours and rest durations used for planning and countdowns. |

### Soft profiles (Car, Car electric, Motorcycle, Motorcycle electric, Mobile home, Bicycle, Electric cycle)

In Drive settings:

- **Desired hours between breaks (Car)** — soft preference interval (label says
  “Car” even when another soft profile is active; values save as the Car
  profile default pack).
- **Rest time (minutes)** — suggested break length.
- **Next break shown as** → **Time** or **Distance**.
- **Units** → **Metric**, **US · ft / mph**, or **UK · mi / mph** (first install
  may infer from SIM/network country; chips always override).

These are preferences for reminders / soft multi-day overnight, not commercial
driving-hours law.

### Truck / Truck electric

Drive settings switch to truck wording:

- Hint: values save as **Truck EC 561/2006 defaults** (not a one-trip override).
- **Mandatory break after (hours, Truck)**
- **Break duration (minutes, continuous)**
- **Split break 15+30 min**
- **Arm +1 h exceptional extension**

Which legal pack applies (EU **EC 561**, US **FMCSA**, or **decline / unknown**)
is chosen automatically from the trip start location — there is **no** separate
jurisdiction picker in the UI. Outside recognised packs, truck multi-day legal
segmentation is not invented.

(The Tools **Download scope** country list is a Geofabrik extract picker only —
continent → country for offline maps — not a HOS/jurisdiction override. See
[`jurisdiction-rules.md`](jurisdiction-rules.md).)

Mobile home uses **car-style** soft breaks, not EC 561 tracking.

---

## Tools menu

Open **Tools** from the planning panel (toggles to **Hide tools**).

| Action | Meaning |
|---|---|
| **Download scope** | Chips **Country** vs **Region in country**. Country mode is a **continent → country** picker using the standard seven continents. Country paths and bboxes come from Geofabrik’s published index (`index-v1.json`); Central America extracts appear under North America in the UI while keeping `central-america/…` download paths. Antarctica is listed (Geofabrik root extract). Selecting a country shows an honest support note (most are maps-only; HOS/cameras only where packs exist). Region chips remain Norway landsdels only. Country-scale downloads warn about low-RAM devices. |
| **Geofabrik path** | Editable path (e.g. `europe/norway/ostlandet`, `europe/sweden`, `north-america/us`). |
| **Download region + build place index** | Downloads the Geofabrik PBF, binds the region, builds the place search index, and builds indexed routing maps when possible. |
| **Rebuild indexed maps (local PBF)** | Rebuilds preprocess packs from a PBF already on the device. |
| **Download basemap (PMTiles)** | Offline Protomaps basemap for the selected region. |
| **Download terrain DEM (Mapterhorn)** | Offline hillshade and elevation contours beside the basemap. Independent of the **3D** / **Contours** map toggles — download once, enable either or both. |
| **Pause / Resume / Cancel** | Controls an in-progress download job. |
| **Check for OSM updates** | Opt-in Geofabrik update check (never silent). |
| **Apply pending OSM update** | Applies a previously checked update after you confirm. |
| **Diagnostic logging** | **Debug toggle** (off by default). When on, writes a dated pipe-delimited session log under **Internal storage → Documents → debug** (`navi_session_*.log`) for USB/MTP copy — no adb required. Categories: GPS, camera, toggles, route plan/stages, eco, POIs, pauses, instructions, fuel, system. Not uploaded; when off, no new file and native per-stage plan timing stays gated off. |
| **Export diagnostic log** | Share-sheet export of the latest session file (enable logging first if none exists). |

A weekly OSM-update **reminder** may appear in Tools; it does not download by
itself.

Full retrieval and category detail:
[`debugging.md`](debugging.md#3b-diagnostic-session-log-on-device-file).

---

## Saved places

A **saved place** is one named coordinate (not a full route).

### Create

1. Long-press the map (~4 s) → **Save this place**, or
2. From the mark sheet, confirm/edit the **Name** → **Save**.

Stored in the app database (`saved_places`).

### Use, rename, delete

1. Open **Saved places**.
2. On a row: **From** / **Via** / **To**, **Rename**, or **Delete**.
3. Plan as usual.

The same place can be From on one trip and To on another.

More detail (gestures, screenshots): [`map-marking-saved-places.md`](map-marking-saved-places.md).

---

## Saved routes

A **saved route** stores start/end/via names and coordinates plus profile
metadata (via `saveNamedRoute`).

In the current UI under **Saved routes**:

- **Save** — stores the current From (or a fallback start), To, and vias.
  Requires a **To** destination (`Set a To destination first` otherwise).
- List of saved rows with profile and time.
- **Delete route** per row.
- **Delete planned route** clears the **active** map corridor only.

There is **no** “Load” / “Select” button today that restores a saved row into
From/To and replans. **Continue from last stop** (planning panel) can reuse a
saved route’s last break coordinates when present.

---

## Travel modes (what differs)

Open Drive settings (bottom bar) to change **Travel mode**, eco, POI radius, and
break fields. Avoidances and network toggles live in the planning **profile**
panel.

### Car / Car electric

- Road network planning.
- Soft **surface preference** (default **Car**): prefers good driveable surfaces
  for waypoint snaps and route legs; untagged `highway=track` is treated
  cautiously. No on-screen surface warnings. **Offroad** / 4×4 mode disables
  the weighting (config key `surface_routing_mode` via API — no UI toggle yet).
- **Vehicle** panel: **Height (m)** (cars).
- **Avoid motorways** / **Avoid toll roads** / **Avoid ferries**.
- **Eco mode** toggle (hill-aware energy costing). Car electric also has battery
  / efficiency fields in Drive settings.
- Soft break interval (see above). Soft multi-day overnight can use **Lodging**
  / camping / rest-area style stops from the POI index.

### Motorcycle / Motorcycle electric

- Plans on the **car** road graph (same builder class as Car).
- Same silent **surface preference** as Car (default **Car** mode).
- Avoidances like other motor profiles.
- **Eco mode** uses **motorcycle-specific** physics defaults (drag, frontal area,
  mass) — not the car Passat baseline.
- Soft breaks like Car (not truck HOS).

### Hiking

- Foot graph; wetland soft-avoid (bog/fen) and hard-avoid (swamp/reedbed) with
  **boardwalk** exceptions.
- **Eco mode** is locked on (not toggleable).
- **POI search radius** slider (Drive settings) for hut / stop search.
- **Follow official hiking/cycling networks** — soft preference for marked
  networks; ordinary paths remain available if the network does not connect.
- **Use networked cabins** — allow DNT/STF-style **network** huts as auto-via /
  waypoint candidates (off by default). Independent of overnight membership.
- **Network hut member (DNT/STF/…)** — overnight preference only (off by
  default). When off, prefer non-network cabins; network huts are last-resort
  and labelled membership-required. Does not grant cabin access.
- **Follow pilgrim routes** — **Hiking only** in the UI (soft preference; off by
  default). Matches `route=pilgrimage` and pilgrim-named hiking relations
  (Pilegrimsleden, Camino, Via Francigena, St. Olav, etc.). Falls back to normal
  hiking when no pilgrim way connects the points.
- Multi-day overnight / hut and via promotion follow hiking rest rules
  (allemannsretten-oriented overnight distance logic in core).

### Bicycle / Electric cycle

- **Bike type** (Drive settings): **Road / Gravel / MTB** — hard-excludes OSM
  ways whose `surface`, `tracktype`, or MTB tags exceed the selected capability.
- Same **Follow official hiking/cycling networks** and **Use networked cabins**
  toggles as Hiking.
- **Network hut member** and **Follow pilgrim routes** are **not** shown for
  Bicycle / Electric cycle (Hiking only).
- Eco locked on for cycling profiles.
- Electric cycle Drive fields: **Battery capacity (Wh)**, **Motor torque (Nm)**,
  **Wheel diameter (inches)** (presets + custom). Used for range / climbing
  reporting in plan status — legal assist caps are not enforced.

### Truck / Truck electric

- Same **surface preference** as Car (default **Car** mode; **Offroad** via config).
- **Vehicle** panel: axle / bogie weight, height, width, length.
- Avoid motorways / tolls / ferries.
- EC 561-oriented break fields (and automatic FMCSA / unknown pack selection by
  start country — see Breaks section).
- Multi-day day cards when a long truck plan segments overnight.

---

## Official networks and pilgrim routes

| Toggle | Profiles in UI | Behaviour |
|---|---|---|
| **Bike type** | Bicycle, Electric cycle | Hard filter by OSM surface / tracktype / MTB difficulty (`road` / `trekking` / `mountain`). |
| **Follow official hiking/cycling networks** | Hiking, Bicycle, Electric cycle | Soft cost preference for marked network ways; not a hard lock. Gaps fall back to ordinary paths. |
| **Use networked cabins** | Hiking, Bicycle, Electric cycle | Allow network huts as auto-via / waypoint candidates (off by default). Does not imply membership or overnight preference. |
| **Network hut member (DNT/STF/…)** | Hiking only | Overnight may prefer network huts when on; when off (default), prefer non-network and flag network overnight as membership-required. |
| **Follow pilgrim routes** | Hiking only | Soft preference for pilgrim route ways; falls back to normal hiking. |

Motor profiles (Car, Truck, Motorcycle, Mobile home) apply a separate silent
**surface routing mode** (`car` default, or `offroad` / `4x4` via config API).
That is soft costing for driveable surfaces — not shown in this toggle table
because there is no Drive-menu chip yet.

Search for a **named** route (including pilgrim route names) in **From** / **Via** /
**To** when that name is in your place index — independent of the toggle.

---

## Pilgrim stops and stamp centers (POI coverage)

Verified against OSM tagging and the Ostlandet extract, plus public OSM samples:

**Rest / overnight for pilgrims**

- Hostels and guest houses used as pilgrim lodgings (`tourism=hostel` /
  `guest_house`, often with “pilegrim” in the name) already match **Lodging**
  and (for hostels) **Cabin** / **OvernightFacility**.
- Examples in Ostlandet data: *Kongsveien Pilegrimsherberge*, *Pilegrimsloftet
  Borkerud*, *Lia Gård Pilegrimssenter* (`guest_house`), *Smedberget Pilegrimstun*.

**Stamp / credential / information centers**

- There is **no** single, widely used OSM tag for “pilegrimspass /
  Pilgerausweis / credencial office”.
- Ostlandet examples: *Pilgrim's center* as `tourism=information` +
  `information=office`; *Pilgrimssenter Oslo* often only as a **building**
  without `tourism` — so it is **not** in Lodging/Cabin/General today.
- Many “Pilegrimsleden” hits are **guideposts** (`information=guidepost` /
  `board`), not stamp offices.
- Nominatim did not resolve several official Norwegian center names as dedicated
  POIs; that is an **OSM data completeness** issue, not something Navi invents.
- **International:** Santiago’s *Oficina del Peregrino* is tagged
  `office=company` (not `tourism=hostel` / `information=office`). Camino
  *albergues* are typically `tourism=hostel` and already fall under **Lodging**.
  A Via Francigena *ostello* sample was likewise `tourism=hostel`.

**Product decision:** no new “Pilgrim center” POI category for now — tagging is
too inconsistent for a reliable matcher, while lodgings are already covered and
named centers remain findable via **place-name search** when present in the
index. See [`poi.md`](poi.md) and README [Known issues](../README.md#known-issues).
OSM tagging proposal:
[pilgrimage=stamp_office and network=pilgrim](https://community.openstreetmap.org/t/proposal-pilgrimage-stamp-office-and-network-pilgrim/146371).

---

## See also

- [`map-marking-saved-places.md`](map-marking-saved-places.md) — long-press detail
- [`poi.md`](poi.md) — POI category rules
- [`map-styles.md`](map-styles.md) — online Liberty vs offline Protomaps
- [`ec-561-truck-rest.md`](ec-561-truck-rest.md) / [`fmcsa-truck-rest.md`](fmcsa-truck-rest.md) — truck HOS detail
- README — install, download minimums, disclaimer
