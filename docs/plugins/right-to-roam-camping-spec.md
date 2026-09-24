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
- Suggesting wild camps under Nordic-style rules where the country’s pack is
  Tier C or Tier D (see §3); designated sites or decline only in those cases.
- Deriving vehicle overnight suggestions from right-to-roam packs (see §3.5).

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
| `admin_region_read` (new) | Must return ISO 3166-2 subdivisions: German Länder, Austrian Bundesländer, Swiss cantons, Norwegian counties, plus the national park core for France |
| `clock_read` (new) | Current local or UTC date for fire-ban window and date-gated packs |
| `plugin_kv` / `storage` (new) | Plugin-local persist for max_nights per pack and DE vehicle same-spot key |
| `protected_area_query` (new) | Is the point inside a national park, nature reserve or other protected area (OSM `boundary=protected_area`, `boundary=national_park`, `leisure=nature_reserve`)? Where needed, also return the park's core/zone |
| `landcover_query` (new) | Is the point in forest (`landuse=forest`, `natural=wood`), on farmland or pasture, on beach or dune, or on open alpine land above the treeline? The treeline is **not** a fixed altitude — do not hard-code one |
| `travel_mode_read` (new) | Non-motorised (foot/bicycle/horse/canoe) vs motorised |
| `vehicle_profile_read` (new) | `{ class: car \| campervan_motorhome \| caravan_combo \| hgv, gross_weight_kg, is_professional_driver_under_rest_rules }`. The last field is set by the user in the vehicle profile; never infer it from size. Reuse Navi's existing vehicle profile if one exists |
| `route_destination_read` (new) | The final destination of the active route (needed for the German vehicle rule) |
| `log` | Diagnostics |

**Designated-site layers** (paalkamp poles, bivakzones, Trekkingplätze, Danish
fri-teltning forests, Polish "Zanocuj w lesie" areas, Norwegian NVDB rest
areas) are **host-side POI/area data**. Never fetch them from inside WASM.

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
   `admin_region_read` (ISO 3166-2 where required).
2. Select the rule pack below by **tier**.
3. If country or region **cannot be determined confidently**, or the pack is
   **Tier D** → **decline** wild-camp suggestions and suggest campsites only.
4. **Unknown country or region → Tier D. Never fall back to Norway.**
5. The Norwegian 150 m / 2-night / 15 Apr–15 Sep logic is **never** reused for
   another country. Each pack declares its own distance ("none in law" is a
   valid value), duration, fire rule and sources.

### 3.2 Tent / on-foot rule packs

Legal frameworks change. Treat the following as the implementer reference;
re-check each country’s official source before production use. Every row has
a **Sources** column. Full URL list with verification dates:
[`docs/jurisdiction-sources.md`](../jurisdiction-sources.md).

#### Rule-pack tiers

| Tier | Meaning |
|---|---|
| **A – general right** | The road∩track wild-camp algorithm may run, with the country's own filters. |
| **B – conditional** | The algorithm runs only when every listed condition is host-checkable and passes. Each Tier B pack sits behind a **maintainer flag that defaults to OFF** until its conditions are implemented. |
| **C – designated sites only** | Never run the wild-camp algorithm. Suggest only designated sites from POI/area data. |
| **D – not verified** | Decline and suggest campsites only. |

#### Norway — Tier A

Clarification: *allemannsretten* covers people on foot, not vehicles. Motorised
travel in the outfield is regulated separately → vehicles use §3.5 only.
Detailed hard filters and guidance remain in §2.

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
| Wien, Burgenland | D | not verified | — |

#### Switzerland — Tier B above the treeline; Tier C below it

| Field | Value |
|---|---|
| Legal basis | Art. 699 ZGB, access to forest and pasture |
| Practice | A single night above the treeline is generally tolerated when done considerately |
| Always excluded | Swiss National Park, federal hunting reserves (*Jagdbanngebiete*), wildlife rest zones (*Wildruhezonen*) during their protection period, and many nature reserves. Cantons and communes may be stricter |
| Sources | ZGB Art. 699 https://www.fedlex.admin.ch/eli/cc/24/233_245_233/de#art_699 ; SAC https://www.sac-cas.ch/de/umwelt/bergsport-und-umwelt/campieren-und-biwakieren/ (**Swiss Alpine Club, not government**); protection zones on map.geo.admin.ch |

#### France — Tier C for tents; national-park cores have their own sub-packs

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
| Wallonia | Bivouac zones exist; rules not separately verified |
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

#### Tier D (no official source verified yet)

TODO rows — do not write rules for these countries:

| Country | Tier | Notes | Sources |
|---|---|---|---|
| Croatia | D | TODO | not verified |
| Slovenia | D | TODO | not verified |
| Spain | D | TODO | not verified |
| Italy | D | TODO | not verified |
| Portugal | D | TODO | not verified |
| Greece | D | TODO | not verified |
| Hungary | D | TODO | not verified |
| Slovakia | D | TODO | not verified |
| Luxembourg | D | TODO | not verified |
| Liechtenstein | D | TODO | not verified |
| Every country not listed above | D | TODO | not verified |

