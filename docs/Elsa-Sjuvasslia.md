# Elsa's caravan & galleri (Bugoynes) -> Sjuvasslia Camping

Date: **2026-10-05**. Automotive AVD **`Navi_8c_4G_128G`** as-is (8 cores, 4 GB RAM, 128 GB internal, 512 GB SD). Morning campaign: app process **`no.navi.app` pid 30162**. Later verify: **pid 18674**. No curl/wget/adb-push of region packs. No emulator spec changes. No git commit.

**Canonical role:** one-shot MobileHome long-trip UI campaign. Distance / duration / maneuver **bands and morning measured km/time/maneuver numbers** below stay as recorded at 08:38. Wild-camping count is from the later suggest that produced **76** accepted sites.

---

## Verdict

| Metric | Result | EXPECTED | OK vs bands / spec |
|---|---|---|---|
| Distance | **1994.626 km** (`NaviRouting planning_done`, dump `distance_km`) | 1800-2300 km | **YES** |
| Driving time | **1426.05 min = 23.77 h** (`eta_minutes`) | 20-31 h | **YES** |
| Instructions / maneuvers | **55** | 1-170 | **YES** |
| DATEX | **6** synthetic Blocks in `datex_cache` via ADB (08:12). Overlay ON. After plan: duckdns refresh `active=85` (real NPRA cache; synthetic file left on disk). HUD earlier: `DATEX impacts: 0 block, 0 penalize` | 3-6 must happen | **injected**; **0 block/penalize** on the finished plan |
| Wild camping sites used | **76** accepted / `kind=OK` (`on_foot=76`, `vehicle=0`; `json_bytes=153913`) | > 0 | **YES** |
| Plan | Elsa (Sagveien 4) -> 59.80318, 9.39787; `terminate=found`; `pack_hit=true` | found | **YES** |

**Overall:** wild-camping > 0 **PASS** (**76**). Distance / duration / maneuver bands **pass** (morning 1994.626 km / 23.77 h / 55). Attractions "several" **not met** (morning: 1 viewpoint). DATEX 3-6 synthetic was the morning injection.

---

## Nested driver / stalls (same session)

| Event | What happened |
|---|---|
| Nord-Norge PMTiles 07:20-07:35 | UI frozen ~1% (`25032407/1973055604`); file stuck at 1888836381 B. Hung Protomaps extract, not duckdns 500. Tools **Download region** resumed duckdns packs 181/181 ~07:35-07:39. Stall **~15 min** zero byte growth. |
| Corridor fetch | Continued via long-trip UI onto SD. Finland + Swedish lan + Ostlandet. `place-index-ready.json` had all **8** leaves at 08:33 before the kept plan. |

---

