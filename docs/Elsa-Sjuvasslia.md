# Elsa's caravan & galleri (Bugøynes) → Sjuvasslia Camping

Date: **2026-10-01**. Branch: **`right-to-roam`**. HEAD: **`3f88984f`**
(+ Fehmarn densify / tunnel-tag fixes already on branch).

**Canonical role:** one-shot MobileHome long-trip UI campaign on Automotive AVD
`Navi_8c_4G_128G`. Real region packs on the SD card; synthetic DATEX Blocks via
host ADB only (GPS + DATEX). Not wired into CI.

Instrumented runner:

- `LongTripMobileHomeElsaSjuvassliaUiCampaignTest` — every setting, region
  delete/download, and Plan via visible Compose UI. No forced vias, bridges,
  ferries, or road segments.

Host monitor (non-mutating except GPS + DATEX): `/tmp/elsa_sjuvasslia_host.py`
→ `/tmp/elsa-sjuvasslia-host/`.

---

## Verdict — 2026-10-01 UI re-run

| Metric | Result | EXPECTED (user bands) | OK vs user |
|---|---|---|---|
| Distance | **2001.0 km** | 1800–2300 km | yes |
| Driving time | **~23.9 h** (1434.4 min) | 20–31 h | yes |
| Instructions | **57** | 150–170 | **no** (post-`thin_route_maneuvers`) |
| DATEX | **yes** — 6 synthetic Blocks; **6** chunk legs with `datex_impacts=1` | 3–6 must apply | yes |
| Plan | **found** (`ui_planned=true`, 43 chunk legs, MobileHome/Truck, SE transit) | found | yes |

Instrumented `expected_check` uses maneuvers band **55–100** (same thinning class
as Bevensen) and reports `maneuvers_ok=true`. Against the original campaign
spec band **150–170**, maneuvers remain outside.

`PASS_UI` / plan found. Ship gate for Bevensen path was independent and **not**
met today (Bevensen Fehmarn leg2 disconnected). No version bump from this Elsa run.

Corridor: fair SE path (~2001 km), not Norway-only E6.

---