Last verified: 2026-09

### 3.3 Where wild-camp suggestions are allowed

Apply the tier table in §3.2:

- **Tier A:** may run the road∩track algorithm with that pack’s filters and guidance.
- **Tier B:** may run only when the maintainer flag is ON and every listed
  condition is host-checkable and passes; otherwise treat as Tier C/D as stated
  for that pack.
- **Tier C:** never run the wild-camp algorithm; designated sites from host
  POI/area data only.
- **Tier D / unknown:** decline wild camp; suggest campsites only. Never fall
  back to Norway.

### 3.4 Design note (mandatory)

Right-to-roam is **not uniform**. Silent reuse of allemannsretten geometry +
150 m / 2-night / fire window elsewhere is a **spec violation**.

**No rule pack may borrow another country's distance, duration or fire rule;
vehicle overnight rules are never derived from right-to-roam.**

---

## 3.5 Vehicle overnight mode

Right-to-roam packs **never** authorise sleeping in a vehicle. Vehicle
suggestions use `vehicle_profile_read` and designated/road-side data only.

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

### All other countries

Vehicle rules are not researched. Suggest designated motorhome sites
(`tourism=caravan_site`) for campervans and signed truck parking for HGV
professional drivers only. Add a TODO per country.

| Country / region | Vehicle overnight | Sources |
|---|---|---|
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
6. **Vehicle cards** show the vehicle-class condition that was checked.
7. **Fire guidance** computed from the **current date** at display time (where
   the pack has a date-gated fire rule).
8. **Cloudberry** note only for Northern Norway candidates.
9. The **disclaimer** (§ top).

Ranking UI may show multiple candidates; each card must carry the full guidance
set for its location.

---

## 5. Filter vs guidance summary

| Item | Hard filter | Always / conditional guidance |
|---|---|---|
| Min distance from buildings (shared SafetyConfig; NO pack; SE labelled as Navi safety default) | Yes where pack requires | Show value used and whether it is law or safety default |
| max_nights per pack | Yes where pack hard-limits | Explain limit. **NO 2**; **DE / DK / IS / EE / Écrins 1**; **PL 2** (designated); **Scotland 2–3**; **SE / FI soft guidance** |
| Service-road seed quality | Soft rank / caution | Optional access note |
| Forest (DE, AT, CZ) | Yes | Explain forest ban / consent |
| Protected area (all packs, unless a park sub-pack applies) | Yes | Explain decline or switch to park sub-pack |
| Travel mode (DE Tier B) | Yes — non-motorised required | Show checked condition |
| Døgnhvileplass for non-professional profiles | Yes — hard exclude | — |
| Truck bays for car/campervan | Yes — hard exclude | — |
| DE roadside at/near destination or 2nd night | Yes — hard exclude | Card: interrupted journey only; no furniture/awning; "~10 hours" secondary |
| FR seashore / catchment / listed-site vehicle bans | Yes — hard exclude | — |
| Fire ban window NO 15 Apr–15 Sep | No (map can’t enforce permission) | Yes — date-gated text |
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
| Per-vehicle "same spot" key (German roadside 2nd-night rule) | Plugin-local KV/storage |
| Fire ban / date-gated calendar logic | Plugin (uses host clock) |
| Country / ISO 3166-2 / Nordland–Troms–Finnmark / park cores | Host `admin_region_read` |
| Vehicle class and professional-driver flag | Host `vehicle_profile_read` (user-set; never inferred from size) |
| Designated-site polygons / NVDB rest areas | Host POI/area data (never fetched from WASM) |

---

## 7. Implementation sketch (future; out of scope now)

1. Host exposes route corridor samples + optional junction list or edge pairs.
2. Guest ranks seeds, walks tracks, queries POIs for buildings, reads safety
   distance and admin region.
3. Apply country pack by tier; persist max_nights and DE vehicle same-spot keys;
   format suggestion payload for UI.
4. Android (or other) host renders pins + rule text; no WASM UI.
5. Vehicle overnight mode (§3.5) is a separate suggestion path using
   `vehicle_profile_read` and designated/road-side layers only.

---

## 8. Related documents

- [`plugins.md`](../plugins.md) — sandbox, capabilities, design rules  
- [`../jurisdiction-rules.md`](../jurisdiction-rules.md) — reusable country/region rule-pack pattern (this camping table is a grounding example)  
- [`../jurisdiction-sources.md`](../jurisdiction-sources.md) — every official URL cited in this spec, with a "verified on" date per link  
- [`../poi.md`](../poi.md) — POI categories / spatial index  
- Core: `SafetyConfig`, `SAFETY_MIN_BUILDING_DISTANCE_M`  
- [`../README.md`](../README.md) — Rest and overnight (*allemannsretten* default note)
