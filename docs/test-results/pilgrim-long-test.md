# Pilgrim long test — Mariakirken → Nidaros

Date: **2026-11-24** (Europe/Oslo). Emulator **`Navi_8c_4G`** (`emulator-5554`).
Branch **`tests`**. **No production app/plugin code changes.** Bevensen files not modified.

## Verdict

**FAIL / HARD_STOP.** OD + 3 vias can be set and Plan can be tapped with vias still present, but **UI Plan does not produce a new multi-via / Hiking route** (no nonzero *new* km). Status stays **Plan route / ETA off** aside from a **stale OD-only** “Route planned · 514.8 km” banner.

## Root cause of timeout / 0 km

1. **`vias3=False` was a false negative.** With 3 vias, Plan sits below the fold. Scrolling to Plan moves the `From:…` summary off-screen, so polls that keyed only on `sm.count('→')` reported vias cleared. **`Clear all vias (3)` / Via 3 rows remained** for the full Plan wait (`p3d_*.xml`, 42 polls).
2. **Plan tap with OD+3 vias intact does not start a new CoMaps build.** After a confirmed Plan tap (`reveal_final.xml` → clickable parent at 540,719; vias still `Clear all vias (3)`):
   - No `NaviRouting` / `BuildRoute` / `comaps_route_code` / distance-calc lines in filtered logcat (`p3dlog_*.txt`, `p3elog_*.txt`).
   - UI stays **Plan route** + **ETA off**.
   - Sheet still shows **“Route planned · 514.8 km”** — same figure as the earlier **OD-only** `PLAN_MWM_ROUTE` artifact (not a 3-via length).
3. **Why ~525 / 514.8 km “replayed”:** that distance is the **leftover OD-only** plan (`ART/report/route-result.json`, Via:(none)), reused by GPS replay. It is **not** evidence that UI Plan with 3 vias succeeded.
4. **Hiking not armed on the Plan sheet** in the failing attempts (Profile chips show **Car**; Tools → Hiking / Long trip not reliably re-applied once the tall via sheet is open).
5. **Regions / packs are not the blocker:** Ostlandet + Trondelag stay **ready / Indexed**. Noise only: `poi_lookahead … Missing` at start coords; June guest clock → `ERR_CERT_DATE_INVALID` (expected under option a).

| Item | Result |
|---|---|
| Regions | Ostlandet + Trondelag **Indexed / ready** |
| Via (3/4) set | **OK** (proven repeatedly) |
| Plan + vias co-present | **OK** after scroll (`rev3.xml` / `reveal_final.xml`: Plan + `Clear all vias (3)`) |
| New nonzero km from UI Plan + 3 vias | **Not achieved** |
| OD-only ~514.8 / GPS ~525 km | **Invalid** for multi-via acceptance (stale) |
| Force-stop | Not used |
| Bevensen file edits | None |

## ART / driver

| Item | Value |
|---|---|
| ART | `/tmp/pilgrim-long-campaign/attempt_20261124_081229` |
| Driver | UI ops after instrument corridor wait; host GPS (`pilgrim_long_host.py`) |
| Intended route | From Mariakirken (59.90348,10.76219) → vias Pilegrimssenter Oslo / Hamar / Åmot → To Nidaros (63.42661,10.39992); Hiking; June 1 |
| Wipe | `skip_wipe` after Østlandet TLS install |

## Regions / corridor

- UI: **Ostlandet: ready · Trondelag: ready**.
- Long-trip status (OD-only artifact): `Ostlandet (1 of 2)=Indexed · Trondelag (2 of 2)=Indexed`.
- Trondelag pack required Wi-Fi (`requireUnmetered`); was `Paused` until AndroidWifi enabled.
- MWM install under option **(a)** TLS-valid guest clock, then June re-pin before Plan.

## Clock (option a)

