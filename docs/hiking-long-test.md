# Hiking long trip — Nashaugsætra → Sandhaug → Lothepus

Date: **2026-11-20**. Emulator **`Navi_8c_4G`** (`emulator-5554`, x86_64).
App `no.navi.app` **0.5.1-beta** (`versionCode` **36**). Not added to CI.

Instrumentation driver
`HikingLongTripNashaugsLothepusUiCampaignTest#hiking_nashaugs_lothepus_ui_plan_campaign`
drives the Compose UI. Host `/tmp/hiking_long_host.py` feeds GPS over ADB and
monitors logcat / `dumpsys meminfo` / screenshots (no DATEX). Artifacts:
`/tmp/hiking-long-campaign/` and `docs/evidence/hiking-long-test/`.

Download path: **MWM Choose region** first (Østlandet), then Long-trip prompts
for Vestlandet. JUnit **`OK (1 test)`** in **586.9 s**.

---

## Verdict

**FAIL overall** (instrument JUnit OK; product expectations not fully met).

| Check | Result |
|---|---|
| Attractions (≥ several, typed) | **PASS** — **5** (`tourism-attraction=2`, `tourism-viewpoint=1`, `tourism-museum=1`, `tourism-artwork=1`) |
| Plan found | **PASS** — `ui_planned=true`, **430.381 km**, **433** maneuvers, via Hardangervidda coords |
| GPS replay + no force-stop | **PASS** — host GPS only; `HIKING_DONE`; JUnit **OK (1 test)** / 586.9 s |
| DNT cabin screenshots | **FAIL quality** — 3 files written but **identical MD5** (screencap did not vary by place) |
| Wild camping / rest / water | **FAIL** — all **0** with plugins on |
| Avoid tunnels | **FAIL** — **13** tunnel segments with Avoid tunnels **on** |
| Downloads to SD | **FAIL** — MWM leaves on **internal** only (`sd_mwm_count=0`) |
| Overall campaign | **FAIL** — plan/GPS/attractions OK; SD / tunnels / rest / wild / water / DNT-shot uniqueness not met |

---

