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

**Coverage:** Europe in detail; North America (USA, Canada) and Russia with
first packs; selected Latin American and Asian protected-area packs. **Every
country, region or territory without a pack is Tier D** (decline wild camp,
campsites only). See §3.2 and the world Tier D table.

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
4. Apply **land-manager-aware** rule sets where the right to camp depends on who
   manages the land rather than on the country (USA, Canada, Russia — §3.2.1).
5. Present **safety and leave-no-trace guidance** with every suggestion, even
   when those rules are not algorithmic hard filters.

## Non-goals

- Implementing the plugin in this pass.
- Changing core A* / graph build / OSM ingest for the plugin’s sake.
- Re-parsing `.osm.pbf` inside the guest.
- Guaranteeing legal compliance or assessing “fire cannot spread” from map data.
- Suggesting wild camps under Nordic-style rules where the country’s pack is
  Tier C or Tier D (see §3); designated sites or decline only in those cases.
- Deriving vehicle overnight suggestions from right-to-roam packs (see §3.5).
- Inferring land tenure from OSM tags alone where an authoritative tenure
  layer is required (§3.2.1).

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
| `admin_region_read` (new) | Must return ISO 3166-2 subdivisions: German Länder, Austrian Bundesländer, Swiss cantons, Norwegian counties, US states, Canadian provinces/territories, Russian federal subjects, plus the national park core for France. Must return the **own ISO 3166-1 code** for territories that have one (SJ, AX, FO, GL, IM, JE, GG, GI) — never the parent state's code (see §3.1) |
| `clock_read` (new) | Current local or UTC date for fire-ban window and date-gated packs |
| `plugin_kv` / `storage` (new) | Plugin-local persist for max_nights per pack, DE vehicle same-spot key, and US/CA per-area stay counters (§3.2.1) |
| `protected_area_query` (new) | Is the point inside a national park, nature reserve or other protected area (OSM `boundary=protected_area`, `boundary=national_park`, `leisure=nature_reserve`)? Where needed, also return the park's core/zone and managing operator |
| `land_tenure_query` (new) | Who manages the land at a point, from an **authoritative tenure layer** (not OSM guesswork): `{ manager_type, manager_name, unit_id, unit_name, sub_unit }`. Examples: USA → BLM / USFS / NPS / state / tribal / private (PAD-US `Mang_Type`, `Mang_Name`); Canada → provincial Crown land / Parks Canada / ZEC / wildlife reserve / PLUZ; Russia → land category (e.g. forest fund, protected natural territory, defence lands) and border-zone flag. Return `unknown` when no layer covers the point |
| `landcover_query` (new) | Is the point in forest (`landuse=forest`, `natural=wood`), on farmland or pasture, on beach or dune, or on open alpine land above the treeline? The treeline is **not** a fixed altitude — do not hard-code one |
| `travel_mode_read` (new) | Non-motorised (foot/bicycle/horse/canoe) vs motorised |
| `vehicle_profile_read` (new) | `{ class: car \| campervan_motorhome \| caravan_combo \| hgv, gross_weight_kg, is_professional_driver_under_rest_rules }`. The last field is set by the user in the vehicle profile; never infer it from size. Reuse Navi's existing vehicle profile if one exists |
| `traveller_profile_read` (new) | `{ residency_country }`, **set by the user**, never inferred from position or locale. Needed where rules differ by residency (Ontario Crown land, §3.2 Canada) |
| `route_destination_read` (new) | The final destination of the active route (needed for the German vehicle rule) |
| `log` | Diagnostics |

**Designated-site layers** (paalkamp poles, bivakzones, Trekkingplätze, Danish
fri-teltning forests, Polish "Zanocuj w lesie" areas, Norwegian NVDB rest
areas, US/Canadian park backcountry zones, Alberta camping-pass boundary) are
**host-side POI/area data**. Never fetch them from inside WASM.

**Land-tenure layers** (PAD-US, provincial Crown land atlases, Russian land
categories and border zones) are **host-side data** with the same rule.

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
4. At each probe point, run **country detection**, **land-tenure detection**
   where the pack requires it (§3.2.1), and **hard filters** (§2–3).
5. Accept the first viable probe (or best-ranked viable set) as a suggestion.

Distance along track is a tunable (e.g. tens to a few hundred metres); the spec
requires “short walk off the road,” not camping on the carriageway.

**Pack-specific walk distances override the tunable.** Some packs require a
minimum distance from roads that is far longer than the default walk (e.g.
Alberta: no camping within 1 km of a road in named Public Land Use Zones;
Parks Canada random-camping zones measured in kilometres). Where a pack sets a
minimum road distance, probes closer than that are rejected; if the track does
not reach that far, the seed yields no suggestion.

### 1.4 Data ownership

| Data | Owner |
|---|---|
| OSM ingest, graph, POI/area R-tree | Core / host |
| Building distance default (150 m) | Core `SafetyConfig.min_building_distance_m` (`SAFETY_MIN_BUILDING_DISTANCE_M`) |
| Land-tenure layers (PAD-US, Crown land, RU land categories) | Host |
| Two-night stay memory / per-area stay counters | **Plugin-local** storage only |
| Candidate ranking / presentation | Plugin |

---

## 2. Allemannsretten / Norway rules (primary detailed set)

Apply when country detection resolves to **Norway (mainland, ISO NO)**. Rules
are classified as **hard filter** (reject candidate) or **always-shown
guidance** (do not filter by map alone). **Svalbard and Jan Mayen (ISO SJ) are
not covered by this section** — see §3.2 territories.

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
   `admin_region_read` (ISO 3166-2 where required).
2. **Territories with their own ISO 3166-1 code resolve to that code, never to
   the parent state.** SJ (Svalbard and Jan Mayen) is not NO; AX (Åland) is
   not FI; FO (Faroe Islands) and GL (Greenland) are not DK; IM, JE, GG and GI
   are not GB. Northern Ireland (GB-NIR) is not England/Wales or Scotland.
   Each of these has its own row in §3.2 and is Tier D until a pack exists.
3. If the pack is **land-manager-keyed** (§3.2.1), also resolve
   `land_tenure_query`. Tenure `unknown` → treat as Tier D for that point.
4. Select the rule pack below by **tier**.
5. If country or region **cannot be determined confidently**, or the pack is
   **Tier D** → **decline** wild-camp suggestions and suggest campsites only.
6. **Unknown country or region → Tier D. Never fall back to Norway.**
7. The Norwegian 150 m / 2-night / 15 Apr–15 Sep logic is **never** reused for
   another country. Each pack declares its own distance ("none in law" is a
   valid value), duration, fire rule and sources.

### 3.2 Tent / on-foot rule packs

Legal frameworks change. Treat the following as the implementer reference;
re-check each country’s official source before production use. Every row has
a **Sources** column. Full URL list with verification dates:
[`docs/jurisdiction-sources.md`](../jurisdiction-sources.md). **Every URL added
in this revision must also be added there.**

Source quality labels used below: **official** (government, statute,
agency); **NGO** (e.g. Alpenverein, SAC); **secondary** (press, blogs,
commercial guides) — secondary sources may justify a TODO, never a hard rule.

#### Rule-pack tiers

| Tier | Meaning |
|---|---|
| **A – general right** | The road∩track wild-camp algorithm may run, with the country's own filters. |
| **B – conditional** | The algorithm runs only when every listed condition is host-checkable and passes. Each Tier B pack sits behind a **maintainer flag that defaults to OFF** until its conditions are implemented. |
| **C – designated sites only** | Never run the wild-camp algorithm. Suggest only designated sites from POI/area data. |
| **D – not verified** | Decline and suggest campsites only. |

### 3.2.1 Land-manager-keyed packs (USA, Canada, Russia)

