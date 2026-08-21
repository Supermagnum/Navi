# Right-to-roam overnight camping plugin (specification)

**Status:** specification only — not implemented.  
**Path:** `docs/plugins/right-to-roam-camping-spec.md`  
**Architecture:** WASM guest via `plugin-host` / `plugin-sdk` and capability-gated
`HostApi` ([`plugins.md`](../plugins.md)). No new core routing; the plugin
consumes position, route, POI/area, and safety config the host already exposes.
**System requirements** (all plugins): user **enable/disable** toggle; any
device link uses host-mediated **USB** / **Bluetooth**
([`plugins.md` — enable/disable](../plugins.md#enable--disable-required),
[USB/Bluetooth](../plugins.md#external-device-io--usb-and-bluetooth-required)).

Working title / id suggestion: `right_to_roam_camping` (or
`allemannsretten_camping` for a Norway-first packaging of the same guest).

---

## Disclaimer (must appear in the plugin UI)

This plugin provides **informational guidance** based on publicly described
right-to-roam / outdoor-access rules (including Norwegian *allemannsretten*).
It is **not legal advice** and **not a compliance guarantee**. Laws and local
practice change; municipal fire bans, private land, and seasonal restrictions
can be stricter than these summaries. **The user remains responsible** for
checking official sources and complying with the law where they camp.

The same disclaimer must be shown with every user-facing suggestion list (and
in any “about this plugin” screen).

---

## Goals

1. Suggest **legal overnight camping positions** along (or near) the active
   route, where a broad wild-camping right actually exists.
2. Prefer candidates derived from **road ∩ track intersection geometry** already
   present in the road network the core indexed — do not invent new paths.
3. Apply **country-aware** rule sets; never silently apply Norwegian rules
   outside Norway.
4. Present **safety and leave-no-trace guidance** with every suggestion, even
   when those rules are not algorithmic hard filters.

## Non-goals

- Implementing the plugin in this pass.
- Changing core A* / graph build / OSM ingest for the plugin’s sake.
- Re-parsing `.osm.pbf` inside the guest.
- Guaranteeing legal compliance or assessing “fire cannot spread” from map data.
- Suggesting wild camps under Nordic-style rules in **England/Wales** or
  **Denmark** (see country table).

---

## Host capabilities (proposed)

Implemented today (reuse): `log`, `position_read`, `poi_query` (and related
read paths as the host grows).  

Proposed additions (declare in manifest only after HostApi exists — see
[`plugins.md`](../plugins.md) capability sketch pattern):

| Capability | Purpose for this plugin |
|---|---|
| `position_read` | Current lat/lon |
| `poi_query` / area query | Buildings, cabins, overnight facilities, existing campsites; spatial index already built by core |
| `route_read` (new) | Active corridor polyline / sampled waypoints + optional nearby graph edges or intersection hints the host is willing to expose |
| `safety_config_read` (new) | Read `SafetyConfig.min_building_distance_m` (and related) — **shared** with core overnight safety |
| `admin_region_read` (new) | Country / county (or ISO region) for a lat/lon from host admin-boundary data |
| `clock_read` (new) | Current local or UTC date for fire-ban window |
| `plugin_kv` / `storage` (new) | Small plugin-local persist for the two-night rule |
| `log` | Diagnostics |

The guest must **not** open network or filesystem outside declared caps. Official
law text refresh is a host/documentation concern, not a silent WASM fetch.

---

## 1. Candidate-finding algorithm

### 1.1 Preferred seed: road ∩ track

**Preferred candidate seeds** are network nodes (or edge junctions the host
exposes) where:

- one incident way is a **“real road”** in the sense used here:
  - `highway=tertiary`, or
  - `highway=unclassified`, or
  - `highway=service` (see weighting below),
- and another incident way is `highway=track`.

**Reasoning:** a track leaving a real road is a common, realistic access point
to reach a camping area away from through traffic, without inventing geometry
the map does not already contain.

The plugin receives these junctions (or enough edge topology to detect them)
from the **host / core spatial index**, not from a second OSM parse inside WASM.

### 1.2 Weighting `highway=service`

Intersections involving `highway=service` must be **weighted lower** and used
**cautiously**. Core routing already treats many service ways as private
driveways or business access — not every service/track junction is legitimate
public access.

Specification requirements:

- Prefer tertiary/unclassified ∩ track over service ∩ track when ranking.
- Optionally require additional evidence before promoting a service seed (e.g.
  track continues beyond a short stub; not solely abutting a building footprint
  query). Exact signals are implementation detail; the requirement is that
  service seeds are not treated as equal to tertiary/unclassified seeds.
- Document in UI when a suggestion used a service-road access seed (optional
  transparency).

### 1.3 Seed ≠ campsite

The intersection is a **search origin**, not the overnight position.

Algorithm sketch:

1. Enumerate candidate seeds along the route corridor (bounded search distance
   from the route, host-defined).
2. Rank seeds (tertiary/unclassified ∩ track first; service last / downranked).
3. From each seed, **walk a short distance along the track** (away from the
   real road) — host path-follow or sampled points along the track edge —
   generating **probe points**.
4. At each probe point, run **country detection** and **hard filters** (§2–3).
5. Accept the first viable probe (or best-ranked viable set) as a suggestion.

Distance along track is a tunable (e.g. tens to a few hundred metres); the spec
requires “short walk off the road,” not camping on the carriageway.

### 1.4 Data ownership

| Data | Owner |
|---|---|
| OSM ingest, graph, POI/area R-tree | Core / host |
| Building distance default (150 m) | Core `SafetyConfig.min_building_distance_m` (`SAFETY_MIN_BUILDING_DISTANCE_M`) |
| Two-night stay memory | **Plugin-local** storage only |
| Candidate ranking / presentation | Plugin |

---

## 2. Allemannsretten / Norway rules (primary detailed set)

Apply when country detection resolves to **Norway**. Rules are classified as
**hard filter** (reject candidate) or **always-shown guidance** (do not filter
by map alone).

### 2.1 Distance from dwellings — hard filter (shared with core)

- **Minimum 150 m from inhabited houses and cabins** (and equivalent overnight
  building footprints the host indexes).
- **Must reuse** core `SafetyConfig.min_building_distance_m` (default
  `SAFETY_MIN_BUILDING_DISTANCE_M` = **150 m** in
  `core/src/config/defaults.rs`), via `safety_config_read`.
- **Do not** hard-code a second 150 m constant in the plugin. If the user or
  profile changes the core safety distance, the plugin follows that value.

Query buildings/cabins through the existing POI/area spatial index.

### 2.2 Duration — hard filter with plugin-local state

- **General rule:** not more than **two consecutive nights** in the same spot.
- Plugin persists (via plugin storage capability), per logical location:

  | Field | Meaning |
  |---|---|
  | `location_id` | Stable key (e.g. rounded lat/lon grid cell, or seed id + probe index) |
  | `first_night_date` | Local calendar date of the first night attributed to this spot |
  | `nights_used` or last night date | Enough to know if a third consecutive night would be suggested |

- **Reset:** when the user camps elsewhere (different `location_id`), or when
  more than one calendar night has passed since the stay without extending the
  consecutive sequence (implementation may treat “gap ≥ 1 unused night” as
  reset). After two consecutive nights at A, further suggestions for A must be
  suppressed until reset.
- This state is **not** core’s trip history table.

### 2.3 Fire safety

| Rule | Classification | Behaviour |
|---|---|---|
| **General fire ban 15 April – 15 September** in/near forests and other wilderness without municipal permission | **Date-gated guidance** (and optional soft warning flag) | At suggestion time, read **current date** (`clock_read`). Inside window: state that open fire is generally **prohibited without permission**. Outside window: state that fire is generally permitted with normal caution. **Live check** — not static baked text. |
| **Exception:** fire allowed in the ban window where it **clearly cannot spread** | **Informational only** | Plugin **cannot** verify “cannot spread” from map data. Always present as user judgment guidance, never as pass/fail. |
| **Do not light a fire on bare rock** (rock can crack from heat) | **Year-round informational** | Show whenever fire guidance is shown; not date-gated. |

### 2.4 Foraging

| Rule | Classification | Behaviour |
|---|---|---|
| Some rare berry, mushroom, and flower species are protected from picking | **Informational** | Standing note with Norway suggestions. |
| Northern Norway has special **cloudberry** picking rules | **Location-gated informational** | Show **only** when the candidate falls in **Nordland, Troms, or Finnmark** (host admin region). Do not show universally for all Norway. |

### 2.5 Leave no trace

| Rule | Classification | Behaviour |
|---|---|---|
| Clean up after yourself | **Informational** | Standing note with **every** suggested camping spot. |

---

## 3. Country-aware rule selection

### 3.1 Detection and default

1. Resolve country (and sub-region if needed) for the candidate via host
   admin-boundary / region data.
2. Select the rule pack below.
3. If country **cannot be determined confidently** → **decline** to suggest a
   wild-camping spot. Prefer “use a marked campsite / I cannot determine local
   rules” over guessing or falling back to Norway.

### 3.2 Reference table (re-verify periodically)

Legal frameworks change. Treat the following as a **starting reference** for
implementers; re-check each country’s official source before relying on the
plugin in production.

| Country | Local name / basis | Camping distance from dwellings | Duration limit | Notes for the plugin |
|---|---|---|---|---|
| **Norway** | *Allemannsretten* (Outdoor Recreation Act 1957) | **150 m** (shared `SafetyConfig`) | Generally ≤ **2** consecutive nights same spot | Primary detailed set (§1–2). |
| **Sweden** | *Allemansrätten* (constitution / Environmental Code) | ~**70 m** | One night generally; two in remote areas | Personal use; commercial foraging not covered. |
| **Finland** | *Jokamiehenoikeus* | No fixed legal metres; must not disturb residents | Short-term / temporary | Not a fully enumerated statute right; open fires generally need landowner permission — emphasize guidance. |
| **Scotland** | Scottish Outdoor Access Code (Land Reform Act 2003) | **No Nordic-style fixed metres** — “reasonable” access | Short-term wild camping | Use Code framing; do **not** reuse 150 m / 70 m filters as if they were Scots law. |
| **Iceland** | Nature Conservation Act / public access tradition | No fixed metres — no damage to nature | Short-term | Condition on not damaging/disturbing nature. |
| **Estonia** | *Igaüheõigus* | Nordic-like general access | Short-term | Flag: verify against Estonian sources before relying. |
| **England / Wales** | CRoW Act 2000 (limited) | N/A for general countryside | N/A | Access only on specific mapped open land types. **Do not** run Nordic intersection wild-camp logic by default. If ever supported: separate narrow ruleset gated to those land types / designated sites only. |
| **Denmark** | More limited public access | Wild camping generally **not** part of the right | N/A | Roaming forests/beaches ≠ Nordic wild camping. **Do not** suggest wild camps with Norway/Sweden/Finland logic. Designated sites or explicit decline only. |

### 3.3 Where wild-camp suggestions are allowed

**May** offer intersection/track-based wild-camping suggestions (with that
country’s filters and guidance):

- Norway, Sweden, Finland, Iceland, Scotland (Scotland with access-code wording).

**Must not** apply Nordic wild-camp suggestion logic:

- England / Wales, Denmark — suggest **designated / legal campsites** from POI
  data only, or clearly decline wild-camp suggestions.

Estonia: allow only after source verification; until then, treat like
“uncertain → decline wild camp.”

### 3.4 Design note (mandatory)

Right-to-roam is **not uniform** across the Nordics and is **substantially
different or absent** in England/Wales and Denmark. Silent reuse of
allemannsretten geometry + 150 m elsewhere is a **spec violation**.

---

## 4. Presentation requirements

For every accepted suggestion, show **in one place**:

1. Location (map pin / lat–lon / distance along route).
2. Access note (e.g. “via track from tertiary/unclassified/service”).
3. **All applicable rules** for that country/region — hard-filter outcomes
   already applied, **plus** fire, foraging, leave-no-trace guidance even when
   those were not used to reject the candidate.
4. **Fire guidance** computed from the **current date** at display time.
5. **Cloudberry** note only for Northern Norway candidates.
6. The **disclaimer** (§ top).

Ranking UI may show multiple candidates; each card must carry the full guidance
set for its location.

---

## 5. Filter vs guidance summary

| Item | Hard filter | Always / conditional guidance |
|---|---|---|
| Min distance from buildings (shared SafetyConfig) | Yes | Show value used |
| Two consecutive nights (plugin state) | Yes | Explain limit |
| Service-road seed quality | Soft rank / caution | Optional access note |
| Fire ban window | No (map can’t enforce permission) | Yes — date-gated text |
| Fire “cannot spread” exception | No | Yes — informational |
| No fire on bare rock | No | Yes — year-round with fire text |
| Protected species foraging | No | Yes |
| Cloudberry (N. Norway) | No | Yes — region-gated |
| Leave no trace | No | Yes — every suggestion |
| Unknown country | Decline suggestion | Explain decline |
| England/Wales, Denmark wild camp | Decline Nordic algorithm | Designated sites or decline |

---

## 6. State ownership summary

| Concern | Where it lives |
|---|---|
| Default / configured building distance (150 m in Norway default) | Core `SafetyConfig.min_building_distance_m` |
| Glacier / other overnight safety already in core | Core (plugin may query related POIs; does not fork constants) |
| Two-night consecutive stay tracking | Plugin-local KV/storage |
| Fire ban calendar logic | Plugin (uses host clock) |
| Country / Nordland–Troms–Finnmark | Host admin region read |

---

## 7. Implementation sketch (future; out of scope now)

1. Host exposes route corridor samples + optional junction list or edge pairs.
2. Guest ranks seeds, walks tracks, queries POIs for buildings, reads safety
   distance and admin region.
3. Apply country pack; persist two-night keys; format suggestion payload for UI.
4. Android (or other) host renders pins + rule text; no WASM UI.

---

## 8. Related documents

- [`plugins.md`](../plugins.md) — sandbox, capabilities, design rules  
- [`../jurisdiction-rules.md`](../jurisdiction-rules.md) — reusable country/region rule-pack pattern (this camping table is a grounding example)  
- [`../poi.md`](../poi.md) — POI categories / spatial index  
- Core: `SafetyConfig`, `SAFETY_MIN_BUILDING_DISTANCE_M`  
- [`../README.md`](../README.md) — Rest and overnight (*allemannsretten* default note)