## Test setup (as used)

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G_128G` (`emulator-5554`) |
| Origin | Elsa / Sagveien 4, Neset, Sor-Varanger. GPS `adb emu geo fix 29.6337571 69.9741435 4`. HUD Alt **4 m** |
| From UI | Sagveien 4, Neset, Sor-Varanger |
| To UI | **59.80318, 9.39787** |
| Via | none |
| Departure 2026-06-01T08:00:00 | **No Compose field.** Device clock 2026-10-05. DATEX synthetic validity used June 2026 dates. |
| Profile | **Mobile home** (`planning_start profile=mobile_home`) |
| Eco | **ON** (`planning_done eco=true`; HUD leaf) |
| Avoid toll / ferries / motorways / tunnels | All **OFF** on drive-settings text. Plan `route_uses_tolls=true`, `toll_policy=allow` |
| Soft daily 6.0 h | **No Compose field.** Multi-day UI used **8.0 h** days |
| Break | HUD **Break in 240 min** (4 h). 1.5 h / 15 min fields **not confirmed** |
| Vehicle | Limits **saved** this session: height **2.477 m**, width **2.297 m**, length **5.304 m**, axle **1661.2 kg**. Total weight UI none. Tank 70 L report-only |
| Wild camping | Pref ON; **76** accepted / kind=OK |
| Long trip | ON; packs on SD |
| DATEX | ON; Wi-Fi only. Synthetic XML + later duckdns overlay |
| Nearby attractions | Pref ON. Stats: **1** (`tourism-viewpoint`) |

---

## Regions (order, gfv, placement)

`current.json` generation **`20261005T052422Z-1435041-776c294b`**. Listed leaves **`graph_format_version: 9`**.

Long-trip packs: **`/storage/0000-0000/Android/data/no.navi.app/files/long-trip-packs`** (~9.1 GiB at 08:30). Place index / Tools copies also on **internal** `files/`. PBFs seen on SD: `nord-norge-latest.osm.pbf`, `finland-latest.osm.pbf`, `sweden-latest.osm.pbf`. Plan used `pbf=/data/user/0/no.navi.app/files/nord-norge-latest.osm.pbf` and `packDir=` SD.

| # | Region | gfv | Server bytes | This run |
|---|---|---|---|---|
| 1 | europe/norway/nord-norge | 9 | 881603816 | Indexed (PMTiles stall then pack retry). SD + internal |
| 2 | europe/finland | 9 | 5297463968 | Indexed (slow place-index). SD `finland-latest.*` |
| 3 | europe/sweden/norrbotten | 9 | 341668937 | Indexed |
| 4 | europe/sweden/vasterbotten | 9 | 263639558 | Indexed |
| 5 | europe/sweden/vasternorrland | 9 | 265793335 | Indexed. After plan, HUD once: offline data "no longer available" for this leaf (warning only; plan already used packs) |
| 6 | europe/sweden/jamtland | 9 | 255025531 | Indexed |
| 7 | europe/sweden/dalarna | 9 | 295460682 | Indexed last (~08:33) |
| 8 | europe/norway/ostlandet | 9 | 2624434047 | Indexed (already present internally) |

`place-index-ready.json` at plan time: all eight ids above.

---

## Plan results

Kept plan: **08:38:20** `planning_done eco=true duration_ms=70549 distance_km=1994.626` `expansions=322338 terminate=found`. First plan (~08:34) was a 4-day 661.6 km split; vehicle save + a second Plan from the search sheet produced the 3-day numbers below. **Do not add** the two plans.

### Multi-day (HUD)

| Day | Distance | Driving | Overnight |
|---|---|---|---|
| 1 | 671.4 km | 8.0 h · 0-671 km | Motell Tore |
| 2 | 671.4 km | 8.0 h · 671-1343 km | Hussborgs herrgard |
| 3 (arrival) | 651.9 km | **~7.77 h** (from remaining ETA; HUD truncated the hours line) | destination |

Sum 671.4+671.4+651.9 = **1994.7 km** (matches 1994.626). Total drive **23.77 h** vs soft 6 h/day (no UI): HUD used 8 h days, **3 days** not 4.

### Ferries

`route_uses_ferry=false`, `route_ferry_legs=0`. Native also logged ferry overlay on nord-norge (`ferry_edges=13`) and a densify `preparing ferry data for finland` miss; the **finished** motor path still reports **no ferry legs**.

### Attractions

**1** (`tourism-viewpoint 1`). Spec "several" **not met**.

### Rest places (name + coordinates)

`NaviRouting planning_pois count=6 names=Rest 3297560855|Rest 3237017603|Rest 3581979327|Ljordalen|Motell Tore|Hussborgs herrgard kinds=rest_area|rest_area|rest_area|rest_area|lodging|lodging`.

`rest_place_count=6`. Maneuver dump overnight stops have **lat=null lon=null** for both lodgings. OSM rest ids were **not** paired with coordinates on the HUD or dump. **Honest: names known, coordinates not shown.**

### Wild camping

**76** accepted sites (`rejected=4`, `vehicle=0`, `on_foot=76`), `NaviCamping` **kind=OK**, `via=wasmtime`, `json_bytes=153913`. HUD: **Vehicle overnight** empty ("No vehicle overnight spots along this corridor."); **On foot from here** showed cards (first visible: `69.96646, 29.61672 · walk 0 m · access track`, Tier A · NO). Spec > 0 **PASS**.

A later plan at 08:52 logged `planning_done eco=false duration_ms=64479 distance_km=2003.589` (`expansions=472126 terminate=found pack_hit=true`; maneuvers 55; `eta_minutes=1452.80`). Distance **2003.589 km** is still inside 1800-2300; it is **not** a replacement of the morning 1994.626 km eco=true figure in the verdict distance row.

### Tunnels

`route_tunnel_count=0` in dump / stats. Report **0** as enumerated, not "unknown".

### Navigation instructions

**55** maneuvers. First: sharp_right Ostersandveien (69.97297, 29.63814). Last: destination (59.80336, 9.39827). Mix: left 18, roundabout 15, right 8, exit_right 7, plus keep/merge/exit_left/destination. E 4 / E 6 appear on the Swedish/Norwegian trunk.

### Estimated fuel stops (report-only; planner unimplemented)

Tank 70 L. Using the campaign formula (start-full, 100 km reserve): usable **704.7 km** at 500 mi range, **865.6 km** at 600 mi. For D=1994.626 km: `ceil((D - usable) / usable)` -> **2** stops (500 mi) or **2** stops (600 mi: ceil(1.31)=2). Not a live HUD fuel list.

---

## DATEX

Spec: **3-6 synthetic** must happen. This campaign **injected 6** synthetic **MaintenanceWorks** Blocks (`NAVI-SYNTH`) via ADB **08:12** into `files/datex_cache/`: `datex-GetSituation.xml` (12377 B), `datex-cache.json`, `apply_to_routing=1`. Plugin ON; Wi-Fi only.

| Id | Country | Lat | Lon | Timing vs arrival |
|---|---|---|---|---|
| syn-se-inari | FI | 69.44041 | 28.41524 | **Not captured.** GPS stayed at origin. Not staged 30-5 min before site arrival (no driven ETA along the line). |
| syn-se-pajala | SE | 67.79923 | 24.93191 | same |
| syn-se-skelleftea | SE | 66.00533 | 22.56261 | same |
| syn-se-ornskoldsvik | SE | 63.57672 | 19.64161 | same |
| syn-se-sveg | SE | 62.29125 | 15.13323 | same |
| syn-no-elverum | NO | 60.62109 | 11.26339 | same |

Countries recorded: **FI, SE, SE, SE, SE, NO**. June 2026 validity dates (test-setup DATEX synthetic; device clock 2026-10-05).

After the kept plan: `NaviDatex refresh source=server-duckdns overlay=true active=85` (`source.json unchanged; skipping GetSituation.xml`). HUD / drive-settings: `DATEX impacts: 0 block, 0 penalize`. Synthetic files were left on disk; the live overlay count is NPRA-scale (**85**). **Routing impact on this plan was zero.** The 3-6 synthetic requirement is the **injection** (6 on disk), not 3-6 blocks applied to the finished route.

## RAM

| When | TOTAL PSS | Notes |
|---|---|---|
| ~07:26 | ~332 MiB | PMTiles UI frozen |
| ~07:56 | ~419-1211 MiB | Finland index |
| ~08:16 | ~945 MiB | Corridor still fetching |
| **08:38 after plan** | **~756-778 MiB** | App not killed (pid 30162) |
| **08:51 during later plan** | **~653 MiB** PSS; native heap **~512 MiB** | pid 18674 |
| **08:52 after suggest OK** | **~570 MiB** PSS; native heap **~363 MiB** | not LMK-killed |

OOM: **no LMK/AndroidRuntime kill** of pid 30162 (morning) or pid 18674 (later).

---

## Comparison vs `elsa-sjuvass.geojson`

File is not in this mirror tree. Copy used: `/tmp/elsa-sjuvasslia-host/elsa-sjuvass.geojson` (from sibling `Navi/geojson-routes/`).

| | Reference geojson | This campaign plan |
|---|---|---|
| Vertices | **33911** | not exported as GPX. Polyline **979884 chars** (`lon,lat;...`) |
| Length | **1944.235 km** | **1994.626 km** |
| Delta length | | **+50.391 km (~2.59%)** |
| Start | lon,lat 29.63342, 69.9742 | maneuver start 29.63814, 69.97297 (~0.2 km class) |
| End | 9.39866, 59.80326 | dest 9.39827, 59.80336 |
| Max/mean offset | | **not computed** (no GPX pull; would need the full polyline) |
| Ferry | no ferry property | plan **0** legs |
| Via | none | none |
| Corridor | Bugoynes-Finland-Sweden-Ostlandet land | same: E6/92 into FI, E4 Sweden, E6 Ostlandet. First/last maneuvers match that land spine |

---

## Honest gaps

- Wild camps > 0: **PASS** (**76** accepted).
- Attractions "several": **FAIL** (count 1).
- DATEX 3-6 synthetic: **written**; **not** 30-5 min vs driven arrival; **0** routing penalties.
- Soft 6.0 h and departure ISO: **no UI**.
- Fuel stops: formula only.
- Rest coordinates: **not in dump**.
- Geometry offset vs geojson: **not computed**.
- Second Plan after vehicle save replaced a 4-day 661.6 km split with the 3-day 1994.626 km plan reported here.