In Europe the tier is chosen by **country or region**. In the USA and Canada
(and, differently, Russia) there is **no country-wide right to roam**: whether
you may camp depends on **who manages the land** at the probe point. These
packs therefore key on `(country, land_tenure_query.manager_type,
manager_name, sub_unit)` rather than on country alone.

Rules for all land-manager-keyed packs:

- **Tenure must come from an authoritative host layer.** OSM `boundary` /
  `protect_class` / `operator` tags are **not** sufficient to establish that a
  point is, e.g., BLM land or unreserved Crown land. No layer → Tier D.
- **Private land, tribal / First Nations land, state/provincial land without a
  pack, and `unknown` → Tier D.**
- **Posted closures override everything.** The host cannot see signs, closure
  orders or fire restrictions; every card says "posted closures, fire
  restrictions and local orders override this suggestion".
- **Sub-unit orders vary.** US field offices and national forests, and Canadian
  PLUZs / RCMs, publish their own stay limits and distances. The pack stores a
  **conservative default** plus the text "check the local order for
  {unit_name}"; a sub-unit override table may be added later with its own
  sources.
- **Stay counters are per pack**, stored in plugin KV, and may need a radius
  (US BLM) or a calendar-year window (Ontario), not only a spot key.
- All of these packs are **Tier B, maintainer flag default OFF**, until the
  tenure layer and counters exist.

---

## Europe

#### Norway — Tier A

Clarification: *allemannsretten* covers people on foot, not vehicles. Motorised
travel in the outfield is regulated separately → vehicles use §3.5 only.
Detailed hard filters and guidance remain in §2. Applies to mainland Norway
(NO) only; Svalbard and Jan Mayen (SJ) → see territories below.

| Field | Value |
|---|---|
| Distance | 150 m via shared `SafetyConfig` (§2.1) |
| Duration | Generally ≤ 2 consecutive nights same spot (§2.2) |
| Fire | Date-gated 15 Apr–15 Sep (§2.3); forskrift om brannforebygging § 3 |
| Sources | Friluftsloven https://lovdata.no/dokument/NL/lov/1957-06-28-16 ; Motorferdselloven https://lovdata.no/dokument/NL/lov/1977-06-10-82 ; Fire rules https://lovdata.no/dokument/SF/forskrift/2015-12-17-1710 |

#### Sweden — Tier A

| Field | Value |
|---|---|
| Distance | No statutory distance and no statutory size of the home-privacy zone. Pitch well away from homes, out of sight of their windows. The plugin reuses the shared SafetyConfig distance, labelled **"Navi safety default, not Swedish law"**. |
| Duration | No statutory limit; the official rule of thumb is "a single day or so". |
| Filters / notes | Not on farmland, pasture or plantations. National parks, nature reserves and municipal rules may ban tents. |
| Sources | https://www.naturvardsverket.se/allemansratten ; handbook PDF https://prod-egp.naturvardsverket.se/497366/globalassets/vagledning/allemansratten/material/handbok-gora-allemansratt-a4.pdf |

#### Finland — Tier A

Applies to mainland Finland (FI) only; Åland (AX) → see territories below.

| Field | Value |
|---|---|
| Distance | not verified (no fixed metres in this pack); not in yards, plantings or cultivated fields |
| Duration | Temporary stay = typically 1–2 nights, where movement is allowed |
| Fire | Open fire **always** needs landowner permission; it is never part of everyman's rights. In national parks, fire only at maintained fire sites |
| Notes | Everyman's rights do not apply as-is in nature conservation areas |
| Sources | Ministry of the Environment FAQ https://valtioneuvosto.fi/-//1410903/saako-toisen-mailla-hiihtaa-enta-saako-jaalle-tehda-avannon-usein-kysyttya-ymparistosta-palveluun-koottu-yhteen-kysymyksia-ja-vastauksia-jokaisenoikeuksista ; Metsähallitus https://luontoon.fi |

#### Iceland — Tier A (Act no. 60/2013)

| Field | Value |
|---|---|
| Along public routes in inhabited areas | One night, traditional tent only, on uncultivated land, if there is no campsite in the immediate vicinity and no signs prohibit it |
| Uninhabited areas / away from public routes | Allowed unless special rules apply |
| Landowner permission needed | Near dwellings or farms; for more than one night; for more than three tents; or on cultivated land |
| Vehicles | Campervans, tent trailers, caravans: **never** outside campsites or urban areas without permission (§3.5 / designated only) |
| Protected areas | Decline unless the host has per-area rules. Many ban camping outright or restrict it to marked areas (e.g. Þingvellir, Hornstrandir, Mývatn, parts of Vatnajökull) |
| Sources | https://ust.is/english/visiting-iceland/travel-information/where-can-you-camp/ |

#### Estonia — Tier A

| Field | Value |
|---|---|
| Duration | Camping for one day (24 h) on unfenced, unsigned land. Longer stays need landowner permission |
| Distance | Pitch out of sight and hearing of dwellings. The ministry cited ≥150 m on open terrain; this comes via a Postimees report → **secondary; re-verify against the Code** |
| Sources | https://rmk.ee/en/exploring-nature/rules-of-conduct/freedom-to-roam/ ; General Part of the Environmental Code Act https://www.riigiteataja.ee/en/eli/523122024010/consolide |

#### Scotland — Tier A

| Field | Value |
|---|---|
| Style | Lightweight, small numbers, max 2–3 nights in one place |
| Filters | Not in enclosed fields of crops or animals. Keep well away from buildings, roads and historic structures. Ask permission to camp close to a house |
| Vehicles | Vehicle-based camping is not covered |
| Date-gated | Loch Lomond & Trossachs National Park Camping Management Zones need a permit or a campsite from **1 Mar to 30 Sep** |
| Sources | https://www.outdooraccess-scotland.scot/practical-guide-all/camping ; https://www.lochlomond-trossachs.org/things-to-do/camping/go-wild |

#### England / Wales — Tier C, except Dartmoor (Tier B)

| Area | Tier | Rules | Sources |
|---|---|---|---|
| England / Wales (general) | C | Designated / legal campsites from POI data only; do not run the wild-camp algorithm | not verified beyond Dartmoor exception |
| Dartmoor Commons | B | Backpack camping right under s.10(1) Dartmoor Commons Act 1985, upheld in *Darwall v Dartmoor NPA* [2025] UKSC 20 (21 May 2025). National park byelaws apply. Requires a host polygon for the Dartmoor commons; **without it → Tier C** | Judgment https://www.supremecourt.uk/cases/judgments/uksc-2023-0126 ; Dartmoor NPA camping https://www.dartmoor.gov.uk/enjoy-dartmoor/outdoor-activities/camping ; camping map https://www.dartmoor.gov.uk/about-us/about-us-maps/camping-map ; byelaws https://www.dartmoor.gov.uk/about-us/who-we-are/byelaws |

#### Denmark — Tier C, with one special case

Applies to Denmark proper (DK) only; Faroe Islands (FO) and Greenland (GL) →
see territories below.

Wild camping is not a general right. Naturstyrelsen **"fri teltning"** in ~275 designated state forests is the special case:

| Field | Value |
|---|---|
| 1-2-3 rule | 1 night in the same spot, max 2 tents, max 3 people per tent |
| Pitch | The tent must stand under trees and must not be visible from paths, roads or buildings. So **not** on beaches, dunes, meadows or clearings, even inside a fri-teltning forest |
| On Naturstyrelsen land | Sleeping bag / hammock / tarp directly on the forest floor is allowed |
| Host layer | Only run the special case if the host has the fri-teltning polygon layer. Otherwise suggest shelters and primitive sites only |
| Sources | https://naturstyrelsen.dk/aktiviteter-i-naturen/overnat-og-spis-i-naturen/fri-teltning ; https://naturstyrelsen.dk/aktiviteter-i-naturen/overnat-og-spis-i-naturen |

#### Germany — split by Land

