# Bad Bevensen → Dalsøren MobileHome campaign

## Latest run — 2026-10-01 night (FAIL — OUT OF BAND; plan completed)

Tree on **`right-to-roam`** with Rayon hang fix (`pbf_priority`), Fehmarn ferry
overlay pier/hop gate (`bbox_build` / `indexed/load`), Indexed-before-plan gate
(`orchestrate` + `LongTripCoordinator` / `MainActivity`), and Geofabrik harden
`c8457d6f`. Native rebuild + APK install before UI; **zero** app/plugin code
changes during the UI phase. Emulator: `Navi_8c_4G_128G` as-is. Runner:
`LongTripMobileHomeBevensenDalsorenUiCampaignTest`. Host ADB GPS + **6**
synthetic DATEX Blocks with re-pin. Evidence: `/tmp/bevensen-ui-host/`
(`report.json`, logcat, host.log). Terminal:
`PASS_UI dist=2287.196 etaMin=2108.4 man=56 datex=true` (~22:51).

UI wipe of corridor (+ leftover) regions, then Plan with long trip ON
(`downloads_mode=long_trip_auto_corridor_only`). No manual region downloads.

| Metric | Result | EXPECTED | OK |
|---|---|---|---|
| Distance | **2287.2 km** | 1461.3–1648.6 km | **no (OOB high)** |
| Driving time | **35.14 h** (`eta_min=2108.4`) | 17–~22 h | **no (OOB high)** |
| Instructions | **56** | 55–100 | yes |
| Ferries used | **yes** (`route_uses_ferry=true`; legs 2/7/8/16/20) | natural if competitive | ok |
| Nearby attractions | not broken out in report | several | n/a |
| Wild camping sites | not broken out in report | reported | n/a |
| Rest places | 18 soft rests + lodging overnight marks in chunked report | name+coords | partial |
| km/day | soft 6.0 h → 6 motor days (390.5 km ×5 + 334.5 km) | soft 6.0 h | budget ok / path long |
| Tunnels used | `avoid_tunnels=false`; not enumerated | reported | n/a |
| Fuel stops (report-only) | 2287 km → ~3 at 500 mi / ~2 at 600 mi | 100 km margin / 70 L | report-only |
| DATEX host inject | **yes** (6 sits) | 3–8 synthetic | host ok |
| DATEX in plan | **yes** (`max_datex_impacts=1` on chunk_leg5) | applied | partial |
| Plan | **completed** (Fehmarn leg2 PASS with ferry) but **OUT OF BAND** distance/time | in-band | **FAIL band** |

### Notes

- Fehmarn/Baltic water gap is **routable** on this build (leg2 ferry;
  `graph_ferry_edges=102` on that leg). Prior same-day Fehmarn `disconnected`
  fail is superseded for connectivity.
- OOB length (~640–820 km over band) is a **routing geometry / corridor**
  issue (long detours, e.g. large snap/TripAabb expansions on later legs), not
  a hang or download failure.
- Post-`corridor_ready` wall still dominated by Sweden-style single-country
  `sweden-latest` ferry_overlay scans — see below.
- Elsa→Sjuvasslia not started (Bevensen out-of-band gate).

### Auto-download / Indexed gate

Corridor reached all-**Indexed** before plan start (new gate). Vestlandet
finished Indexing ~22:19; plan progress stayed `distance_km=0` through
Sweden PBF ferry_overlay densify until ~22:51 completion.

RAM: before ≈ **185 MiB** PSS; post-plan ≈ **765 MiB**.

---

## Earlier same night — 2026-10-01 late evening (FAIL — Fehmarn leg2; Elsa not started)

Hardened tree (uncommitted) on **`right-to-roam`** tip `56314990` + Geofabrik
502-retry / dated-URL harden from agent bd50cfee (`http.rs` /
`GEOFABRIK_EXTRACT_RETRIES=6`, HTML→Location→YYMMDD probe; `region.rs` wiring).
Native `libnavi.so` rebuilt and APK installed **before** UI phase
(`lastUpdateTime` 17:54). **Zero** app/plugin code changes during UI.
Emulator: `Navi_8c_4G_128G` as-is. Runner:
`LongTripMobileHomeBevensenDalsorenUiCampaignTest`. Host ADB GPS + **6**
synthetic DATEX Blocks with re-pin. **No version bump / tag / ship.
Elsa→Sjuvasslia not started (Bevensen fail gate).**

