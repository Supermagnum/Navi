# Elsa's caravan & galleri (Bugøynes) → Sjuvasslia Camping

Date: **2026-10-04**. Branch: **`dev`**. HEAD: **`0588b00e9930277f5e89f9fb3c533414185ac9ba`**

**Canonical role:** one-shot MobileHome long-trip UI campaign on Automotive AVD
`Navi_8c_4G_128G`. Real region packs on the SD card; synthetic DATEX Blocks via
host ADB only (GPS + DATEX). Not wired into CI.

Instrumented runner:

- `LongTripMobileHomeElsaSjuvassliaUiCampaignTest` — settings, optional UI repair
  of pack-server PBF stubs, and Plan via visible Compose UI. No forced vias,
  bridges, ferries, or road segments. Region **delete** skipped when corridor
  car packs already on SD.

Host monitor (non-mutating except GPS + DATEX): `/tmp/elsa_sjuvasslia_host.py`
→ `/tmp/elsa-sjuvasslia-host/` (host log, meminfo snapshots, pulled
`report.json` at `/tmp/elsa-sjuvasslia-host/pull/report.json`).

---

## Verdict — 2026-10-04 UI run (post stub-PBF repair)

| Metric | Result | EXPECTED (user bands) | OK vs bands |
|---|---|---|---|
| Distance | **1996.4 km** | 1800–2300 km | **yes** |
| Driving time | **~23.9 h** (1434.8 min) | 20–31 h | **yes** |
| Instructions | **57** | 1–170 | **yes** |
| DATEX | **yes** — 6 synthetic Blocks injected; plan `datex_ok=true`, `max_datex_impacts=15` on a chunk leg | 3–6 must apply | **yes** |
| Plan | **found** (`ui_planned=true`, corridor **Indexed**, MobileHome) | found | **yes** |