**Federal baseline:** § 59 BNatSchG and § 14 BWaldG give a right of **access** for recreation only, not a right to camp. Hard filters: forest (camping needs owner/forestry consent in every Land) and protected areas.

Sources: https://www.gesetze-im-internet.de/bnatschg_2009/__59.html ;
https://www.gesetze-im-internet.de/bwaldg/__14.html

| Land | Tier | Rules | Sources |
|---|---|---|---|
| Brandenburg | B | § 22 (1) BbgNatSchAG: walkers, cyclists, riders and paddlers may pitch a tent for **one night** in open landscape (*freie Landschaft*). Not in gardens, farmyards or residential grounds. Show landowner-consent guidance: a county authority (Ostprignitz-Ruppin) states that owner consent is required | https://bravors.brandenburg.de/gesetze/bbgnatschag ; https://www.ostprignitz-ruppin.de (search "Genehmigung von Zelten") |
| Mecklenburg-Vorpommern | B | § 28 (2) NatSchAG M-V: non-motorised walkers, one night, in open landscape. Excluded: national parks, national natural monuments, NSG. Only if *privatrechtlich befugt* (no private-law objection) and no other rule forbids it. Forest excluded (§ 29 LWaldG M-V). No tents or fires on dunes, beach ridges or dikes (§ 27) | https://www.landesrecht-mv.de/bsmv/document/jlr-NatSchGMV2010pP28 |
| Schleswig-Holstein | B | § 37 (2) LNatSchG: walkers may camp one night away from campsites. Camping longer than one night is an offence (§ 57 (2) Nr. 21) | https://www.gesetze-im-internet.de/abweichendes_Landesrecht/natschg_sh__57.html |
| All other Länder | C | Designated Trekkingplätze / Biwakplätze only. Niedersachsen: a one-night rule appears in a 2012 draft (Landtag Drs. 16/4983) and is **not verified as enacted** → Tier C | https://www.landtag-niedersachsen.de/drucksachen/drucksachen_16_5000/4501-5000/16-4983.pdf |

**Conditions for every German Tier B pack:** `travel_mode` = non-motorised AND not forest AND not protected area AND not residential ground; `max_nights` = 1.

#### Austria

| Scope | Tier | Rules | Sources |
|---|---|---|---|
| Forest, nationwide | Hard filter | § 33 (3) Forstgesetz 1975: camping and lying up after dark in forest are prohibited without the owner's consent. Fine up to €150 | https://www.bmluk.gv.at/themen/wald/wald-freizeit/verhalten_wald/lagern_zelten_wohnen.html ; https://www.oesterreich.gv.at/de/themen/reisen_und_freizeit/freizeit-in-der-natur/freizeit_im_wald/Seite.3750020 |
| Kärnten, Niederösterreich, Tirol (above treeline) | C | Camping outside campsites not allowed | Alpenverein press release 22 Jun 2023 https://www.alpenverein.at/portal/service/presse/2023/2023_06_22-Wildcampen.php (Alpenverein is an NGO; the federal forest rule is from BMLUK above) |
| Oberösterreich, Salzburg, Steiermark, Vorarlberg (above treeline) | B | Above treeline only, not in protected areas, show "municipal rules may apply" | Same Alpenverein release (NGO) + BMLUK forest sources |
| Non-forest land below the treeline, all Länder | D | No rule researched | not verified |
| Wien, Burgenland | D | not verified | — |

#### Switzerland — Tier B above the treeline; Tier C below it

| Field | Value |
|---|---|
| Legal basis | Art. 699 ZGB, access to forest and pasture |
| Practice | A single night above the treeline is generally tolerated when done considerately |
| Always excluded | Swiss National Park, federal hunting reserves (*Jagdbanngebiete*), wildlife rest zones (*Wildruhezonen*) during their protection period, and many nature reserves. Cantons and communes may be stricter |
| Sources | ZGB Art. 699 https://www.fedlex.admin.ch/eli/cc/24/233_245_233/de#art_699 ; SAC https://www.sac-cas.ch/de/umwelt/bergsport-und-umwelt/campieren-und-biwakieren/ (**Swiss Alpine Club, not government**); protection zones on map.geo.admin.ch |

#### France — Tier C for tents; national-park cores have their own sub-packs

Applies to metropolitan France. Overseas departments and territories
(including French Guiana) are **not** covered → Tier D until researched.

| Field | Value |
|---|---|
| R111-32 Code de l'urbanisme | Camping outside campsites is allowed only **with** the consent of whoever has use of the land. The plugin cannot verify consent → no general wild-camp suggestions |
| R111-33 | Isolated camping is prohibited on the seashore, in listed/classified sites, near historic monuments, and within 200 m of drinking-water catchments |
| R111-34 | Local plans (PLU) or mayoral orders may add bans |
| National-park cores | Tier B sub-packs, one per park, only when the host knows the core boundary |
| Écrins | The new 2026 bivouac order applies. Bivouac only after 19:00, packed up before 09:00 (except bad weather endangering hikers). One night per site. Small tent only (no standing height). Anything left up in daytime counts as a *campement*, which is prohibited. Access-distance rule: at least one hour's walk from road access or the core boundary (2014 order). Re-check whether the 2026 order keeps it before implementing; until then treat distance as **"not verified"** and decline |
| Other parks (Vanoise, Cévennes, Mercantour, Pyrénées, Calanques, Port-Cros) | Tier D until each park's official order is read. Do not use secondary summaries |
| Sources | R111-32 https://www.legifrance.gouv.fr/codes/article_lc/LEGIARTI000031721244 ; R111-33 https://www.legifrance.gouv.fr/codes/article_lc/LEGIARTI000034355031 ; Section R111-32 to R111-35 https://www.legifrance.gouv.fr/codes/id/LEGISCTA000031721246 ; Écrins 2026 order https://www.ecrins-parcnational.fr/sites/ecrins-parcnational.com/files/article/27274/2606148arretebivouacvf1.pdf ; Écrins 2014 order https://aida.ineris.fr/reglementation/arrete-ndeg-1922013-040614-relatif-bivouac-coeur-parc-national-ecrins |

#### Netherlands — Tier C

| Field | Value |
|---|---|
| Wild camping | Not allowed |
| Staatsbosbeheer | Permanently closed its 17 *paalkampeerterreinen* (2020) |
| Remaining sites | About 30 paalkamp sites run by other owners remain. Suggest only when present in host POI data with an operator tag |
| Sources | Staatsbosbeheer closure notice https://www.staatsbosbeheer.nl/wat-we-doen/nieuws/2020/05/sluiten-paalkampeerterreinen ; FAQ (paalkamperen closed) https://www.logerenbijdeboswachter.nl/informatie/veelgestelde-vragen |

#### Belgium — Tier C

| Field | Value |
|---|---|
| Flanders | Wild camping is not allowed. Bivakzones only; Natuur en Bos zones need a (free) reservation |
| Wallonia | Bivouac zones exist; rules not separately verified → treat Wallonia as **Tier D** until its official source is read |
| Sources | https://natuurenbos.vlaanderen.be/faq/mag-ik-kamperen-een-bos-natuurgebied |

#### Poland — Tier C with a designated-area layer

| Field | Value |
|---|---|
| General | Ban outside designated places (Ustawa o lasach, art. 30) |
| "Zanocuj w lesie" | Areas in Lasy Państwowe districts: max 9 people, max 2 consecutive nights without notification (longer stays or bigger groups: email the district and get approval) |
| Fire / bans | Open fire only at designated places. No stays during forest-entry bans. Białowieża-area districts are excluded |
| Area polygons | Bank Danych o Lasach (BDL) |
| Sources | https://zanocujwlesie.lasy.gov.pl/ ; https://www.szczecin.lasy.gov.pl/program-zanocuj-w-lesie- ; statute via ISAP https://isap.sejm.gov.pl/isap.nsf/DocDetails.xsp?id=WDU20250000567 |

