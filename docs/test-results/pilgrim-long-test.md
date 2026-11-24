# Pilgrim long test — Mariakirken → Nidaros

Date: **2026-11-24** (Europe/Oslo). Emulator **`Navi_8c_4G`** (`emulator-5554`).
Branch **`tests`** @ `c490555c2` (+ applied CoMaps quilt `0009-navi-utils-toast-main-thread` for off-main Toast safety; no new feature code).

## Verdict

**CONDITIONAL PASS** — Hiking + 3 vias plan OK; **rest_place_count=29** named with coordinates; GPS replay OK. **Tunnels gate FAIL:** app reported **`route_tunnel_count=20`** despite hiking (brief requires 0). Prior pilgrim ART reported tunnels=0 because MWM plan tokens omitted tunnel fields (same class of gap as Bevensen §6); this re-run emits real tunnel fingerprints.

| Gate | Result |
|---|---|
| Regions before Plan | Ostlandet then Trøndelag MWM on SD; Indexed before Plan |
| Via (3/4) | **OK** — Oslo / Hamar / Åmot |
| Plan + Hiking + 3 vias | **OK** (`plan_kick` `profile=HIKING vias=3`) |
| Distance / ETA | **514.632 km**, **6108.1 min** (~13 soft hike-days) |
| Tunnels | **20** — FAIL (must be 0 for hiking) |
| Ferries | **1** (avoid ferries off / allowed) |
| Rest places | **29** named + coords (`mwm_break_pois=true`) |
| Attractions | **22** (14 attraction + 8 viewpoint) |
| Force-stop | Not used by harness (instrument lifecycle only) |

## ART / driver

| Item | Value |
|---|---|
| ART | `/tmp/pilgrim-long-campaign/attempt_20261124_114827` |
| Driver | `PilgrimLongTripUiCampaignTest#pilgrim_long_ui_plan_campaign` + host GPS (`host.py`) |
| Route | Mariakirken → Pilegrimssenter Oslo → Pilegrimssenter Hamar → Åmot kirke → Nidaros (63.42661, 10.39992) |
| Profile / settings | Hiking; Avoid ferries off; wild camping on; Long trip on; DATEX off; nearby attractions on; Follow pilgrim routes on; open + networked cabins on |
| Departure | **2026-06-01T08:00:00** local (option a) |

## Clock (option a)

| Phase | Guest clock | Evidence |
|---|---|---|
| MWM wipe + Østlandet + Trøndelag downloads | TLS-valid ~`2026-11-24T12:00` (`auto_time=0`) | `ART/logs/clock_timeline.txt` |
| Plan / GPS replay | **`2026-06-01T08:00:00`** | `JUNE_FOR_REPLAY` / June hold |

First instrument leg aborted when a stale June pin raced TLS during Ostlandet CDN fetch (`download failed: … timeout`). Keepers were reset to TLS-only; soft relaunch (`instrumentation_relaunch.txt`) completed under stable Nov 24, then June for GPS.

## Regions (order, paths, timings)

1. **UI wipe** (`force_wipe=true`): Norway leaves deleted; `leaves_left=0`; World.mwm kept.
2. **UI download Østlandet first** (`europe_norway_ostlandet`): Central, Hedmark, Oppland, Southern — `ok=true`, version `261115`, SD placement.
3. **UI download Trøndelag before Plan** (`europe_norway_trondelag`): North + South Trondelag; corridor then **6 leaves** Indexed.

Placement: `/storage/0000-0000/Android/data/no.navi.app/files/mwm` (SD).

Long-trip status at plan: `Ostlandet (1 of 2)=Indexed · Trondelag (2 of 2)=Indexed`.

## Via + Plan

```
From: Mariakirken, Oslo, Norway | To: 63.42661, 10.39992
Via: Pilegrimssenter Oslo … → Pilegrimssenter Hamar … → 61.13390, 11.37289
```

```
plan_kick=1 longTrip=true profile=HIKING … vias=3
planning_start profile=hiking … legs=4
planning_done … distance_km=514.632
mwm_break_pois marks=45 candidates=215 stops=29
```

Compose `btn_plan_route` with Via N/4 placeholders (Oslo / Hamar / Åmot coord).

## Route metrics

| Metric | Value |
|---|---|
| Total length | **514.632 km** |
| ETA | **6108.1 min** (~101.8 h) |
| Ascent / descent | 8357.5 m / 8355.5 m |
| Build | 22.7 s; `comaps_route_code=OK`; planner=mwm |
| Maneuvers | **598** — left 299, right 293, destination 4, straight 2 |
| Estimated stops / days | **13** soft hike-days @ ~8 h (days 1–12 ≈ **40.44 km**, day 13 ≈ **29.33 km**); rest-stop count **29** |
| Ferries | **1** (`unnamed@55.29`; avoid ferries off) |
| Tunnels (`route_tunnel_count`) | **20** — FAIL vs hiking must-be-0 |
| Tunnel fingerprints | unnamed + Grønvollvegen / Industrivegen / Korperudveien (see `route-result.json`) |
| Plan setting note | `plan_settings … avoid_tunnels=false` (UI toggle was ON then absent for hiking sheet) |
| Wild camping sites | **12** (status OK; no-03:5, no-32:7) |
| Rest places | **29** (see table below) |
| Attractions | **22** — tourism-attraction 14, tourism-viewpoint 8 |
| Water sources | Not enumerated in plan report tokens |
| GPS replay | **ok** — 514.1 km; towns Oslo_Mariakirken, Pilegrimssenter_Oslo, Hamar, Rena, Amot_kirke, Trondheim_Nidaros; DATEX=0 |