| Phase | Guest clock | Evidence |
|---|---|---|
| MWM / pack downloads | ~`2026-11-24T12:00+` (`auto_time=0`) | `ART/logs/clock_timeline.txt` `TLS_OK` |
| Before Plan / UI ops | **`2026-06-01T08:00:00`** | `JUNE_REPIN` / June keep during Plan polls |
| HARD_STOP window | June 1 kept | Guest `date` during Plan waits |

Bevensen June keepers were killed as **processes only** (no Bevensen file edits).

## Multi-via UI — what works

Chip `Via (N/4)` supports 4 slots. Setting 3 vias via lat/lon Search works when:

1. Empty panel uses placeholder **`Add via: place, road, or lat, lon`**.
2. Later slots use **`Add via N/4`** (must focus that field).
3. Tap the **result row below Search** (y ≳ 1000), not the typed field echo.
4. Do not dismiss the route sheet between vias.

**Proof** (`via3a.xml`, rebuild logs):

```
Via (3/4)
Via 1: 59.90633, 10.76960
Via 2: 60.79277, 11.04290
Via 3: 61.13390, 11.37289
summary: From: 59.90348, 10.76219 | To: 63.42661, 10.39992 | Via: … → … → …
```

## Plan + 3 vias — evidence (HARD_STOP)

| Dump / log | Finding |
|---|---|
| `pr0.xml` | Via (3/4) + full summary; Plan below fold |
| `pr1.xml` / `rev3.xml` | Plan visible; vias still via `Clear all vias (3)` / Via 3 (summary may be off-screen) |
| `reveal_final.xml` + Plan tap 540,719 | Clickable parent View hit; vias still present |
| `p3d_0.xml` … `p3d_41.xml` | 42×10s polls: `Clear all vias (3)` stays; **ETA off**; banner **Route planned · 514.8 km** (stale OD) |
| `p3dlog_*.txt` | No BuildRoute / NaviRouting; `poi_lookahead … Missing`; June `ERR_CERT_DATE_INVALID` noise |
| `plan3d_result.json` | `ok=false`, `reason=timeout_no_nonzero_km` |

Earlier automation TIMEOUT (36 polls, `vias3=False`) matches the same sheet: Plan visible, summary off-dump, **no new route**.

## Earlier OD-only ~525 km (invalid for this test)

```
TEST_KIND=PLAN_MWM_ROUTE
PLANNER=mwm
PASS
distance_km=514.815
eta_minutes=435.7
comaps_route_code=OK
start=59.90348, 10.76219
end=63.42661, 10.39992
```

GPS replay ~525 km / towns screenshots belong to that OD-only polyline. ETA ~435.7 min is car-like — not pilgrim/hiking acceptance.

## Ops constraints honored

- UI-only diagnosis/fix attempts; no app/plugin source edits for this FAIL close-out.
- No force-stop for the HARD_STOP path.
- No Bevensen file edits.

## Artifact index

| Path | Role |
|---|---|
| `ART/logs/via3a.xml` / `multi_via_success.json` | 3-via set proof |
| `ART/logs/pr0.xml` / `pr1.xml` / `rev3.xml` / `reveal_final.xml` | Via vs Plan viewport |
| `ART/logs/p3d_*.xml` / `plan3d_result.json` | Plan+3 vias poll HARD_STOP |
| `ART/logs/p3dlog_*.txt` / `p3elog_*.txt` | Logcat around Plan (no BuildRoute) |
| `ART/logs/clock_timeline.txt` | TLS then June option (a) |
| `ART/report/route-result.json` | OD-only metrics (invalid acceptance) |
| `ART/report/gps_replay.json` | OD-only ~525 km replay |
| This report | `docs/test-results/pilgrim-long-test.md` |

## Next status

**HARD_STOP / FAIL.** Root cause: **UI Plan with OD+3 vias does not trigger a new route build**; polls show 0 *new* km while a **stale OD 514.8 km** banner remains. Do not accept OD-only metrics. Next work (outside this UI-only close-out): make Plan honor vias / Hiking (or expose why Plan is a no-op when vias are set), then re-run June Hiking Plan for a distance distinct from 514.8 km.