#### Czechia — Tier C

| Field | Value |
|---|---|
| Forest | Zákon 289/1995 § 20 (1)(k): no camping (*táboření*) in forests outside designated places. No open fire within 50 m of the forest edge |
| Parks | National parks, CHKO and NPR have their own bans; České Švýcarsko NP bans overnight stays outside designated sites |
| Without tent | Sleeping without a tent is tolerated by ministry interpretation, but that is guidance text only, not a filter |
| Sources | https://www.e-sbirka.cz/sb/1995/289 |

#### Latvia — Tier B (state forest only)

| Field | Value |
|---|---|
| Forest Law art. 5 | Right to stay in state and municipal forest; owners may restrict access to other forests |
| LVM | Tent or hammock anywhere in state forest outside protected areas. More than a week in one place or groups over 50 need written notice/approval. (**LVM tent statement was reported by public broadcaster LSM, not published by LVM → secondary**) |
| Vehicles | Only on roads |
| Host layer | Requires a host layer for LVM land; otherwise Tier C (LVM rest sites) |
| Sources | Forest Law https://www.vestnesis.lv/ta/id/2825 ; LVM rest sites https://atputa.lvm.lv |

#### Lithuania — Tier C

| Field | Value |
|---|---|
| Tents | Only in campsites and at sites marked with a tent sign |
| Fire | Only at marked fire pits. No overnight stays or camping on coastal dunes |
| Sources | https://aad.lrv.lt/en/memos-on-environmental-requirements/camping-responsibly/ ; https://lithuania.travel/en/what-to-do/active-recreation/camping |

#### Ireland — Tier C

| Field | Value |
|---|---|
| Access | No general right to roam |
| Coillte | Wild camping without a permit only at designated sites. Fires only at designated places |
| Sources | Coillte Recreation Policy https://www.coillte.ie/media/2019/07/Coillte-Recreation-Policy.pdf |

#### Territories that must not inherit a parent pack

These resolve to their own code (§3.1 step 2) and are **Tier D** until their
own pack is written. The rows record why inheritance would be wrong.

| Territory | ISO | Tier | Why the parent pack must not apply | Sources |
|---|---|---|---|---|
| Svalbard and Jan Mayen | SJ | D | Governed by the Svalbard Environmental Protection Act (*svalbardmiljøloven*), not friluftsloven. Polar bear rules: no one may travel or stay closer than **300 m** to a polar bear (**500 m** from 1 Mar to 30 Jun); the distance limit does not apply inside tents or huts. Anyone travelling outside settlements must carry means to scare off polar bears, and the Governor recommends a firearm. Notification duty (*meldeplikt*) applies for travel over large parts of Svalbard. **Card (even when declining):** polar bear distance rule, deterrent requirement, notification duty, "contact Sysselmesteren" | Sysselmesteren info-meeting slides on environmental-rule changes https://www.sysselmesteren.no/contentassets/9de976c28bcb4205a65bc6403ba8e2b2/informasjonsmote-i-longyearbyen-om-endringer-i-miljoregelverket.pdf ; Sikkerhet på Svalbard https://sysselmesteren.no/nb/publikasjoner/brosjyrer/sikkerhet-pa-svalbard ; lokalhistoriewiki summary https://lokalhistoriewiki.no/wiki/Sysselmannen_på_Svalbard (**secondary**) |
| Åland | AX | D | Autonomous; own legislation. Not verified whether Finnish everyman's-rights practice applies unchanged | not verified |
| Faroe Islands | FO | D | Outside Danish law on land access; Danish fri-teltning does not exist there | not verified |
| Greenland | GL | D | Own legislation; Danish rules do not apply | not verified |
| Northern Ireland | GB-NIR | D | Neither the Scottish Outdoor Access Code nor the England/Wales position applies | not verified |
| Isle of Man, Jersey, Guernsey | IM, JE, GG | D | Crown dependencies with own law | not verified |
| Gibraltar | GI | D | Own law | not verified |

---

## North America (land-manager-keyed, §3.2.1)

#### USA — Tier B per land manager (flag default OFF); everything else Tier D

There is no national right to roam. Keyed on PAD-US manager via
`land_tenure_query`. The host layer is the USGS **Protected Areas Database of
the United States (PAD-US)**, which records owner, manager, designation and
public-access fields per unit; its manager field distinguishes federal, tribal,
state, local and private managers and names agencies such as BLM.