## Rest places (named + coordinates)

`rest_place_count=29`, `mwm_break_pois=true`, `mwm_break_candidates=215`.

| Name | Lat | Lon | Kind |
|---|---:|---:|---|
| Picnic Site | 59.95100 | 10.91835 | picnic |
| Gapahuk Tårnbyveien North | 59.97248 | 11.00038 | shelter |
| Picnic Site | 60.02361 | 11.05309 | picnic |
| Picnic Table | 60.15555 | 11.17030 | picnic |
| Picnic Site | 60.23765 | 11.17107 | picnic |
| Picnic Table | 60.30122 | 11.18064 | picnic |
| Picnic Table | 60.41127 | 11.24228 | picnic |
| Picnic Table | 60.50780 | 11.24075 | picnic |
| Picnic Table | 60.53739 | 11.24386 | picnic |
| Picnic Table | 60.70955 | 11.10458 | picnic |
| Picnic Table | 60.79386 | 11.06917 | picnic |
| Picnic Table | 60.84862 | 11.37227 | picnic |
| Picnic Table | 60.94480 | 11.33970 | picnic |
| Veltkoia | 60.99517 | 11.31832 | hut |
| Picnic Table | 61.13249 | 11.37217 | picnic |
| Picnic Table | 61.16438 | 11.46178 | picnic |
| Picnic Site | 61.21338 | 11.33033 | picnic |
| Picnic Table | 61.60184 | 11.23312 | picnic |
| Picnic Table | 61.69559 | 11.21235 | picnic |
| Smedberget Pilegrimstun | 61.90040 | 11.06656 | hut |
| Svarttjønnbua | 61.92862 | 10.97344 | hut |
| Knausen | 62.22388 | 10.95581 | hut |
| Gammelbrua rastplass | 62.41540 | 11.01944 | picnic |
| Spellmovollen | 62.62685 | 10.84827 | hut |
| Picnic Site | 63.02829 | 10.26631 | picnic |
| Picnic Table | 63.10902 | 10.23683 | picnic |
| Picnic Site | 63.22673 | 10.28085 | picnic |
| Picnic Table | 63.28714 | 10.27490 | picnic |
| Picnic Site | 63.39258 | 10.36230 | picnic |

Sample named huts / pilgrim stops: **Gapahuk Tårnbyveien North**, **Veltkoia**, **Smedberget Pilegrimstun**, **Svarttjønnbua**, **Knausen**, **Gammelbrua rastplass**, **Spellmovollen**.

## Crashes / memory / errors

- Instrument result: **OK (1 test)**. `PILGRIM_DONE`.
- Earlier attempt `attempt_20261124_113925` crashed in `MwmBreakPois` / OM `Utils.getStringIdByKey` Toast off-main (`Can't toast… Looper`). Fixed by applying existing quilt **`0009-navi-utils-toast-main-thread`** (posts Toast via `UiThread.run`); rebuild/install; relaunch succeeded.
- RAM PSS samples: min ≈ **235 MiB**, peak TOTAL ≈ **1043 MiB** (`monitor/meminfo_*.txt`, 108 samples).
- Map showed planned route (polyline 741689 chars; town screencaps under `ART/screenshots/towns/`).

## Ops constraints honored

- UI-driven settings / region delete / region download / plan; ADB only for GPS + date.
- No new app/plugin feature code; rebuild of committed sources + apply existing CoMaps patch 0009.
- No force-stop by harness; soft instrument relaunch after TLS recovery.
- Elsa / Raufoss reports left untouched.

## Artifact index

| Path | Role |
|---|---|
| `ART/logs/clock_timeline.txt` | TLS then June option (a) |
| `ART/logs/instrumentation_relaunch.txt` | Final OK instrument |
| `ART/report/route-result.json` | Plan metrics + tunnel/ferry/rest tokens |
| `ART/report/break-pois.json` | 29 named rest places |
| `ART/report/device_report.json` | Full UI campaign report |
| `ART/report/route-polyline.txt` | Planned geometry |
| `ART/report/gps_replay.json` / `gps_replay_summary.json` | Host GPS replay |
| `ART/screenshots/towns/*.png` | Town stills along replay |
| This report | `docs/test-results/pilgrim-long-test.md` |
