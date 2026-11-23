# Raufoss → Bergen busstasjon (Samsung MWM UI campaign)

Date: **2026-11-23** (Europe/Oslo). Device: Samsung **SM-P613** serial
**`R52TB0JQEDE`** (arm64-v8a). App `no.navi.app` **0.5.1-beta**
(`versionCode` **36**), debug APK sha256
`c9f5210fcec88e503bad0a9a04659c9ef02624a08c9a694354553d4461dc097a`
(includes async look-ahead seed + MainActivity re-tally fix). **Not** added to
CI. Instrumentation driver `CarRaufossBergenUiCampaignTest` drove the real UI;
ADB was used only for GPS mock (`cmd location` test provider) + synthetic DATEX
and read-only monitoring. Host: `/tmp/raufoss_bergen_samsung_host.py`.
Artifacts: `/tmp/raufoss-bergen-samsung/attempt_20261123_110059/`.

Download path: **Tools → Offline maps → Choose region**
(`btn_mwm_regions` / `btn_mwm_install_europe_norway_ostlandet`). Geofabrik
coverage dialogs were dismissed so planning stayed on the MWM path. Regions
were UI-deleted via Map storage before any download or plan. Østlandet first;
Vestlandet accepted when the app offered missing-maps download.

No bridges, land hops, ferries, or road segments were forced. Wild camping off.
Long trip off. DATEX on. Nearby attractions on. Car profile. Soft daily budget
6.0 h; soft break 1.5 h / 15 min; eco on; avoid toll/ferries off.

---

## Verdict

| Metric | Result | EXPECTED | OK |
|---|---|---|---|
| Distance | **428.532 km** | 0–500 km | yes |
| Driving time | **~6.5 h** (389.4 min) | 3–8 h | yes |
| Instructions | **28** maneuvers | 1–170 | yes |
| Plan | **found** (`ui_planned=true`, bands all ok) | found | yes |
| Tunnels | **3** (`route_tunnel_count=3`) | >= 1 on corridor | yes |
| Ferries | **0** | reported | yes |
| Rest places | **4** soft-break pins (1.5 h) | soft-break spacing | yes |
| Wild camping sites | **0** | plugin off | yes |
| Attractions | **7** (`tourism-attraction` 5, `tourism-artwork` 2) | several | yes |
| DATEX inject | **yes** — 5 synth sits + mid Block | 3–6 | yes |
| DATEX apply | **yes** — `datex_impacts=5` (`datex_block=1`, `datex_penalize=4`) | Block+penalize | yes |
| DATEX render | **yes** — `datex_render.ok=true`, `drape_marks=5`, `compose_marks=1` | markers/HUD | yes |
| Mid-sim | **yes** — `rerouted=true` / `auto_reroute_count=1`; mid snap on corridor | mid inject | yes |
| JUnit | **OK (1 test)** in 634.4 s (`NaviAndroidTestRunner`) | pass without closing app | yes |

Attractions proof (async seed + re-tally): host logcat
`mwm_attractions seeded=16` → `look-ahead seed done epoch=1` →
`NaviAttractions: lookahead tally attraction_count=7 types=2 seed_epoch=1`.
Patched enumerations in `files/long-trip-ui-report/route-result.json`.

---

## Test setup

| Item | Value |
|---|---|
| Device | Samsung SM-P613 / `R52TB0JQEDE` (arm64-v8a only) |
| Origin | Raufoss **60.7277483, 10.6109403**, elev **311 m** |
| Destination | Bergen busstasjon **60.388114, 5.333857** (coord search) |
| Profile | Car |
| Eco | on |
| Avoid toll / ferries / tunnels | off / off / off |
| Soft daily budget | **6.0 h** |
| Soft break | **1.5 h** / **15 min** |
| Wild camping | off |
| Long trip | off |
| DATEX overlay | on |
| Nearby attractions | on |
| Download UI | MWM Choose region |
| APK sha256 | `c9f5210fcec88e503bad0a9a04659c9ef02624a08c9a694354553d4461dc097a` |

