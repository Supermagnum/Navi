# Pilgrim long test — Mariakirken → Nidaros

Date: **2026-11-24** (Europe/Oslo). Emulator **`Navi_8c_4G`** (`emulator-5554`).
Branch **`tests`**. **No production app/plugin code changes.** Bevensen files not modified.

## Verdict

**PASS** (Hiking + 3 vias). Plan produced a multi-via hiking route with **tunnels=0**, **ferries=0**, **attractions=22**, wild camping sites enumerated. Instrument `pilgrim_long_ui_plan_campaign` finished **OK (1 test)**.

| Gate | Result |
|---|---|
| Regions before Plan | Ostlandet then Trøndelag MWM leaves on SD; Indexed line before GPS |
| Via (3/4) | **OK** — Oslo / Hamar / Åmot |
| Plan + Hiking + 3 vias | **OK** (`plan_kick` `profile=HIKING vias=3`) |
| Distance / ETA | **514.632 km**, **6108.1 min** (~4.2 days continuous / 13 soft hike-days) |
| Tunnels | **0** (PASS) |
| Ferries | **0** |
| Attractions | **22** (14 attraction + 8 viewpoint) |
| Force-stop | Not used |
| Bevensen file edits | None |

## ART / driver

| Item | Value |
|---|---|
| ART | `/tmp/pilgrim-long-campaign/attempt_20261124_101638` |
| Driver | `PilgrimLongTripUiCampaignTest#pilgrim_long_ui_plan_campaign` + host GPS (`host.py`) |
| Route | Mariakirken → Pilegrimssenter Oslo → Pilegrimssenter Hamar → Åmot kirke → Nidaros (63.42661, 10.39992) |
| Profile / settings | Hiking; Avoid ferries off; wild camping on; Long trip on; DATEX off; nearby attractions on; Follow pilgrim routes on; open + networked cabins on |
| Departure | **2026-06-01T08:00:00** local (option a) |

## Clock (option a)

| Phase | Guest clock | Evidence |
|---|---|---|
| MWM wipe + Østlandet + Trøndelag downloads | TLS-valid ~`2026-11-24T12:00` (`auto_time=0`) | `ART/logs/clock_timeline.txt` `TLS_*` |
| Plan / GPS replay | **`2026-06-01T08:00:00`** | `JUNE_REPIN_POST_PLAN` / `JUNE_FOR_REPLAY` / June keep |

Bevensen files were not edited. Clock keepers were host-side date pins only.

## Regions (order, paths, timings)

Sequence (same ART attempt):

1. **UI wipe** (first instrument, `force_wipe=true`): deleted 9 Norway leaves; `leaf_mwm_files_left=0` / `leaves_left=0`. World.mwm kept.
2. **UI download Østlandet first** (`europe_norway_ostlandet`): leaves Central, Hedmark, Oppland, Southern — install reached `ok=true pct=100`.
3. Soft instrument relaunch (`skip_wipe=true`) after corridor-gate harness fix (no `am force-stop`).
4. **UI download Trøndelag before Plan** (`europe_norway_trondelag`): North + South Trøndelag; `region_timings.pre_plan_trondelag` **elapsed_ms=279**, `ok=true`, version `261115`.

Final leaves (SD placement `/storage/0000-0000/Android/data/no.navi.app/files/mwm`):

- Norway_Central, Norway_Hedmark, Norway_Oppland, Norway_Southern
- Norway_North Trondelag, Norway_South Trondelag

Long-trip status at plan: `Ostlandet (1 of 2)=Indexed · Trondelag (2 of 2)=Indexed`.

## Via + Plan

Waypoints summary (pre-Plan proof):

```
From: Mariakirken, Oslo, Norway | To: 63.42661, 10.39992 | Via: Pilegrimssenter Oslo … → Pilegrimssenter Hamar … → 61.13390, 11.37289
```

Logcat (acceptance):

```
plan_kick=1 longTrip=true profile=HIKING … vias=3
planning_start profile=hiking … legs=4 waypoints=Mariakirken…|Pilegrimssenter Oslo…|Pilegrimssenter Hamar…|61.13390, 11.37289|63.42661, 10.39992
planning_done … distance_km=514.632
```

Compose `btn_plan_route` click used (`plan_compose_click_primary`). Prior HARD_STOP (Car+vias / Hiking+0 vias / stale OD) avoided by: OD first → Trøndelag UI download → vias last → Plan; reject car-like ETA in harness.

## Route metrics

| Metric | Value |
|---|---|
| Total length | **514.632 km** |
| ETA | **6108.1 min** (~101.8 h) |
| Ascent / descent | 8357.5 m / 8355.5 m |
| Build | 22.8 s; `comaps_route_code=OK`; planner=mwm |
| Maneuvers | **598** — right 293, left 299, destination 4, straight 2 |
| Estimated stops / days | **13** soft hike-days @ ~8 h (days 1–12 ≈ **40.44 km**, day 13 ≈ **29.33 km**) |
| Ferries | **0** |
| Tunnels (`route_tunnel_count`) | **0** — PASS |
| Wild camping sites | **12** (status OK; no=12; no-03:5, no-32:7) |
| Rest places | **0** named (plan token empty) |
| Attractions | **22** — tourism-attraction 14, tourism-viewpoint 8 |
| Water sources | **Not enumerated** in plan report tokens (0 dedicated water fields) |
| Break POIs JSON | empty `[]` |
| GPS replay | **ok** — 514.1 km along polyline; towns Oslo_Mariakirken, Pilegrimssenter_Oslo, Hamar, Rena, Amot_kirke, Trondheim_Nidaros; DATEX=0 |

## Crashes / memory / errors

- Instrument result: **OK (1 test)**. No FATAL/ANR/OOM attributed to Navi during Plan/GPS.
- June TLS `ERR_CERT_DATE_INVALID` noise expected under option (a) after re-pin; downloads done under TLS-valid clock.
- RAM (PSS): before ≈ **363 MiB**, post-plan ≈ **740 MiB**, final ≈ **728 MiB**. Monitor peak TOTAL sample ≈ **1072873 kB**.
- Map showed planned route (polyline 741689 chars; town screencaps under `ART/screenshots/towns/`).

## Ops constraints honored

- UI-driven settings / region delete / region download / plan; ADB only for GPS + date.
- No app/plugin source edits for product code; androidTest harness packaging only.
- No force-stop; soft instrument host kill only for harness reinstall mid-attempt.
- No Bevensen doc/ART edits. Report only committed here.

## Artifact index

| Path | Role |
|---|---|
| `ART/logs/clock_timeline.txt` | TLS then June option (a) |
| `ART/logs/instrumentation_corridor.txt` | Final OK instrument |
| `ART/report/route-result.json` | Plan metrics + tunnel/ferry/attraction tokens |
| `ART/report/device_report.json` | Full UI campaign report |
| `ART/report/route-polyline.txt` | Planned geometry |
| `ART/report/gps_replay.json` | Host GPS replay summary |
| `ART/screenshots/towns/*.png` | Town stills along replay |
| This report | `docs/test-results/pilgrim-long-test.md` |
