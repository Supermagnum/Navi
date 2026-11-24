# Pilgrim long test — Mariakirken → Nidaros

Date: **2026-11-24** (Europe/Oslo). Emulator **`Navi_8c_4G`** (`emulator-5554`).
Branch **`tests`**. **No production app/plugin code changes.** Bevensen files not modified.

## Verdict

**PASS** (Hiking + 3 vias plan accepted).

| Gate | Result |
|---|---|
| Wipe → world-only, then Østlandet first | **OK** (full UI wipe in first instrument leg; leaves=0 confirmed) |
| Corridor Indexed before Plan | **OK** — Ostlandet + Trondelag Indexed; 6 MWM leaves |
| Via (3/4) + Plan co-present | **OK** — `plan_kick … profile=HIKING … vias=3` |
| Hiking (not Car) | **OK** — `planning_start profile=hiking legs=4`; ETA **6108.1 min** |
| Tunnels | **0** (PASS) |
| Ferries | **0** |
| Attractions | **22** (14 attraction + 8 viewpoint) |
| Wild camping | **12** sites (`OK`) |
| GPS replay | **OK** — 514.1 km along polyline; town screenshots |
| Force-stop | Not used |
| Bevensen file edits | None |

## ART / driver

| Item | Value |
|---|---|
| ART | `/tmp/pilgrim-long-campaign/attempt_20261124_101638` |
| Driver | `PilgrimLongTripUiCampaignTest#pilgrim_long_ui_plan_campaign` + host GPS (`host.py`) |
| Route | Mariakirken → Pilegrimssenter Oslo → Pilegrimssenter Hamar → Åmot kirke → Nidaros (63.42661, 10.39992) |
| Profile / settings | Hiking; avoid ferries off; wild camping on; Long trip on; DATEX off; nearby attractions on; follow pilgrim routes on; open + networked cabins on |
| Departure | `2026-06-01T08:00:00` local |

## Clock (option a)

| Phase | Guest clock | Evidence |
|---|---|---|
| MWM wipe + Østlandet / Trøndelag downloads | **TLS-valid `2026-11-24T12:00+`** (`auto_time=0`) | `clock_timeline.txt` `TLS_OK` / `URGENT_TLS_REPIN` after killing orphan June keepers |
| Plan click | Still Nov 24 (June trigger lagged) | `plan_kick` log lines stamped `11-24` |
| GPS replay + close-out | **`2026-06-01T08:00:00`** re-pinned and held | `JUNE_REPIN_POST_PLAN` / `JUNE_OK` |

Orphan host shells from prior attempt were still forcing `toybox date 060108002026.00` during downloads; they were process-killed (no Bevensen file edits). Downloads continued only after TLS was stable.

## Regions (order / paths / timings)

1. **UI wipe** (first instrument leg, `force_wipe=true`): deleted Norway_* leaves; `leaves_left=0`; World.mwm + WorldCoasts kept.
2. **Østlandet first** (`europe_norway_ostlandet`): UI install succeeded under TLS — Central, Hedmark, Oppland, Southern.
3. **Trøndelag** (`europe_norway_trondelag`): accepted via long-trip / missing-maps UI during Plan wait — North + South Trondelag added → **6 leaves Indexed**.

Placement (product + listing): SD pack volume mirror  
`/storage/0000-0000/Android/data/no.navi.app/files/mwm` (and internal `files/mwm/releases/261115/leaves`).

| Label | Region | Result | Notes |
|---|---|---|---|
| first_ostlandet | europe_norway_ostlandet | ok | UI Choose-region install |
| pre_plan_trondelag | europe_norway_trondelag | ok (~279 ms status sample) | leaves=6, version 261115, SD |
| plan_requested_vestlandet | europe_norway_trondelag | ok | missing-maps accept |

Corridor status at Plan: `Ostlandet (1 of 2)=Indexed · Trondelag (2 of 2)=Indexed`.

## Via + Plan

```
Via (3/4) summary:
From: Mariakirken, Oslo, Norway
Via: Pilegrimssenter Oslo → Pilegrimssenter Hamar → 61.13390, 11.37289
To: 63.42661, 10.39992
```

NaviRouting:

```
plan_kick=1 longTrip=true profile=HIKING … vias=3
planning_start profile=hiking eco=true legs=4
  waypoints=Mariakirken|Pilegrimssenter Oslo|Pilegrimssenter Hamar|61.13390, 11.37289|63.42661, 10.39992
planning_done distance_km=514.632
```

UI sequence that worked: OD first → corridor ready → **add 3 vias** → arm Hiking → Compose `btn_plan_route` while vias still in summary (no adb `input tap`).

## Metrics

| Metric | Value |
|---|---|
| Distance | **514.632 km** |
| ETA | **6108.1 min** (~102 h / ~13 soft hike days) |
| Ascent / descent | 8357.5 m / 8355.5 m |
| Build | 22.8 s; `comaps_route_code=OK` |
| Ferries | **0** |
| Tunnels | **0** (FAIL threshold was >0) |
| Attractions | **22** — tourism-attraction 14, tourism-viewpoint 8 |
| Wild camping sites | **12** (no-03: 5, no-32: 7) |
| Rest places (named break POIs) | **0** (`break-pois.json` empty; `rest_place_count=0`) |
| Water sources | Not enumerated in plan report tokens (no water field in `enumerations`) |
| Estimated stops | **0** (campaign field) |
| Km/day | 13 days @ ~40.4 km (days 1–12), day 13 ~29.3 km; sum 514.632 km |
| Nav instruction types | right 293, left 299, destination 4, straight 2 (598 maneuvers) |

## GPS replay

| Item | Value |
|---|---|
| Polyline | 20037 pts / ~514.1 km |
| Elapsed | 341.5 s |
| Towns hit | Oslo_Mariakirken, Pilegrimssenter_Oslo, Hamar, Rena, Amot_kirke, Trondheim_Nidaros |
| DATEX inject | 0 (off) |
| Screenshots | `ART/screenshots/towns/*.png` |

Host missed live `PILGRIM_AWAIT_GPS_REPLAY` log window; one-shot `replay_gps` under June completed and wrote `gps_replay_done.flag`. Instrument then logged `PILGRIM_DONE`.

## Crashes / memory / errors

- Mid-campaign soft instrument relaunch hit a **native crash** in `liborganicmaps.so` (prior process); app was relaunched via new `am instrument` (no `am force-stop`). Final leg completed without FATAL in filtered excerpts.
- June-vs-TLS clock fighting produced intermittent `ERR_CERT_DATE_INVALID` risk until orphan June keepers were killed; Østlandet install still completed under TLS.
- Meminfo samples: `ART/monitor/meminfo_*.txt` (214 files).

## Artifact index

| Path | Role |
|---|---|
| `ART/logs/clock_timeline.txt` | TLS download → June GPS option (a) |
| `ART/logs/urgent_tls_fix.txt` | Urgent June-during-DL remediation |
| `ART/report/route-result.json` | PASS plan + enumerations (tunnels=0) |
| `ART/report/route-polyline.txt` | Planned geometry |
| `ART/report/final_device_report.json` | UI events, vias, km/day, instructions |
| `ART/report/gps_replay.json` | Replay summary |
| `ART/screenshots/towns/` | Town stills along route |
| This report | `docs/test-results/pilgrim-long-test.md` |

## Notes / residual

- Rest-place / water tallies are empty in the plan report despite wild-camping=12 and attractions=22 — report as observed, not as a tunnel/via failure.
- June should preferably be re-pinned **before** Plan; this run planned under TLS but still produced a valid Hiking multi-via route with June held for GPS.