| Manager | Tier | Rules | Sources |
|---|---|---|---|
| **BLM** (Bureau of Land Management) | B | Dispersed camping allowed on most BLM land unless posted "Closed to Camping" or restricted locally. **Stay:** generally **14 days within any 28-day period**; limits vary by state and field office. After the limit, move to a new location, often **25–30 miles** away (field-office guidance). **Agency guidance (not hard filters unless a local order says so):** camp within 150 ft of designated routes; ≥ 200 ft from lakes, rivers and streams; not within 1 mile of campgrounds, trailheads or picnic areas. Recreational use only, not long-term living (Long-Term Visitor Areas are a separate permitted exception). **Plugin state:** per-user stay counter with a **radius key (25 miles default)** and a 28-day window | BLM camping https://www.blm.gov/programs/recreation/camping ; BLM Idaho guidelines https://www.blm.gov/sites/blm.gov/files/documents/files/BLM%20ID_Camping_Guidelines.pdf ; regulation basis 43 CFR 8360 (via BLM page) |
| **USFS** (National Forests / Grasslands) | B | Dispersed camping allowed outside developed sites unless closed. **Stay limits are set per forest by order** under 36 CFR 261.58: common values are **14 days in 30** or **16 days in 30** with a move of several miles; some forests are far stricter (e.g. Angeles NF 2024–26 order: max 7 consecutive days outside developed campgrounds, max 3 days within 300 ft of a public road centreline, max 21 days per calendar year). **Default in plugin: 14 nights / 30 days, "check the forest order for {unit_name}".** Typical forest guidance: stay within 150 ft of a roadway; 100–200 ft from water (varies by forest); not near developed recreation sites. **The road∩track seed fits USFS well** — Forest Service roads are the expected access | Fishlake NF https://www.fs.usda.gov/r04/fishlake/recreation/dispersed-camping ; Apache-Sitgreaves NF https://www.fs.usda.gov/r03/apache-sitgreaves/recreation/dispersed-camping-guidelines ; San Juan NF https://www.fs.usda.gov/r02/sanjuan/recreation/dispersed-camping-guidelines ; Angeles NF order https://www.fs.usda.gov/r05/angeles/alerts/planning-camp-stay-limits-dispersed-camping-restrictions-until-dec-15-2026 |
| **NPS** (National Park Service), lower 48 + HI | C | 36 CFR 2.10: the superintendent may require permits and designate camping sites or areas; camping outside designated sites or areas is prohibited; camping within 25 ft of a hydrant or main road or within 100 ft of flowing water is prohibited except as designated. Backcountry requires the park's permit → designated sites / permit zones only | 36 CFR 2.10 https://www.ecfr.gov/current/title-36/chapter-I/part-2/section-2.10 |
| **NPS, Alaska park areas** | B | 36 CFR 13.25: camping authorised in Alaska park areas unless the superintendent restricts it; 14 consecutive days in one location, then move at least 2 miles. Separate sub-pack; needs host to flag Alaska NPS units | 36 CFR 13.25 (text via federal-regs.com https://federal-regs.com/title/36/part-13/13.25/ — **re-check on eCFR**) |
| US Fish & Wildlife Service, DoD, other federal | D | not researched | not verified |
| State land (all states), local, private, tribal, unknown | D | Varies by state / owner; not researched | not verified |

**Fire (all US packs):** guidance only — "check current fire restrictions for
{unit_name}"; restrictions change daily in fire season. Never a hard pass.

**Card for every US suggestion:** manager and unit name, stay limit used and
whether it is a default or a verified local order, "posted closures and fire
restrictions override this suggestion".

#### Canada — Tier B per province / manager (flag default OFF); everything else Tier D

Keyed on province (`admin_region_read`) plus tenure (`land_tenure_query`).
Requires `traveller_profile_read.residency_country` for Ontario.

| Province / manager | Tier | Rules | Sources |
|---|---|---|---|
| **Ontario — Crown land** | B | Anyone camping for private, non-commercial purposes may stay up to **21 days on any one site per calendar year**, then must move **≥ 100 m**. Posted signs may restrict camping or shorten the stay. **Non-residents of Canada:** some need a permit to camp north of the French and Mattawa Rivers; they may not camp in designated **green zones**; check the Crown Land Use Policy Atlas. Hard filters: non-resident AND north of French/Mattawa → require permit (card: "permit required"; do not suggest unless the user marks a permit as held — TODO field); non-resident AND green zone → exclude | https://www.ontario.ca/page/recreational-activities-on-crown-land ; https://www.ontario.ca/page/non-resident-crown-land-camping-and-green-zones |
| **British Columbia — Crown land** | B | Anyone may camp on Crown land for up to **14 consecutive days**; the count only resets if the person, vehicle and equipment are absent from the site for **≥ 72 consecutive hours**. Recreation sites have their own 14-day rule (Forest Recreation Regulation s.13) | Land Use Policy — Permission §8.2 https://www2.gov.bc.ca/assets/gov/farming-natural-resources-and-industry/natural-resource-use/land-water-use/crown-land/permissions.pdf ; Forest Recreation Regulation https://free.bcpublications.ca/civix/document/id/loo65/loo65/16_2004 |
| **Alberta — public land / PLUZ** | B | **Public Lands Camping Pass** required to random camp on public land along the Eastern Slopes (Grande Prairie to Waterton), in Porcupine Hills PLUZ and Willmore Wilderness Park; per person; same price for residents and non-residents. Random camping allowed unless signs/notices say otherwise; **not within 1 km of a Public Land Recreation Area, Provincial Park or Provincial Recreation Area**; **not within 1 km of a road** in Kananaskis PLUZ, McLean Creek OHV PLUZ, Sibbald and Cataract Creek Snow Vehicle PLUZs (§1.3 override). Max **14 days**; after that move **1 km for 72 h** (government checklist). In provincial parks / recreation areas: designated campgrounds only (→ Tier C there). Needs host polygon for the pass area | https://www.alberta.ca/camping-on-public-land.aspx ; https://www.alberta.ca/public-lands-camping-pass.aspx ; checklist PDF https://open.alberta.ca/dataset/43f6b769-44f3-4871-ad29-6d84195bbdc1/resource/149d6aea-55b7-4e5d-96ae-68b4ea442474/download/aep-outdoor-recreation-checklist-for-provincial-crown-land-2022.pdf |
| **Québec — open public land** | B | Rough camping permitted in many areas of public land; stay must be temporary and equipment mobile, not fixed to the ground. Official guidance: free public land may be occupied temporarily for **no more than seven months in a year**. **Structured territories** (ZECs, wildlife reserves, outfitters with exclusive rights, national parks) are governed by their own organisations → **exclude** (Tier C/D for those). RCMs may set different rules → card text | https://www.quebec.ca/en/tourism-and-recreation/sporting-and-outdoor-activities/activities-permitted-public-land ; https://www.quebec.ca/nouvelles/actualites/details/terres-du-domaine-de-letat-partager-le-territoire-public-tout-en-respectant-les-lois-42054 |
| **Parks Canada** (national parks) | C | Backcountry overnight stays need a backcountry permit; random camping, where it exists at all, is limited to designated zones with park-specific distance rules (e.g. Banff: ≥ 5 km from trailhead or designated campground; Glacier: ≥ 5 km from the Trans-Canada Highway). Designated / permit sites only | Banff https://parkscanada.gc.ca/banff-backcountry ; Glacier https://parks.canada.ca/pn-np/bc/glacier/activ/passez-stay/arrierepays-backcountry ; Waterton https://parks.canada.ca/pn-np/ab/waterton/activ/camping/arrierepays-wilderness-camping |
| Provincial parks (all provinces) | C | Designated campgrounds only (confirmed for Alberta; assumed for others until read → Tier D for non-Alberta provincial parks) | Alberta source above |
| Other provinces and territories (SK, MB, NB, NS, PE, NL, YT, NT, NU) | D | not researched | not verified |
| Private land, First Nations reserve land, leased/licensed Crown land, unknown | D | — | — |

**Fire (all CA packs):** guidance only — Ontario Restricted Fire Zones,
Alberta fire bans, BC bans change with conditions; card: "check current fire
bans for {province}".

---

## Russia

#### Russia — Tier B (flag default OFF), forest-fund land only

Russia has a **statutory right to be in forests**, not a Nordic-style camping
right, so it is land-category-keyed (§3.2.1).

| Field | Value |
|---|---|
| Legal basis | Forest Code art. 11(1): citizens have the right to be in forests freely and without charge, and to gather wild berries, mushrooms, nuts and similar for personal needs |
| Restrictions in law | Forest Code art. 11(4): presence may be prohibited or restricted in forests on defence and security lands, on specially protected natural territories (ООПТ), and on other lands where federal law restricts access. Art. 11(5): may be restricted for fire safety and sanitary reasons. Citizens must follow forest fire-safety rules |
| Shore strip | Water Code art. 6: the 20 m shore strip (5 m for canals and for rivers/streams ≤ 10 km long) of public water bodies is for public use; everyone may use it **without mechanical vehicles** for movement and staying near the water. Guidance for access, not a filter |
| Hard filters | Land category ≠ forest fund → decline. Inside ООПТ → decline. Defence/security land → decline. **Border zone (пограничная зона)** → decline: entry requires a pass issued by FSB border bodies (FSB order 102 of 28 Feb 2023). This affects the whole Norway–Russia border area |
| Duration | **not verified** — Forest Code sets none; do not invent one. Card: "no statutory stay limit found" |
| Distance | **not verified** |
| Fire | Guidance only: regional **особый противопожарный режим** (special fire regime) may ban fire or forest entry; cannot be checked from map |
| Foreign travellers | Migration-registration and entry rules exist but are **not researched here** → card: "entry and registration rules for foreign nationals are not covered by this plugin" |
| Host layer | Needs Russian land categories + ООПТ + border zones via `land_tenure_query`; without it → Tier D |
| Vehicles | not researched (§3.5 TODO) |
| Sources | Forest Code art. 11 https://www.consultant.ru/document/cons_doc_LAW_64299/fdc3eb1198e1ac4458b4fc50c923d51cb84abab6/ ; Water Code art. 6 as quoted by Ministry of Transport https://mintrans.gov.ru/file/400451 and Prosecutor General's Office https://epp.genproc.gov.ru/upload/iblock/6af/72zvfenwy5s8ce5nejs3o5k0skgrkthv.doc ; border-zone pass rules (FSB order 102/2023) as summarised by ppt.ru https://ppt.ru/amp/art/bezopasnost/kak-poluchit-propusk-v-pogranichnuyu-zonu (**secondary — re-check against publication.pravo.gov.ru**) |

---

## Latin America

#### Mexico — Tier D

| Field | Value |
|---|---|
| Beaches | The Constitution and the General Law of National Assets make beaches and the federal maritime-terrestrial zone (ZOFEMAT) federal property. A 2025 reform approved by the Chamber of Deputies guarantees free access to beaches and ZOFEMAT except for environmental, security or national-interest reasons. **Access is not a right to camp overnight** → do not derive a camping pack from it. Senate/enactment status **not verified** |
| Protected areas (CONANP) | not researched |
| Everything else | not verified |
| Sources | ZOFEMAT regulation https://sidof.segob.gob.mx/notas/docFuente/4739967 ; Senate dictamen https://infosen.senado.gob.mx/sgsp/gaceta/64/3/2020-09-29-1/assets/documentos/Dict_Gobernacion_Minuta_Transito_Playas.pdf ; 2025 Chamber vote (press) https://politica.expansion.mx/congreso/2025/10/02/diputados-aprueban-libre-acceso-a-playas (**secondary**) |

#### Chile — Tier C inside CONAF protected areas; Tier D elsewhere

| Field | Value |
|---|---|
| Scope | State protected areas (SNASPE: national parks, national reserves, natural monuments) administered by CONAF |
| Camping | Designated sites only (host POI with CONAF operator) |
| Fire | Fire is prohibited in most parks except in enabled zones. CONAF states that using fire in unauthorised places in its areas carries **61 days to 3 years' imprisonment** and a fine; causing a serious forest fire in a protected area carries far heavier penalties and **expulsion for foreigners** (Law 20.653). **Card must say this** |
| Outside protected areas | Tier D |
| Sources | Sernatur (official tourism board) https://chile.travel/blog/explora-los-parques-y-reservas-naturales-de-chile/ ; https://chile.travel/?p=45122 ; CONAF director statement reported by https://www.semanariolocal.cl/?p=33017 (**secondary — re-check on conaf.cl / leychile.cl, Ley 20.653**) |

#### Argentina — Tier C inside national parks (APN); Tier D elsewhere

| Field | Value |
|---|---|
| Scope | Areas under the Administración de Parques Nacionales (Law 22.351) |
| Camping | Per-park rules. APN's camping regulation defines camping areas as land **enabled for that purpose**; individual parks set their own sites, registration and fire bans (e.g. Quebrada del Condorito: registration, fire prohibited, motorhomes only in one car park). Designated/registered sites only |
| Outside national parks | Tier D (provincial parks and private land not researched) |
| Sources | APN camping regulation (Boletín Oficial 06-11-2019) https://www.boletinoficial.gob.ar/detalleAviso/primera/220793/20191106 ; example park page https://www.argentina.gob.ar/parque-nacional-quebrada-del-condorito/alojamiento |

#### Rest of Latin America and the Caribbean — Tier D

Costa Rica: each protected area has its own *Reglamento de Uso Público*; some
(e.g. Manuel Antonio, Volcán Irazú) ban camping outright — reported by press
(https://www.elfinancierocr.com/lab-de-ideas/acampar-en-costa-rica-esta-guia-le-detalla-las/I7SKT3TKQRG6FO7AFBIHVNQDQU/story/,
**secondary**) → Tier D until the reglamentos on SCIJ are read. All other
countries: see world Tier D table.

---

## Asia

#### South Korea — Tier C inside natural parks; Tier D elsewhere

| Field | Value |
|---|---|
| Natural Parks Act art. 27(1)6 | Camping outside designated places in natural parks is prohibited; art. 86(2) sets a fine of up to ₩500,000 |
| Behaviour | Natural parks (national, provincial, county): designated campgrounds/shelters only |
| Sources | Art. 86 text as quoted at https://www.nepla.ai/wiki/국토와-건설-부동산/건설-건축-주거환경/-유권해석-자연공원법-제86조-과태료-zxk9e6pl3n7m ; Incheon city park rules https://www.incheon.go.kr/park/park030101/1291099 (**re-check on law.go.kr, 자연공원법**) |

#### Japan — Tier D

| Field | Value |
|---|---|
| Natural Parks Act art. 21(3) | In special protection zones of national/quasi-national parks, lighting fires (火入れ / たき火) needs a permit from the Minister of the Environment (national) or prefectural governor (quasi-national). Guidance text only |
| Camping | No official source read on where camping is allowed → Tier D |
| Sources | Aomori Prefecture reproduction of Natural Parks Act art. 21 https://www.pref.aomori.lg.jp/kensei/jyourei/shinsakijyun/gyote_shinsakijyun_1667.html |

#### Rest of Asia — Tier D

See world Tier D table.

---

## Africa — Tier D

No official source was verified for any African country in this pass. Every
African country is Tier D (campsites only). Priority TODOs: South Africa
(SANParks), Namibia, Botswana, Morocco — national-park rules there are expected
to be designated-site regimes, but **do not write rules until each official
source is read**.

---

## Oceania — Tier D

Not researched in this pass. Priority TODO: **New Zealand** (Freedom Camping
Act and Department of Conservation rules; vehicle "self-contained" rules make
it a §3.5 case as well) and **Australia** (state-by-state Crown land and park
rules — land-manager-keyed like Canada).

---

#### World Tier D table (no official source verified yet)

TODO rows — do not write rules for these countries:

| Region | Countries / territories | Tier | Sources |
|---|---|---|---|
| Europe | Croatia, Slovenia, Spain, Italy, Portugal, Greece, Hungary, Slovakia, Luxembourg, Liechtenstein, Romania, Bulgaria, Serbia, Bosnia and Herzegovina, Montenegro, North Macedonia, Albania, Kosovo, Ukraine, Moldova, Belarus, Malta, Cyprus, Andorra, Monaco, San Marino, Vatican City | D | not verified |
| Europe — sub-regions | Belgium (Wallonia); Austria (Wien, Burgenland, non-forest below treeline); French national parks other than Écrins; France overseas | D | not verified |
| Territories | SJ, AX, FO, GL, GB-NIR, IM, JE, GG, GI (see territories table) | D | see table |
| North America | USA: all non-BLM/USFS/NPS land; Canada: SK, MB, NB, NS, PE, NL, YT, NT, NU; Greenland (GL); Saint Pierre and Miquelon; Bermuda | D | not verified |
| Mexico, Central America, Caribbean | Mexico, Guatemala, Belize, Honduras, El Salvador, Nicaragua, Costa Rica, Panama, all Caribbean states and territories | D | not verified |
| South America | Brazil, Peru, Bolivia, Ecuador, Colombia, Venezuela, Uruguay, Paraguay, Guyana, Suriname, French Guiana; Chile and Argentina outside their national protected areas | D | not verified |
| Asia | Japan; South Korea outside natural parks; China, Mongolia, India, Nepal, Bhutan, Sri Lanka, Pakistan, Bangladesh, all of Southeast Asia, Central Asia, Caucasus (Georgia, Armenia, Azerbaijan), Turkey, Middle East | D | not verified |
| Africa | All countries | D | not verified |
| Oceania | Australia, New Zealand, all Pacific states | D | not verified |
| Every country not listed above | — | D | not verified |

Last verified: 2026-09

### 3.3 Where wild-camp suggestions are allowed

Apply the tier table in §3.2:

- **Tier A:** may run the road∩track algorithm with that pack’s filters and guidance.
- **Tier B:** may run only when the maintainer flag is ON and every listed
  condition is host-checkable and passes; otherwise treat as Tier C/D as stated
  for that pack. Land-manager-keyed packs additionally need an authoritative
  tenure answer at the probe point (§3.2.1).
- **Tier C:** never run the wild-camp algorithm; designated sites from host
  POI/area data only.
- **Tier D / unknown:** decline wild camp; suggest campsites only. Never fall
  back to Norway, and never fall back to a parent state's pack for a territory.

### 3.4 Design note (mandatory)

Right-to-roam is **not uniform**. Silent reuse of allemannsretten geometry +
150 m / 2-night / fire window elsewhere is a **spec violation**.

**No rule pack may borrow another country's distance, duration or fire rule;
vehicle overnight rules are never derived from right-to-roam.** The same
applies between land managers: a BLM stay limit is never applied on USFS land,
and an Ontario Crown land rule is never applied in another province.

---

## 3.5 Vehicle overnight mode

Right-to-roam packs **never** authorise sleeping in a vehicle. Vehicle
suggestions use `vehicle_profile_read` and designated/road-side data only.
Land-manager packs that explicitly include vehicles (USA, Canada below) are a
separate, explicitly sourced vehicle rule — not a right-to-roam derivation.

### Norway

| Site type | Rules |
|---|---|
| **Døgnhvileplass** (NVDB object type 809, sign 638) | Reserved for heavy-transport drivers subject to the driving and rest time rules. Uses: break (45 min), daily rest (11 h), and at most sites reduced weekly rest (≥24 h, <45 h). Not for longer stays, reloading, parking vehicles/trailers, or drivers of other vehicle types. Suggest **only** when `class = hgv` AND `is_professional_driver_under_rest_rules`. For every other profile: **hard exclude**. Card text: which rest type the stop fits, and "follow the parking terms on the site's sign". |
| **Rasteplass** (NVDB object type 39, sign 613) | For breaks; "not made for camping over time". Overnight rules for motorhomes depend on the sign at each site. Campervan/car: offer as "short rest; check the sign for overnight rules". Never claim overnight is permitted unless a site attribute says so. HGV: fallback break stop, labelled "no daily-rest facilities". |
| Motorhome parking (`tourism=caravan_site`) | Preferred for campervans |

Sources:

- https://www.vegvesen.no/kjoretoy/yrkestransport/kjore-og-hviletid/hvileplasser/
- https://www.vegvesen.no/trafikkinformasjon/vei-og-skilt/drift-og-vedlikehold-av-vei/rasteplasser/
- Håndbok V273: https://www.vegvesen.no/globalassets/fag/handboker/v273-2022.pdf
- NVDB spec for Rasteplass: https://www.vegvesen.no/nvdb/datakatalog/eksport/produktspesifikasjon/39.pdf

Last verified: 2026-09

### Germany

| Topic | Rules |
|---|---|
| Wohnmobilstellplätze | `tourism=caravan_site`, or parking with a motorhome-only supplementary sign: preferred for campervan/caravan profiles. Show fee and duration tags if present |
| Public parking / roadside | Sleeping in the vehicle is allowed only to restore fitness to drive during an interrupted journey. Once the destination is reached, it becomes unauthorised special use (OLG Schleswig, 1 Ss-OWi 183/19, 15.06.2020; in SH also an offence under § 37 (1) LNatSchG). **Hard exclude** near `route_destination_read`. **Hard exclude** a 2nd night at the same spot (plugin night store). Card guidance: no camping furniture, no awning. "About 10 hours" is a convention from secondary sources — label it as such. Signs forbidding parking or overnight stays override everything. Truck-only and car-only bays are never offered to motorhomes. Check the § 12 StVO restrictions for >7.5 t vehicles in residential areas against the official text |
| HGV professional drivers | Autobahn rest areas / Autohöfe truck bays (`hgv=designated`) → suggest for daily rest. Never offered to car/campervan |

Sources:

- https://www.schleswig-holstein.de/DE/justiz/gerichte-und-justizbehoerden/OLG/Presse/PI/202007Wohnmobil.html
- § 12 StVO: https://www.gesetze-im-internet.de/stvo_2013/__12.html

Last verified: 2026-09

### France

| Topic | Rules |
|---|---|
| Parking overnight | A camping-car may park like a passenger car. A night-only ban is unlawful (2004 interministerial circular). A mayor may restrict parking only by a reasoned local order; a blanket commune-wide ban is not lawful. → Allowed to suggest ordinary public parking for an overnight rest, with the card line "local parking orders and signs apply" |
| Parking ≠ camping | On the public road: no levelling blocks, no awning, no table or chairs. The card must say so |
| Hard exclude (camping-car parking banned) | Seashore (*rivages de la mer*), near classified/listed sites, within 200 m of drinking-water catchments, woodland protected as *espaces boisés classés*. The AN answer cites old article numbers — map them to the current Code de l'urbanisme numbering before implementing |
| Preferred | *Aires de camping-car* / *aires de services* (`tourism=caravan_site`, `amenity=sanitary_dump_station`), and private land with the owner's permission (show a permission reminder) |
| National park cores | Vehicles are never part of bivouac rules |

Sources:

- Sénat written answer (2014): https://www.senat.fr/questions/base/2014/qSEQ141013206.html
- Sénat written answer (2024): https://www.senat.fr/questions/base/2024/qSEQ241001232.html
- Assemblée nationale answer: https://www.assemblee-nationale.fr/dyn/13/questions/QANR5L13QE4957.pdf
- CGCT L2213-2: https://www.legifrance.gouv.fr/codes/article_lc/LEGIARTI000043976727

Last verified: 2026-09

### USA (land-manager-keyed, Tier B, flag default OFF)

| Manager | Vehicle rule |
|---|---|
| BLM | Dispersed camping rules above apply to vehicle campers too (same 14-in-28 limit, same radius counter). Stay on existing routes; agency guidance is to camp within 150 ft of designated routes and not create new tracks. Offer to car/campervan/caravan; **never to HGV** (not researched) |
| USFS | Motor-vehicle access for camping is governed by each forest's Motor Vehicle Use Map (MVUM) and orders; distances off the road vary by forest (e.g. Medicine Bow: some roads allow parking up to 300 ft off, per MVUM). **The host must hold MVUM data** to offer vehicle camping; without it → designated campgrounds only |
| NPS, all other managers | Designated campgrounds only / Tier D |

Sources: BLM and USFS sources in §3.2 USA; Medicine Bow-Routt FAQ https://fs.usda.gov/media/243280

### Canada (land-manager-keyed, Tier B, flag default OFF)

| Province | Vehicle rule |
|---|---|
| Ontario Crown land | Ontario's definition of a camping unit includes trailers, tent-trailers, recreational vehicles and camper-backs, so the same 21-day / 100 m rule and non-resident permit rule apply to vehicle campers |
| BC Crown land | The 14-day count covers the person **and their vehicle and equipment** |
| Alberta public land | Same random-camping rules and pass requirement; pass is per person, not per vehicle |
| Québec, others | not researched → designated sites only |

Sources: Ontario non-resident policy (camping-unit definition) https://www.ontario.ca/page/non-resident-crown-land-camping-and-green-zones ; BC and Alberta sources in §3.2 Canada.

Last verified: 2026-09

### All other countries

Vehicle rules are not researched. Suggest designated motorhome sites
(`tourism=caravan_site`) for campervans and signed truck parking for HGV
professional drivers only. Add a TODO per country.

| Country / region | Vehicle overnight | Sources |
|---|---|---|
| Russia | TODO — note Water Code art. 6 bars mechanical vehicles from the shore strip | see Russia pack |
| New Zealand | TODO — priority (Freedom Camping Act, self-contained vehicle rules) | not verified |
| Every country not covered above | TODO — designated `tourism=caravan_site` / signed HGV truck parking only | not verified |

Last verified: 2026-09

---

## 4. Presentation requirements

For every accepted suggestion, show **in one place**:

1. Location (map pin / lat–lon / distance along route).
2. Access note (e.g. “via track from tertiary/unclassified/service”), or vehicle
   site type for §3.5.
3. **Tier**, **legal basis**, and **source link(s)** for the pack that applied.
4. **All applicable rules** for that country/region — hard-filter outcomes
   already applied, **plus** fire, foraging, leave-no-trace guidance even when
   those were not used to reject the candidate.
5. **Tier B cards** list the conditions that were checked, e.g.
   "non-motorised ✓, not forest ✓, not NSG ✓", plus
   "landowner / local rules may still apply".
6. **Land-manager cards** (USA, Canada, Russia) show the manager and unit name,
   the stay limit used and whether it is a **default** or a **verified local
   order**, and "posted closures, fire restrictions and local orders override
   this suggestion".
7. **Vehicle cards** show the vehicle-class condition that was checked.
8. **Fire guidance** computed from the **current date** at display time (where
   the pack has a date-gated fire rule).
9. **Cloudberry** note only for Northern Norway candidates.
10. **Svalbard** decline card carries the polar bear, deterrent and
    notification-duty text (§3.2 territories).
11. The **disclaimer** (§ top).

Ranking UI may show multiple candidates; each card must carry the full guidance
set for its location.

---

## 5. Filter vs guidance summary

| Item | Hard filter | Always / conditional guidance |
|---|---|---|
| Min distance from buildings (shared SafetyConfig; NO pack; SE labelled as Navi safety default) | Yes where pack requires | Show value used and whether it is law or safety default |
| max_nights per pack | Yes where pack hard-limits | Explain limit. **NO 2**; **DE / DK / IS / EE / Écrins 1**; **PL 2** (designated); **Scotland 2–3**; **SE / FI soft guidance**; **US BLM 14 in 28 (+25-mile radius)**; **US USFS 14 in 30 default, per-forest order**; **US NPS Alaska 14 then move 2 mi**; **ON 21 per site per calendar year, move 100 m**; **BC 14 consecutive (72 h absence resets)**; **AB 14, then move 1 km for 72 h**; **QC ≤ 7 months per year**; **RU not verified** |
| Territory resolves to own ISO code (SJ, AX, FO, GL, GB-NIR, IM, JE, GG, GI) | Yes — never inherit parent pack | Territory-specific decline card (Svalbard) |
| Land tenure unknown / private / tribal (land-manager packs) | Yes — Tier D | — |
| Non-resident in Ontario green zone | Yes — exclude | — |
| Non-resident in Ontario north of French/Mattawa without permit | Yes — exclude | "Permit required" card |
| Alberta 1 km from parks/PRAs; 1 km from road in named PLUZs | Yes | Explain distance |
| Alberta camping-pass area | Pass required | "Public Lands Camping Pass required" |
| Québec structured territory (ZEC, wildlife reserve, park) | Yes — exclude | Explain |
| Russia: not forest fund / ООПТ / defence land / border zone | Yes — decline | Explain; border-zone pass text |
| Service-road seed quality | Soft rank / caution | Optional access note |
| Forest (DE, AT, CZ) | Yes | Explain forest ban / consent |
| Protected area (all packs, unless a park sub-pack applies) | Yes | Explain decline or switch to park sub-pack |
| Travel mode (DE Tier B) | Yes — non-motorised required | Show checked condition |
| Døgnhvileplass for non-professional profiles | Yes — hard exclude | — |
| Truck bays for car/campervan | Yes — hard exclude | — |
| DE roadside at/near destination or 2nd night | Yes — hard exclude | Card: interrupted journey only; no furniture/awning; "~10 hours" secondary |
| FR seashore / catchment / listed-site vehicle bans | Yes — hard exclude | — |
| US USFS vehicle camping without MVUM data | Yes — designated only | — |
| Fire ban window NO 15 Apr–15 Sep | No (map can’t enforce permission) | Yes — date-gated text |
| Fire restrictions US / CA / RU | No | Yes — "check current restrictions for {unit}" |
| Chile CONAF fire penalties | No | Yes — penalty text on every Chile card |
| Loch Lomond CMZ 1 Mar–30 Sep | Yes where in zone without permit/campsite | Permit / campsite guidance |
| Écrins bivouac hours 19:00–09:00 | Yes for Écrins pack | Show hours on card |
| Fire “cannot spread” exception (NO) | No | Yes — informational |
| No fire on bare rock (NO) | No | Yes — year-round with fire text |
| Protected species foraging | No | Yes |
| Cloudberry (N. Norway) | No | Yes — region-gated |
| Leave no trace | No | Yes — every suggestion |
| Unknown country / Tier D | Decline wild camp | Suggest campsites only |
| Tier C packs | Decline wild-camp algorithm | Designated sites only |

Last verified: 2026-09

---

## 6. State ownership summary

| Concern | Where it lives |
|---|---|
| Default / configured building distance (150 m in Norway default; SE safety default) | Core `SafetyConfig.min_building_distance_m` |
| Glacier / other overnight safety already in core | Core (plugin may query related POIs; does not fork constants) |
| max_nights per pack (plugin night store) | Plugin-local KV/storage |
| Per-area stay counters with radius or calendar window (US BLM 25-mile / 28-day; Ontario calendar year; BC 72 h reset; AB 1 km / 72 h) | Plugin-local KV/storage |
| Per-vehicle "same spot" key (German roadside 2nd-night rule) | Plugin-local KV/storage |
| Fire ban / date-gated calendar logic | Plugin (uses host clock) |
| Country / ISO 3166-2 / territory codes / Nordland–Troms–Finnmark / park cores / US states / CA provinces / RU subjects | Host `admin_region_read` |
| Land manager, unit, sub-unit, RU land category, border zone | Host `land_tenure_query` |
| Vehicle class and professional-driver flag | Host `vehicle_profile_read` (user-set; never inferred from size) |
| Traveller residency | Host `traveller_profile_read` (user-set; never inferred) |
| Designated-site polygons / NVDB rest areas / Alberta pass area / MVUM | Host POI/area data (never fetched from WASM) |

---

## 7. Implementation sketch (future; out of scope now)

1. Host exposes route corridor samples + optional junction list or edge pairs.
2. Guest ranks seeds, walks tracks, queries POIs for buildings, reads safety
   distance and admin region.
3. Where the pack is land-manager-keyed, query `land_tenure_query` per probe.
4. Apply country / land-manager pack by tier; persist max_nights, per-area
   stay counters and DE vehicle same-spot keys; format suggestion payload for UI.
5. Android (or other) host renders pins + rule text; no WASM UI.
6. Vehicle overnight mode (§3.5) is a separate suggestion path using
   `vehicle_profile_read` and designated/road-side layers only.
7. Suggested order of new host layers by value: PAD-US (USA) → Ontario and BC
   Crown land → Alberta pass area → Russian land categories/border zones.

---

## 8. Related documents

- [`plugins.md`](../plugins.md) — sandbox, capabilities, design rules  
- [`../jurisdiction-rules.md`](../jurisdiction-rules.md) — reusable country/region rule-pack pattern (this camping table is a grounding example)  
- [`../jurisdiction-sources.md`](../jurisdiction-sources.md) — every official URL cited in this spec, with a "verified on" date per link  
- [`../poi.md`](../poi.md) — POI categories / spatial index  
- Core: `SafetyConfig`, `SAFETY_MIN_BUILDING_DISTANCE_M`  
- [`../README.md`](../README.md) — Rest and overnight (*allemannsretten* default note)

---

## 9. Revision notes (2026-09)

- Added coverage statement; all unlisted jurisdictions are Tier D.
- Added land-manager-keyed pack model (§3.2.1) and host capabilities
  `land_tenure_query` and `traveller_profile_read`; extended
  `admin_region_read` for territory ISO codes, US states, CA provinces and RU
  subjects.
- New packs: USA (BLM, USFS, NPS, NPS Alaska), Canada (Ontario, BC, Alberta,
  Québec, Parks Canada), Russia (forest fund), Chile (CONAF areas), Argentina
  (APN), South Korea (natural parks).
- New Tier D entries with notes: Mexico, Japan, Costa Rica, all Africa, all
  Oceania, remaining Europe, Latin America and Asia.
- Territories table: SJ, AX, FO, GL, GB-NIR, IM, JE, GG, GI must never
  inherit a parent pack; Svalbard decline card with polar bear rules.
- §1.3: pack-specific minimum road distances override the track-walk tunable.
- §3.5: vehicle rows for USA and Canada; Russia and New Zealand TODOs.
- **Action:** copy every new URL into `docs/jurisdiction-sources.md` with its
  verified-on date; items marked **secondary** or **re-check** must be verified
  against the official text before their pack's flag is turned ON.