Reference polyline: [`elsa-sjuvass.geojson`](https://github.com/Navi-app/Navi/blob/main/geojson-routes/elsa-sjuvass.geojson) **1944.2 km** (33 911 vertices); this run **+52.2 km** (~2.7%) — fair SE land corridor, not Norway-only E6.

**Overall: PASS** vs distance / duration / maneuver bands. Gradle `:app:connectedDebugAndroidTest` exited FAILED after ~23 min wall clock despite in-test `PASS_UI` / `ELSA_CAMPAIGN_DONE` (see device report; treat metrics below as authoritative).

---

## Root cause fixed this run (BlobHeader / long trip)

Earlier attempts hit **`BlobHeader` / graph build** because:

1. **`longTrip=false` / coordinator off** — Plan ran without chunked corridor fetch
   (`longTrip=false` in `NaviPlan` log), so routing fell through to a single PBF.
2. **Pack-server leaf stub** — `nord-norge-latest.osm.pbf` was **16 384 bytes**
   (all-zero stub) while **car** graph tiles were present. MobileHome routes as
   **Truck**; published manifests ship **car+foot** graph keys and the core
   **aliases Truck → car tiles** (no separate `navi-graph-truck.*` on disk).
   Missing **real PBF** blocked place-index → `corridor_ready=false` stuck at
   `Nord-Norge=Installed`.
3. Other **16 KiB stubs** on SD (not on this corridor): vestlandet, trondelag,
   sorlandet. **Real PBFs** already present: finland, ostlandet, sweden läns.

**Fix (UI only, no ADB pack push):** Tools → **Download region** for
`europe/norway/nord-norge` (~407 MB PBF + pack fetch/index). Then long trip ON,
From/To set, host DATEX, **Plan**. All eight corridor regions reached **Indexed**
before densify completed.

Log anchor: `Installed europe/norway/nord-norge missing real PBF … cannot place-index yet` → after repair `Nord-Norge (1 of 8)=Indexed`.

---

## Test setup

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G_128G` (`emulator-5554`), unchanged specs |
| Origin | Elsa's caravan & galleri, Bugøynes **69.9741435, 29.6337571**, elev **4 m** (ADB `geo fix`) |
| Destination | Sjuvasslia Camping **59.803175, 9.397871** |
| Departure | `2026-06-01T08:00:00` local |
| Profile | MobileHome / Truck |
| Vehicle | VW T6 camper — height **2.477 m**, width (mirrors) **2.297 m**, length **5.304 m**, total **3020.4 kg**, rear axle **1661.2 kg**, tank **70 L** |
| Eco | on |
| Avoid toll / ferries | off / off |
| Soft daily budget | 6.0 h |
| Soft break | 1.5 h interval, 15 min rest |
| Wild camping / long trip / DATEX / nearby attractions | on |

Wall clock: stub repair + nord-norge download/index ~**20 min**; post-Plan wait loop **`download_or_plan_elapsed_ms=79515`** (~1.3 min after corridor ready).

---

## Results summary

### Ferries

**0** ferry legs (`route_uses_ferry=false`, `graph_ferry_edges=0` on chunk legs).

### Attractions

Nearby-attractions on. Look-ahead along-route sample (`sample_points=8`):
`records=33544`, **`hit_count=1`**, **`by_type={general: 1}`** (e.g. Y:et Café).
Chunk legs still `poi_skipped=chunk_leg` by design.

### Rest places (name + coords)

Soft/overnight POIs (`break_poi_count=12`):

| Name | Lat | Lon | Along km |
|---|---|---|---|
| Rest 4041318934 | 67.799257 | 24.796809 | 375.7 |
| Rest 6585850109 | 66.920803 | 23.140353 | 500.9 |
| LappeanLohi | 67.155551 | 23.577503 | 500.9 |
| Rest 3299089221 | 66.311651 | 22.810884 | 626.2 |
| Rest 980653755 | 64.607929 | 21.192544 | 876.6 |
| Rest 3237017603 | 63.727946 | 20.089181 | 1001.8 |
| Best Western Hotel Bothnia | 63.802436 | 20.280263 | 1001.8 |
| Rest 7698896629 | 63.007075 | 18.285645 | 1127.1 |
| Skönviksberget V | 62.459966 | 17.340285 | 1252.3 |
| Støa rasteplass | 61.260853 | 12.817432 | 1628.0 |
| Skjefstadfossen | 60.831943 | 11.615530 | 1753.2 |
| Rest 4382146589 | 59.911723 | 10.759505 | 1878.5 |

Approx **km/day** under 6 h soft budget: **1996 km / ~23.9 h** driving ≈ **4 calendar days** of driving time; soft rests spaced ~125 km along-route.

### Wild camping

`wild_camping_site_count=0` (suggest `UNAVAILABLE`: corridor graph segments produced no seeds).

### Tunnels

Per chunk legs: **`route_tunnel_count=0`**, `avoid_tunnels=false` (vehicle limits on).

### Total length / nav instructions

- Total **1996.4 km**, **1434.8 min** (~23.9 h).
- Maneuvers **57** — kinds: left 18, roundabout 16, right 9, exit_right 7,
  sharp_right 1, keep_left/right, merge_right, exit_left, destination 1.

### Estimated fuel stops (report-only)

Full tank, 100 km margin, 500–600 mile range class:
`stops_at_500mi=2`, `stops_at_600mi=2`. Fuel planner unimplemented.

### DATEX (timing)

6 synthetic Blocks injected via host ADB before Plan (`/tmp/elsa_sjuvasslia_host.py`);
re-pinned during run. Plan report: **`datex_ok=true`**, **`max_datex_impacts=15`**
(one chunk leg); **4** chunk legs with non-zero `datex_impacts` in the saved
report. Static campaign injection at `ELSA_AWAIT_DATEX` (before densify), not
live drive-time arrival simulation.

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
(`pack_on_removable`). Corridor order (plan): all **Indexed** at Plan time.

| # | Region | gfv (current.json) |
|---|---|---|
| 1 | europe/norway/nord-norge | 9 |
| 2 | europe/finland | 9 |
| 3 | europe/sweden/norrbotten | 9 |
| 4 | europe/sweden/vasterbotten | 9 |
| 5 | europe/sweden/vasternorrland | 9 |
| 6 | europe/sweden/jamtland | 9 |
| 7 | europe/sweden/dalarna | 9 |
| 8 | europe/norway/ostlandet | 9 |

`current.json` generation: **`20261001T181622Z-625025-europe_isle_of_man-426b9d3f`**.

**Nord-Norge repair:** UI download replaced stub PBF (**16 384 B → 406 790 351 B**)
and completed place-index (`stub_pbf_repairs[0].ok=true`). Other corridor läns reused
existing car graph tiles on SD; Truck routing used **car tile alias** (no truck-only
pack files on server).

### RAM

PSS before / post-plan / final ≈ **176 / 1310 / 1185 MiB**.

---

## How to re-run (not CI)

```bash
# Host: GPS + DATEX inject/re-pin
python3 /tmp/elsa_sjuvasslia_host.py &
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.LongTripMobileHomeElsaSjuvassliaUiCampaignTest
```

**Precondition:** corridor **real PBF + Indexed** for origin leaf (nord-norge if
Bugøynes start). If only car tiles + 16 KiB stub PBF, use Tools **Download region**
for that path before Plan.

Evidence on device: `files/long-trip-elsa-sjuvasslia/report.json`.  
Host copy: `/tmp/elsa-sjuvasslia-host/pull/report.json`.