UI wipe of corridor (+ leftover) regions, then Plan with long trip ON
(`downloads_mode=long_trip_auto_corridor_only`). No manual region downloads.

| Metric | Result | EXPECTED | OK |
|---|---|---|---|
| Distance | **0 km** (leg1 only; full plan failed) | 1461.3–1648.6 km | no |
| Driving time | **0** | 17–~22 h | no |
| Instructions | **0** | 55–100 | no |
| Ferries used | leg1 `route_uses_ferry=false` (`graph_ferry_edges=26`) | natural if competitive | n/a |
| Nearby attractions | n/a | several | n/a |
| Wild camping sites | n/a | reported | n/a |
| Rest places | leg1 only: Rest stop 53.733966, 10.736334 | name+coords | partial |
| km/day | n/a | soft 6.0 h | n/a |
| Tunnels used | n/a | reported | n/a |
| Fuel stops (report-only) | n/a (0 km full plan) | 100 km margin / 70 L | n/a |
| DATEX host inject | **yes** (6 sits DE/DK/SE/NO; re-pin) | 3–8 synthetic | host ok |
| DATEX in plan | **no** (`datex_impacts=0` on chunk legs) | applied | no |
| Plan | **HARD FAIL** — `chunk_leg2` `bbox_exhausted` / `disconnected` (Fehmarn water) | found | no |

### Auto-download order (SD `long-trip-packs`) — Geofabrik harden OK

Plan click ~17:57:24. All 9 corridor regions reached **Installed/Indexed** with
**real** Geofabrik PBFs (no Failed extract gate). Approx sizes:

| # | Region | Final | Geofabrik PBF on SD |
|---|---|---|---|
| 1 | Niedersachsen | **Indexed** | **506.6 MB** |
| 2 | Schleswig-Holstein | **Indexed** | **158.5 MB** (was Failed on prior 502 run) |
| 3 | Denmark | **Indexed** | **495.1 MB** |
| 4–6 | Skåne / Halland / Västra Götaland | **Indexed** | shared **sweden 839.4 MB** |
| 7 | Ostlandet | **Indexed** | **455.3 MB** |
| 8 | Sorlandet | **Indexed** | **77.1 MB** |
| 9 | Vestlandet | **Installed** at plan start (Indexed shortly after) | **257.4 MB** |

`place-index-ready.json` at fail:
`[denmark, niedersachsen, schleswig-holstein, ostlandet, sorlandet, halland, skane, vastra_gotaland]`
(vestlandet not yet listed).

Corridor ready ~18:45:21 (~48 min downloads). Harden validation: prior SH/DK/SE
502 Failed path did **not** recur.

### Post-corridor Sweden PBF slowdown