## Test setup

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G` / `emulator-5554` (8c / 4 GB / as-is) |
| Profile | **Hiking** |
| Avoid tunnels | **on** |
| Avoid ferries | **off** (ferries allowed) |
| Wild camping | **on** |
| Long trip | **on** |
| DATEX | **off** (`datex_mwm=plugin_off`) |
| Nearby attractions | **on** |
| Departure | `2026-06-01T08:00:00` local (reported; GPS replay timed from plan) |
| From | **Nashaugsætra, Oppland** — UI name search → **61.15114, 9.84336** |
| Via | **Sandhaug (Hardangervidda)** — name hit “Sandhaug AS, Gjøvik” rejected; **coord forced** → **60.19220, 7.51250** |
| To | **Lothepus camping, Odda** — UI name search → **60.06507, 6.55261** |

---

## Regions (order, timing, placement)

UI Map storage wipe first: **`leaves_after_ui_delete_count=0`**,
`mwm_leaf_files_after_delete_count=0` (file listing confirmed clean before
redownload). World overlays remain at MWM root by design.

| Order | Region | Elapsed | Result | Placement |
|---|---|---|---|---|
| 1 (manual UI) | `europe_norway_ostlandet` | **195565 ms** (~3.3 min) download/process/index to Ready | ok, release **261105**, 9 leaves + 9 sidecars | **`/data/user/0/no.navi.app/files/mwm` (internal)** |
| 2 (app prompt during Plan) | `europe_norway_vestlandet` | accepted via `btn_mwm_missing_download`; corridor status → Ready | ok (timing folded into plan wait; host `elapsed_ms=0` passive) | **internal** (same tree) |

Final installed leaves (11): Denmark_North Denmark Region, Norway_Central,
Norway_Hedmark, Norway_Hordaland, Norway_Oppland, Norway_Rogaland,
Norway_South Trondelag, Norway_Southern, Norway_Western, Sweden_Bergslagen,
Sweden_Vastra Gotaland. Disk: **~1.3 GB** under `files/mwm`.

**SD card:** long-trip pack volume pref was `uuid:0000-0000`, but this MWM run
wrote maps to **internal** only. No `.mwm` under
`/storage/0000-0000/Android/data/no.navi.app/`. (Expectation “every region on
SD” **not met** for MWM.)

---

## Route metrics

| Metric | Value |
|---|---|
| Distance | **430.381 km** |
| ETA | **5842.9 min** (~97.4 h) |
| Maneuvers | **433** |
| Polyline | **777754** chars |
| Ferries | **1** (`route_ferry_legs=1`, `unnamed@41.08`) |
| Tunnels | **13** despite Avoid tunnels **on** (`Kvernmolykkja`, `Veståsvegen`, `Bjørøyvegen`, `Vestaforvegen`, `Ustedalen`, `Lilletoppen`, `Tyssedalsvegen`, + unnamed) |
| Wild camping sites | **0** |
| Rest places | **0** (no named overnight/rest pins) |
| Estimated stops | **0** (from rest-place count) |
| Water sources | **0** |
| Attractions | **5** — see types above |
| DATEX | off — `datex_impacts=0` |

### Km per day (soft 8 h hiking budget)

Total **430.381 km** split from ETA:

| Day | Hours | Km |
|---|---|---|
| 1–12 | 8.0 each | ~35.36 each |
| 13 | ~1.38 | ~6.11 |

### Navigation instruction types

| Kind | Count |
|---|---|
| right | 216 |
| left | 211 |
| straight | 3 |
| destination | 3 |

---

## Rest places

None enumerated (`rest_place_count=0`, empty `break_pois`). GPS host still
screenshotted corridor waypoints / DNT seeds when the puck passed them.

---

## DNT cabin / shelter screenshots

Named after the place (rule 10). Files under
`docs/evidence/hiking-long-test/dnt_cabins/` and
`/tmp/hiking-long-campaign/dnt_cabins/`:

| Place | Coordinates (seed / pass) | File |
|---|---|---|
| Sandhaug Turisthytte | via **60.192, 7.512**; passed ~60.202, 7.521 | `Sandhaug_Turisthytte.png` |
| Lothepus camping | end ~60.065, 6.553; passed ~60.109, 6.555 | `Lothepus_camping.png` |
| Nashaugsætra | start seed (end-of-replay ensure) | `Nashaugsætra.png` |

All three DNT PNGs share MD5 `5f5bea93b5fa9fc30e9ea61141ad742a` (host `adb exec-out screencap`
likely captured a stale/blank frame). UI stage screenshots `01_launch` …
`12_after_gps_replay` are distinct and live under
`docs/evidence/hiking-long-test/`.

---

## Memory / stability

| Item | Value |
|---|---|
| PSS before | ~395 MiB |
| PSS post-plan | ~809 MiB |
| Peak PSS (host meminfo during download/plan) | **~1368 MiB** (`TOTAL_PSS_kb=1368210`) |
| GPS-replay peak (host summary) | ~841 MiB |
| Crashes / ANRs / OOM / LMK | **None** attributable to the app. Host logcat “alerts” were only `am instrument` RuntimeInit bootstrap lines. |
| Freezes | None observed; plan + GPS replay completed. |
| JUnit | **OK (1 test)** |

No FATAL EXCEPTION / ANR / lowmemorykiller lines for `no.navi.app` in the
campaign logcat follow.

---

## Restart cleanup (from prior pause)

Prior run paused at `2026-11-20T12:16:25+01:00` for preparing/Tools-cycle fix
(`stuck_install_progress_running=false`). On restart:

- Leftover **logcat** from paused host was killed (documented in
  `/tmp/hiking-long-campaign/report/restart-cleanup.txt` /
  `docs/evidence/hiking-long-test/logs/restart-cleanup.txt`).
- Host + instrument PIDs from the pause were already dead; pre-start
  `am force-stop` used **only before** the clean campaign (never mid-run).
- APKs reinstalled (`0.5.1-beta`, lastUpdate **12:40** for this successful run).
- Idempotent `openTools` in the androidTest harness used; one harness-only
  relaunch retry added when MainActivity was backgrounded after MWM install.

---

## Notes / blockers

1. **Via name search** first resolved to “Sandhaug AS, Gjøvik”; driver rejected
   that hit and forced Hardangervidda coordinates so the via is on Sandhaug.
2. **From** name search used **Nashaugsætra @ 61.151, 9.843** (not the Nordre
   Land fallback 60.884, 9.972) — lengthens the corridor (~430 km).
3. **MWM storage is internal**, not the SD long-trip volume — SD expectation
   unmet for this MWM-first path (`uuid:0000-0000` pack pref present; `sd_mwm_count=0`).
4. **Avoid tunnels on** but planner still reported **13** tunnel segments
   (pedestrian/MWM path).
5. **Wild camping / rest / water** tallies stayed at zero with plugins on —
   attractions alone carried the “nearby POI” bar.
6. Product / plugin code was **not** modified for this run; only the androidTest
   campaign driver + `/tmp/hiking_long_host.py` were used.