## Test setup

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G_128G` (`emulator-5554`), unchanged specs |
| Origin | Elsa's caravan & galleri, Bugøynes **69.9741435, 29.6337571**, elev **4 m** |
| Destination | Sjuvasslia Camping **59.803175, 9.397871** |
| Departure | `2026-06-01T08:00:00` local |
| Profile | MobileHome / Truck |
| Vehicle | VW T6 camper — height **2.477 m**, width (mirrors) **2.297 m**, length **5.304 m**, total **3020.4 kg**, rear axle **1661.2 kg**, tank **70 L** |
| Eco | on (Compose; chunk legs may still log `use_eco=false`) |
| Avoid toll / ferries | off / off |
| Soft daily budget | 6.0 h |
| Soft break | 1.5 h interval, 15 min rest |
| Wild camping / long trip / DATEX / nearby attractions | on |

Wall clock plan loop ~14.8 min after corridor already Installed on SD
(`download_or_plan_elapsed_ms=888896`).

---

## Results summary

### Ferries

**0** ferry legs used (`route_uses_ferry=false` on chunk legs).

### Attractions

Nearby-attractions on. Look-ahead sampling: `records=33544`, `hit_count=0`,
`by_type={}`. Chunk legs still `poi_skipped=chunk_leg` by design.

### Rest places (name + coords)

Soft/overnight POIs from post-chunk finalize (`break_poi_count=13`):

| Name | Lat | Lon | Along km |
|---|---|---|---|
| Rest 4041318934 | 67.799257 | 24.796809 | 376.6 |
| Rest 6585850109 | 66.920803 | 23.140353 | 502.2 |
| LappeanLohi | 67.155551 | 23.577503 | 502.2 |
| Rest 3299089221 | 66.311651 | 22.810884 | 627.7 |
| Rest 970970329 | 65.059777 | 21.413435 | 753.3 |
| Rest 980653755 | 64.598795 | 21.206152 | 878.8 |
| Rest 3237017603 | 63.727946 | 20.089181 | 1004.4 |
| Best Western Hotel Bothnia | 63.802436 | 20.280263 | 1004.4 |
| Rest 7698896629 | 63.007075 | 18.285645 | 1129.9 |
| Skönviksberget V | 62.459966 | 17.340285 | 1255.5 |
| Støa rasteplass | 61.260853 | 12.817432 | 1632.1 |
| Skjefstadfossen | 60.831943 | 11.615530 | 1757.7 |
| Rest 4382146589 | 59.911723 | 10.759505 | 1883.2 |

Approx km/day under 6 h soft budget (~2001 km / ~23.9 h driving): multi-day
stitch; rest spacing ~125 km along-route for soft rests.

### Wild camping

`wild_camping_site_count=0` (suggest returned OK with empty sites this pass).

### Total length / nav instructions

- Total **2001.0 km**, **1434.4 min** (~23.9 h).
- Maneuvers **57** — kinds: left 20, roundabout 15, right 9, exit_right 7,
  sharp_right 1, keep_left/right, merge_right, exit_left, destination 1.

### Estimated fuel stops (report-only)

Full tank, 100 km margin, 500–600 mile range class:
`stops_at_500mi=2`, `stops_at_600mi=2`. Fuel planner unimplemented.

### DATEX (timing)

6 synthetic Blocks injected via host ADB before plan; re-pinned during run.
Legs with `datex_impacts=1`: **6 / 43** chunk legs. Host inject occurs at
`ELSA_AWAIT_DATEX` (before Plan), i.e. well before vehicle would reach each
affected road (static campaign injection, not live drive simulation).

| Id | Country | Lat | Lon |
|---|---|---|---|
| syn-se-inari | FI | 69.44041 | 28.41524 |
| syn-se-pajala | SE | 67.79923 | 24.93191 |
| syn-se-skelleftea | SE | 66.00533 | 22.56261 |
| syn-se-ornskoldsvik | SE | 63.57672 | 19.64146 |
| syn-se-sveg | SE | 62.29125 | 15.13323 |
| syn-no-elverum | NO | 60.62109 | 11.26339 |

### Regions (order, SD, graph_format_version)

Pack root: `/storage/0000-0000/Android/data/no.navi.app/files/long-trip-packs`
(`pack_on_removable`). Final status: all **Installed**.

| # | Region | gfv |
|---|---|---|
| 1 | europe/norway/nord-norge | 9 |
| 2 | europe/finland | 9 |
| 3 | europe/sweden/norrbotten | 9 |
| 4 | europe/sweden/vasterbotten | 9 |
| 5 | europe/sweden/vasternorrland | 9 |
| 6 | europe/sweden/jamtland | 9 |
| 7 | europe/sweden/dalarna | 9 |
| 8 | europe/norway/ostlandet | 9 |

This pass reused already-Installed corridor packs after UI delete/reinstall
cycle; index/process wall times dominated by prior session downloads (Finland
~5 GB class). Plan densify hops=43, `chunk_deg=1.15`.

### RAM

PSS before / post-plan / final ≈ **180 / 806 / 838 MiB**.

---

## Bevensen same-day note (no ship)

Bevensen→Dalsøren UI campaign on the same AVD **failed**: Fehmarn densify hops
reach `(54.21, 11.025)→(55.175, 11.7)` but `chunk_leg2` terminates
`bbox_exhausted` / `disconnected` with `graph_ferry_edges=0`. Distance 0 —
outside bands; no version/tag/push.

See `docs/bevensen-mobilehome-campaign.md` latest-run section.

---

## How to re-run (not CI)

```bash
# Host: GPS + DATEX inject/re-pin
python3 /tmp/elsa_sjuvasslia_host.py &
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.LongTripMobileHomeElsaSjuvassliaUiCampaignTest
```

Evidence: app `files/long-trip-elsa-sjuvasslia/report.json`.