After `corridor_ready`, wall clock can still sit for **~17+ minutes** with
`distance_km=0` / no polyline while densify builds ferry overlays for Swedish
län stems. Server packs for Halland / Västra Götaland / Skåne provide `.rkyv`
graphs only; Geofabrik has **no leaf extracts** for those regions — only the
shared **`sweden-latest.osm.pbf` (~840 MB)**. Place-index and ferry overlay
both resolve to that one country file. Each `ferry_overlay` for a län stem
therefore scans the full country PBF (typically **~3–4 min per stem** on the
`Navi_8c_4G_128G` AVD); TripAabb / pad retries repeat the scan (Halland and
Västra Götaland alternating). That cost dominates post-corridor wall time —
**not** A* search. Same class of issue for any country with subregion packs
but only a single country PBF. A request to the team at
[Geofabrik](https://www.geofabrik.de/) for Sweden extracts split by län
(and the same for Finland) should remove this shared-country ferry-overlay /
place-index cost; ideally all countries would publish matching subregion
extracts so leaf packs pair with leaf PBFs. Product note: README Known issues
(long-distance / Sweden-style single-country PBF).

Live continue/reuse evidence (`/tmp/bevensen-ui-host/`, 2026-10-01 evening):
`corridor_ready` ~22:14; six full `sweden-latest` ferry_overlay walks for
`halland-latest` / `vastra_gotaland-latest` from ~22:17–22:43 (~3.3–3.9 min
each, ~26 min of Sweden PBF I/O). Log lines:
`ferry_overlay geofabrik stem=… path=…/sweden-latest.osm.pbf bytes=839447705`.

### Root cause (routing, not download)

Long-trip chunked plan (`hops=20`, `chunk_deg=1.15`):

- **leg1 PASS**: Bevensen → 54.21000,11.02500; 167.2 km / 154.6 min;
  `graph_ferry_edges=26`; `route_uses_ferry=false`.
- **leg2 FAIL**: 54.21000,11.02500 → 55.17500,11.70000 (Fehmarn/Baltic chord).
  CorridorBand then TripAabb pads 0.35→1.4 all `disconnected` /
  `bbox_exhausted`. No ferry hop taken across the water gap.

UI note: `eco_routing_switch` clicked but remained OFF (`use_eco=false` on
chunk legs). Secondary to Fehmarn disconnect.

### RAM

Before ≈ **181 MiB** PSS; post-plan ≈ **695 MiB**; final ≈ **526 MiB**.

### DATEX

Six synthetic Blocks injected + re-pinned. Chunk-leg reports show
`datex_impacts=0` (not applied / not on chunk path).

### `graph_format_version`

Corridor regions **9** (incl. `vastra_gotaland`).

### Process notes / ask-before-fix

- Geofabrik extract harden: **validated** on this campaign (no Failed regions).
- Remaining blocker: Fehmarn/Baltic water connectivity for chunk_leg2
  (`disconnected` despite `graph_ferry_edges=26` on leg1 and avoid_ferries off).
- **No Elsa campaign** (Bevensen must succeed first).
- **No ship**. Hardening left **uncommitted** (ask before commit/push).

Evidence: `/tmp/bevensen-ui-host/` (`report_final_pull.json`, host.log,
logcat, packs, screenshots).

---

## Earlier same-day evening — 2026-10-01 (FAIL — Geofabrik 502 extract)

Branch tip: **`right-to-roam`** `@56314990` (stub-PBF / Geofabrik `-latest`
resolve + refuse soft-PASS stubs + Tools SD delete). Native `libnavi.so`
rebuilt and APK installed from tip **before** UI phase; **zero** app/plugin
code changes during the campaign. Emulator: `Navi_8c_4G_128G` as-is. Runner:
`LongTripMobileHomeBevensenDalsorenUiCampaignTest`. Host ADB GPS + **6**
synthetic DATEX Blocks with re-pin. **No version bump / tag / APK refresh.
Elsa→Sjuvasslia not started (Bevensen fail gate).**

UI wipe of corridor (+ leftover) regions, then Plan with long trip ON
(`downloads_mode=long_trip_auto_corridor_only`). No manual region downloads.

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
| Tunnels used | n/a | reported | n/a |
| Fuel stops (report-only) | n/a | 100 km margin / 70 L full tank | n/a |
| DATEX host inject | **yes** (6 sits DE/DK/SE/NO; re-pin) | 3–8 synthetic | host ok |
| DATEX in plan | **no** (no plan) | applied | n/a |
| Plan | **HARD FAIL** — SH/DK/SE Failed after Geofabrik 502 | found | no |

### Auto-download order + process/index times (SD `long-trip-packs`)

Local-first corridor after Plan click (~17:13:36):

| # | Region | Final | Pack→Installed / Failed | Indexed | Geofabrik PBF on SD |
|---|---|---|---|---|---|
| 1 | Niedersachsen | **Indexed** | Installed 17:18:58 (~5.4 min) | 17:25:32 | **506.6 MB** real |
| 2 | Schleswig-Holstein | **Failed** | Failed 17:20:45 | — | no leaf PBF (graphs/manifest present) |
| 3 | Denmark | **Failed** | Failed 17:24:10 | — | no leaf PBF (graphs/manifest present) |
| 4 | Skåne | **Failed** | Failed 17:26:42 | — | **16 KiB** stub left |
| 5 | Halland | **Failed** | Failed 17:27:22 | — | **16 KiB** stub left |
| 6 | Västra Götaland | **Failed** | Failed 17:28:42 | — | **16 KiB** stub left |
| 7 | Ostlandet | **Indexed** | Installed 17:32:17 | 17:37:34 | **455.3 MB** real |
| 8 | Sorlandet | **Indexed** | Installed 17:32:51 | 17:34:52 | **77.1 MB** real |
| 9 | Vestlandet | **Indexed** | Installed 17:34:55 | 17:39:20 | **257.4 MB** real |

`place-index-ready.json`:
`[niedersachsen, ostlandet, sorlandet, vestlandet]`.

### Root cause

Pack-server graph installs succeeded for SH/DK/SE. Follow-up Geofabrik place-index
extract (`provisionRegionData`) hit **`502 Bad Gateway`** on:

- `…/schleswig-holstein-latest.osm.pbf/`
- `…/denmark-latest.osm.pbf/`
- `…/sweden-latest.osm.pbf/` (shared by Skåne / Halland / Västra Götaland)

With tip `56314990`, a non-PASS extract marks the region **`failed`** and does
**not** emit `Installed`, even though routing packs are already on disk. Failed
regions permanently block `corridorReadyForPlanning()` (needs every corridor
region Installed or Indexed). No automatic retry; download queue emptied;
app ~idle (~4% CPU). Instrumentation would wait up to 20 h — stopped as hard-fail
~17:40 after sustained no-progress. App left running; host/instrumentation stopped.

Host note during stall: dated Geofabrik URL
`sweden-260930.osm.pbf` returned **200** (~839 MB) while `-latest` / region HTML
flapped 502/404 — resolve-via-HTML still insufficient under Geofabrik outages,
and Failed regions are not re-queued.

### RAM

Before ≈ **175 MiB** PSS; final sample ≈ **481 MiB** PSS.

### DATEX (host only; not applied to a plan)

Six synthetic Blocks injected + re-pinned: syn-de-a7, syn-dk-e45, syn-se-e6,
syn-se-gbg, syn-no-e6, syn-no-otta.

### `graph_format_version` (from `https://navigate-me.duckdns.org/current.json`)

All corridor regions **9** (niedersachsen, schleswig-holstein, denmark, skane,
halland, vastra_gotaland, ostlandet, sorlandet, vestlandet).

### Process notes / ask-before-fix

- Stub fix on tip is partially validated: DE + NO got real Geofabrik PBFs and
  Indexed; SE/DK/SH failed open on Geofabrik 502 + hard-fail extract gate.
- **Local fix (awaiting commit approval):** Geofabrik extract path now retries
  502/5xx with longer backoff (`GEOFABRIK_EXTRACT_RETRIES=6`), and
  `-latest` → dated resolve falls through region-HTML (retried) →
  `-latest` redirect `Location` → recent `{leaf}-YYMMDD` day probe. Still
  refuses soft-PASS stubs; prefer real extract over Installed-without-PBF.
- **No Elsa campaign** per addendum (Bevensen must succeed first).
- **No ship** (no CI gate / version / tag / APK refresh).

Evidence: `/tmp/bevensen-ui-host/` (`report.json`, `stall_snapshot.json`,
`fail_extract_log.txt`, host.log, logcat, meminfo, screenshots including
`hard_fail_stall.png`).

---

## Earlier same-day afternoon — 2026-10-01 (FAIL — stub PBF stall)

Branch tip at start: **`right-to-roam`** `@eb3e6037` (ferry overlay stub-skip /
pier approaches). Every corridor `*-latest.osm.pbf` landed as a **16 KiB**
pack-server stub; SE regions stuck Downloading >80 min idle. Prompted stub-PBF
fix `56314990`. No ship.

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
- **Sweden-style single-country PBF ferry_overlay cost** (documented, not a
  routing bug): see [Post-corridor Sweden PBF slowdown](#post-corridor-sweden-pbf-slowdown).
  Upstream fix path: ask [Geofabrik](https://www.geofabrik.de/) for län-level
  Sweden (and Finland) extracts so leaf packs match leaf PBFs.
