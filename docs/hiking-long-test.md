# Hiking long trip — Nashaugsætra → Sandhaug → Lothepus

Date: **2026-11-20**. Emulator **`Navi_8c_4G`** (`emulator-5554`, x86_64).
App `no.navi.app` **0.5.1-beta** (`versionCode` **36**). Not added to CI.
Branch: `cursor/hiking-long-trip-fixes-a494`.

Instrumentation driver
`HikingLongTripNashaugsLothepusUiCampaignTest#hiking_nashaugs_lothepus_ui_plan_campaign`
drives the Compose UI. Host `/tmp/hiking_long_host.py` feeds GPS over ADB and
monitors logcat / `dumpsys meminfo` / screenshots (no DATEX). Artifacts:
`/tmp/hiking-long-campaign/` and `docs/evidence/hiking-long-test/`.

---

## Unblock (2026-11-20 ~16:24)

Stuck redo was looping Map storage deletes (`hit=false`, `left=11`) while the UI
already showed **No maps installed** and 11 orphan leaf MWMs remained under
`files/mwm/releases/261105/leaves`. Plan never started.

| Step | Action |
|---|---|
| 1 | Killed stuck `hiking_long_host.py` + `am instrument` HikingLongTrip (and competing redo) |
| 2 | Cleared orphan leaves via `run-as` (disk matched empty UI) |
| 3 | Harness: `deleteMwmViaStorageUi` aborts when a full pass has **0 hits** or UI is empty, then **filesystem wipe** |
| 4 | Reinstalled app + androidTest with hiking fixes (SD `MwmDataDir`, Hiking locks avoid-tunnels, soft breaks / wild / water) |
| 5 | Started **one** clean full campaign at **16:24:24** → `/tmp/hiking-long-campaign` |

---

## Verdict

**IN PROGRESS** — clean campaign started after unblock. Outcome section updated when
`HIKING_PLAN_OK` / `HIKING_DONE` / instrument exit lands.

| Check | Result |
|---|---|
| Wipe-loop unblock | **PASS** — orphans cleared; harness abort + file wipe shipped |
| Attractions | *pending* |
| Plan found | *pending* |
| GPS replay + no force-stop | *pending* |
| DNT cabin screenshots | *pending* |
| Wild camping / rest / water | *pending* (fixes in APK) |
| Avoid tunnels | *pending* (Hiking locks avoid-tunnels in native) |
| Downloads to SD | *pending* (`MwmDataDir` follows pack volume `uuid:0000-0000`) |
| Overall campaign | **IN PROGRESS** |

---

## Test setup

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G` / `emulator-5554` (8c / 4 GB) |
| Profile | **Hiking** |
| Avoid tunnels | **on** (forced / locked for Hiking) |
| Avoid ferries | **off** |
| Wild camping | **on** |
| Long trip | **on** |
| DATEX | **off** |
| Nearby attractions | **on** |
| Departure | `2026-06-01T08:00:00` local |
| From | **Nashaugsætra** — `60.8835, 9.9720` |
| Via | **Sandhaug (Hardangervidda)** — `60.1922, 7.5125` |
| To | **Lothepus camping** — `60.0651, 6.5526` |
| MWM data dir | Pack volume SD via `MwmDataDir` when writable |

---

## Campaign run (clean after unblock)

| Field | Value |
|---|---|
| Start | 2026-11-20T16:24:24+01:00 |
| Host pid | see `/tmp/hiking-long-campaign/logs/host.pid` |
| Instrument | `NaviAndroidTestRunner` / HikingLongTrip campaign method |
| Artifacts | `/tmp/hiking-long-campaign/` |

*(Regions, route metrics, soft-break / tunnel / SD results filled when the run completes.)*