---

## Regions (MWM Choose region)

Active install version **261115**, placement
`/data/user/0/no.navi.app/files/mwm`. Catalog mode **comaps-release**.

### Policy

**UI delete via Map storage before plan; redownload Østlandet first.**

### Download order and disk usage

| Order | Region / leaf | Result | Disk |
|---|---|---|---|
| 1 | `europe_norway_ostlandet` | ok in **93046 ms**; 6 leaves + 6 sidecars @ **261115** | pack under `releases/261115` |
| 1a–1f | Norway_Central / Hedmark / Hordaland / Oppland / Southern / Western | leaves | ~160M / 98M / 86M / 96M / 238M / 132M |
| 2 | `europe_norway_vestlandet` | `vestlandet_prompt_handled=true` | corridor leaves already present |
| — | `files/mwm` total | — | **~1.6G** |

---

## Results summary

### Plan

| Field | Value |
|---|---|
| Distance | **428.532 km** |
| ETA | **389.4 min** (~6 h 29 m) |
| Maneuvers | **28** |
| Soft breaks | 4 Rest stop pins |
| Attractions | **7** (5 attraction, 2 artwork) |
| Tunnels | **3** — `Vinjadalen@210.39\|Arnavegen@77.83\|Nygårdstangen viadukt@22.69` |

### Rest places (name + coordinates)

| # | Name | Lat | Lon | Along route |
|---|---|---|---|---|
| 1 | Rest stop | 60.980025 | 9.266455 | 99.0 km |
| 2 | Rest stop | 61.098425 | 7.991893 | 198.1 km |
| 3 | Rest stop | 60.807062 | 6.663911 | 297.1 km |
| 4 | Rest stop | 60.425958 | 5.597019 | 396.1 km |

### Attractions by type

| Type | Count |
|---|---|
| tourism-attraction | 5 |
| tourism-artwork | 2 |
| **Total** | **7** |

### Navigation instruction types

| Kind | Count |
|---|---|
| roundabout | 19 |
| left | 2 |
| slight_left | 2 |
| straight | 2 |
| right | 1 |
| exit_left | 1 |
| destination | 1 |

### Km / day (soft 6 h budget) and total

Total **428.532 km**. Soft daily cap 6.0 h proportional to 389.4 min ETA:

| Day | Drive hours | Km |
|---|---|---|
| 1 | 6.0 (cap) | ~396.2 |
| 2 | ~0.49 | ~32.4 |

### Fuel stops (report-only estimate)

Report-only (planner unimplemented): **0** stops at 500–600 mi range with
100 km margin, starting full tank.

### DATEX

| Question | Answer |
|---|---|
| Injected? | yes — 5 synth sits + mid `syn-mid-block-sim` |
| Applied? | yes — impacts=5; block=1; penalize=4 |
| Rendered? | yes — drape_marks=5 |
| Mid-sim? | yes — mid snap ~60.798,10.426; rerouted=true |

### RAM

| Phase | MiB |
|---|---|
| Before | ~352.6 |
| Post plan | ~927.3 |

---

## App fix verified

Async look-ahead seed (`mwm_enrich_lookahead=async`) previously left
`attraction_count=0` because MainActivity tallied once before ingest finished.
Fix: notify on seed completion and re-run the isotropic corridor tally
(`seed_epoch=1` → count=7). Campaign wait now waits for
`lookAheadSeedCompletedEpoch` before recording enumerations.

---

## Machine-readable report

Device `files/raufoss-bergen-ui/report.json` and
`files/long-trip-ui-report/route-result.json` (enumerations with attractions=7).
Host copy under `/tmp/raufoss-bergen-samsung/attempt_20261123_110059/`.
Instrumentation: `OK (1 test)`, 634.4 s. **Not** wired into CI.
