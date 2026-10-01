# Bad Bevensen → Dalsøren MobileHome campaign

## Latest run — 2026-10-01 afternoon (FAIL — no ship)

Branch tip at start: **`right-to-roam`** `@eb3e6037` (ferry overlay stub-skip /
pier approaches). Emulator: `Navi_8c_4G_128G` (8c / 4 GB / SD as-is). Runner:
`LongTripMobileHomeBevensenDalsorenUiCampaignTest`. Host ADB GPS + 6 synthetic
DATEX Blocks with re-pin. **No version bump / tag / APK refresh.**

Corrective re-run after leftover SD packs looked like pre-fetched installs:
Tools UI wipe of corridor (+ leftover) regions, then Plan with long trip ON so
corridor packs auto-download only (`downloads_mode=long_trip_auto_corridor_only`).
No manual Tools download taps.

| Metric | Result | EXPECTED | OK |
|---|---|---|---|
| Distance | **0 km** (corridor never ready; no plan) | 1461.3–1648.6 km | no |
| Driving time | **0** | 17–~22 h | no |
| Instructions | **0** | 55–100 | no |
| Ferries used | n/a | natural if competitive | n/a |
| Nearby attractions | n/a | several | n/a |
| Wild camping sites | n/a | reported | n/a |
| Rest places | n/a | name+coords | n/a |
| km/day | n/a | soft 6.0 h | n/a |
| Fuel stops (report-only) | n/a | 100 km margin / 70 L | n/a |
| DATEX host inject | **yes** (6 sits DE/DK/SE/NO; re-pin) | 3–8 synthetic | host ok |
| DATEX in plan | **no** (no plan) | applied | n/a |
| Plan | **HARD FAIL** — SE corridor stuck | found | no |

### Auto-download evidence

After Plan click, all 9 regions started as Downloading/Queued (none Installed).
Observed local-first order on SD `long-trip-packs`:

1. Niedersachsen → Installed (~3.5 min)
2. Schleswig-Holstein → Installed
3. Denmark → Installed
4–6. Skåne / Halland / Västra Götaland → **stuck Downloading**
7–9. Ostlandet / Sorlandet / Vestlandet → Installed (while SE still Downloading)

Packs landed under `/storage/0000-0000/Android/data/no.navi.app/files/long-trip-packs`
(removable). Every corridor `*-latest.osm.pbf` on disk is a **16 KiB pack-server
stub** (graphs/manifests present). `place-index-ready.json` stayed `[]`.

### Stall / hard-fail

From ~14:18 through stop (~15:38, **>80 min**): Skåne + Halland + Västra Götaland
remained `Downloading` while `no.navi.app` was **idle (0% CPU)**, no
`region-download.json` job, negligible network. Instrumentation would wait up to
20 h; run stopped as hard-fail after sustained idle stall. App left running;
instrumentation/host stopped.

RAM PSS: before ≈ **177 MiB**; during stall / final sample ≈ **420 MiB**.

### `graph_format_version` (from `https://navigate-me.duckdns.org/current.json`)

| Region | graph_format_version |
|---|---|
| europe/germany/niedersachsen | 8 |
| europe/germany/schleswig-holstein | 8 |
| europe/denmark | 9 |
| europe/sweden/skane | 9 |
| europe/sweden/halland | 9 |
| europe/sweden/vastra_gotaland | 9 |
| europe/norway/ostlandet | 9 |
| europe/norway/sorlandet | 8 |
| europe/norway/vestlandet | 8 |

### Process notes

- Ferry overlay fix already on tip (`eb3e6037`); cherry-picked to `dev` as
  `2532b0fa` earlier the same day.
- Mid-campaign **Tools SD-delete visibility** tweak landed in the working tree
  (app + androidTest) so UI wipe could clear removable packs — that violates the
  strict UI-phase “no app code” rule and must be reviewed/committed **separately**
  (not part of a ship). No further app edits while waiting on the plan.

Evidence: `/tmp/bevensen-ui-host/` (report.json, host.log, logcat, meminfo,
screenshots including `11_hard_fail_stall.png`).

---

## Earlier same-day morning run — 2026-10-01 (FAIL — Fehmarn disconnected)

Branch: **`right-to-roam`** (Fehmarn densify bias + tunnel tag retain). Emulator
`Navi_8c_4G_128G`. UI runner with host DATEX. Plan found leg1 then failed leg2
across Fehmarn (`bbox_exhausted` / `disconnected`; `graph_ferry_edges=0`). No ship.

---

Date: **2026-09-30**. Branch: **`right-to-roam`**. UI campaign PASS with
EXPECTED distance / duration / maneuvers and synthetic DATEX applied.

**Canonical role:** one-shot campaign evidence for the long-trip MobileHome path
on the fixed Automotive AVD (`Navi_8c_4G`). Real region packs on removable SD;
only DATEX closures are synthetic. Not wired into CI as a test job.

Instrumented runners:

- `LongTripMobileHomeBevensenDalsorenUiCampaignTest` — **primary** (this pass):
  plan via visible MainActivity Compose UI (From / Via / To / settings / Plan).
  Single natural via at Ottadal corridor; **no** forced corridor vias, land-bridge
  densify hops, ferries, or road segments. Host ADB only for GPS + DATEX inject
  (and re-pin so live NPRA poll cannot overwrite synthetics).
- `LongTripMobileHomeBevensenDalsorenCampaignTest` — earlier FFI assist path with
  multi-via corridor (superseded for the natural-via requirement).

---

## Verdict (UI natural-via run) — IN EXPECTED BAND

| Metric | Result | EXPECTED |
|---|---|---|
| Distance | **1616.8 km** | 1461.3–1648.6 km |
| Driving time | **~20.9 h** (1254 min) | 17–~22 h |
| Instructions | **57** | **55–100** |
| DATEX | **applied** (`datex_ok=true`; 6 synthetic Blocks; 6 legs with `datex_impacts=1`) | must happen |
| Plan | **found** (`ui_planned=true`, 14 chunk legs PASS) | found |

Release: **v0.3.9-beta** (`versionCode` 14).

### Puttgarden (DE) ferry — not used

External ECO-off reference polyline (`geojson-routes/bad-luster.json`):
**1436.9 km / ~17.9 h** with Fehmarn Belt ferry Puttgarden→Rødby.

| Path | Distance | Puttgarden ferry |
|---|---|---|
| Campaign (this pass) | **1616.8 km** | **no** (`route_uses_ferry=false`) |
| ECO-off reference GeoJSON | **1436.9 km** | **yes** |

Campaign had `avoid_ferries=false`. Some chunk graphs reported
`graph_ferry_edges>0`, but densify still prefers the Baltic land bridge
(Jutland/Zealand), so the Fehmarn ferry is not chosen. About
**1616.8 − 1436.9 ≈ 180 km** left vs that reference.

---

## Test setup (2026-09-30 UI run)

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G` (8 cores / 4 GB RAM / 128 GB internal / 512 GB SD) |
| Origin | Bad Bevensen Kurpark Stellplatz ≈ 53.079686, 10.587198 |
| Via (single, natural) | **61.8691419, 9.1055130** (Ottadal corridor) |
| Destination | Dalsøren Camping ≈ 61.4433766, 7.4614016 |
| Departure | `2026-06-01T08:00:00` local |
| Eco | requested on (Compose; chunk legs still log `use_eco=false` — known gap) |
| Avoid toll roads | off |
| Ferries | use (allow if natural — not forced); `route_uses_ferry=false` |
| Soft daily budget | 6.0 h |
| Soft break spacing | 1.5 h interval, 15 min rest |
| Wild camping / long trip / DATEX / nearby attractions | on |
| Profile | MobileHome / Truck routing + VW T6 camper limits |
| Plan path | Compose `btn_plan_route` |

Packs on removable SD:
`/storage/0000-0000/Android/data/no.navi.app/files/long-trip-packs`.

### Vehicle — VW Transporter T6 2.0 BiTDi 4Motion camper

| Spec | Value |
|---|---|
| Length | 5.304 m |
| Width (incl. mirrors) | 2.297 m |
| Body height | 2.477 m |
| Loaded total weight | 3020.4 kg |
| Loaded rear axle | 1661.2 kg |
| Fuel tank | 70 L (`FuelConfig` HUD only) |

---

## Corridor and packs (real downloads)

Download / install order (local first, then corridor toward dest):

1. `europe/germany/niedersachsen`
2. `europe/germany/schleswig-holstein`
3. `europe/denmark`
4. `europe/sweden/skane`
5. `europe/sweden/halland`
6. `europe/sweden/vastra_gotaland`
7. `europe/norway/ostlandet`
8. `europe/norway/vestlandet`

Fresh-corridor timing (earlier same-day UI download pass before this PASS_UI):
basemap PMTiles coalesce ~1.2 GB, then region packs. Approximate wall clock
from Queued→Installed: Niedersachsen ~15–20 min (incl. OSM PBF + wetland tiles);
Schleswig-Holstein through Västra Götaland ~10–15 min combined after DE;
Ostlandet re-fetch ~4 min when deleted. Indexing continued in background
(`Installed` then `Indexed`). This PASS_UI plan reused corridor Already
Installed/Indexed on SD (`pack_on_removable=true`).

`current.json` now exposes `graph_format_version` per region (rebake in progress;
this run mixed format 8/9 packs by region).

| Metric | Result |
|---|---|
| Pack on removable | **true** |
| Corridor ready | **true** |
| Planner densify hops | **14** (`chunk_deg=1.15`) |
| RAM (PSS) before / post-plan / final | ~180 / ~505 / ~491 MiB |

---

## Plan metrics (full route)

| Metric | Result |
|---|---|
| Distance | **1616.8 km** |
| ETA | **1254 min (~20.9 h)** |
| Maneuvers | **57** |
| Ferries used | **0** (`route_uses_ferry=false`) |
| Soft multi-day | **4 days** (budget 6.0 h) |

### Kilometers per day

| Day | Distance | Driving | Overnight |
|---|---|---|---|
| 1 | 464.1 km | 6.00 h | HumleoreHus (55.47509, 11.91095) lodging |
| 2 | 464.1 km | 6.00 h | Rösseliden 28 (57.92579, 11.61400) lodging |
| 3 | 464.1 km | 6.00 h | Nedre Berg Gård (61.03352, 10.51770) lodging |
| 4 | 224.4 km | 2.90 h | (arrival) |

### Rest places (soft break POIs on chunk legs)

| Name | Lat | Lon |
|---|---|---|
| Rest stop | 53.981997 | 10.238788 |
| Rest stop | 55.364206 | 11.246042 |
| Rest stop | 56.657424 | 12.906331 |
| Rest stop | 57.864547 | 11.973599 |
| Rest stop | 58.806559 | 11.224076 |
| Rest stop | 60.351303 | 10.581935 |
| Rest stop | 61.835685 | 9.274616 |

Chunked soft breaks: interval ~116 km; `chunked_rest_pauses=11`;
`break_pois_total=14`; overnight_candidates=509; rest_candidates=131.

### Attractions / wild camping

Nearby attractions are post-plan (POI look-ahead covering load over
`long-trip-packs/`), not densify-leg `poi_skipped=chunk_leg` scoring.
Wild camping: **on**; soft overnights resolved to **3 lodging** POIs on this
pass. Camping suggest uses Removable pack dirs after plan (segmented corridor
loads) so SD-only Ready packs are visible.

### Fuel-stop estimate (unimplemented planner)

Report-only (`FuelConfig` tank/fill for HUD). With full 70 L start and ~100 km
margin heuristic: `stops_at_500mi=2`, `stops_at_600mi=1`. No fuel-stop lookahead
inside `planCarRouteAt` (see `plugins/safety-resupply.md`).

---

## Synthetic DATEX (only synthetic inputs)

Six Block situations under `{dataDir}/datex_cache` with `apply_to_routing=1`.
Host ADB injects and **re-pins every ~20 s** so the live NPRA redistributor
cannot replace the synthetic snapshot before/during plan.

Coords are densify-chord midpoints on the Bevensen→Dalsøren path (within the
~1.5 km DATEX corridor margin). Ottadal→Dalsøren mountain choke is avoided
(earlier Block there caused `disconnected` on leg14).

| ID | Country | Lat/Lon | Label |
|---|---|---|---|
| syn-de-a7 | DE | 53.64485, 10.33915 | DE A7 chord (Bevensen–Kiel) |
| syn-dk-e45 | DK | 55.04625, 11.23625 | DK Great Belt approach |
| syn-se-e6 | SE | 56.42250, 12.96875 | SE Halland E6 |
| syn-se-gbg | SE | 58.30281, 11.32281 | SE Bohuslan coast |
| syn-no-e6 | NO | 60.17719, 10.64550 | NO E6 Ostlandet |
| syn-no-otta | NO | 61.56436, 9.66331 | NO toward Ottadal via |

**DATEX proof:** six chunk legs reported `datex_impacts=1; datex_block=1`
(legs 1, 3, 6, 8, 10, 12). `datex_ok=true`.

---

## How to re-run (not CI)

```bash
# Host: GPS + DATEX inject/re-pin (see /tmp/bevensen_host.py pattern)
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.LongTripMobileHomeBevensenDalsorenUiCampaignTest
```

Evidence JSON: app / SD
`files/long-trip-bevensen-dalsoren-ui.json` and
`long-trip-bevensen-dalsoren/report.json`.

---

## Prior notes (not this pass)

- **2026-09-30 UI after soft-pull densify (`ddcf8469`)**: **1631.9 km / ~21.1 h /
  55 maneuvers** — in band; DATEX soft (1 impact) on older sit coords.
- **2026-09-30 UI after westbound mid fix (`9997e310`)**: **1660.9 km / ~21.4 h /
  305 maneuvers** — duration in band; maneuvers far above 55–100.
- **2026-09-30 FFI multi-via**: **2035.8 km / ~28.4 h / 380 maneuvers** — OOB;
  forced corridor vias.
- Live NPRA overwrite of synthetic DATEX caused Vestlandet leg14
  `disconnected` until host re-pin kept synthetics on disk.

---

## Known gaps

- Puttgarden ferry still unused (~180 km vs ECO-off ferry reference) because
  densify Baltic land-bridge policy steers west of Fehmarn.
- Eco Compose switch not reliably reflected in chunk `use_eco` flags.
- Fuel-stop planning unimplemented (`FuelConfig` HUD only).
- Tools UI `DownloadedRegionDelete.blockReason` does not see SD-only
  long-trip-packs (precheck can report “Nothing installed” while corridor
  still shows Installed on removable). Delete clicks still attempted via UI.
- `graph_format_version` in `current.json` / mid-rebake: mixed format 8/9 packs
  across corridor regions this day.
