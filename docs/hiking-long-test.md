# Hiking long trip — Nashaugsætra → Fondsbu → Hjelle motell

Date: **2026-11-21** (Europe/Oslo). Emulator **`Navi_8c_4G`** (`emulator-5554`).
App `no.navi.app` **0.5.1-beta** from **`origin/dev`** tip **`fbe5558ca`**
(androidTest OD only adapted for Fondsbu/Hjelle; **no main/plugin changes**).
Not added to CI.

Driver: `HikingLongTripNashaugsLothepusUiCampaignTest#hiking_nashaugs_lothepus_ui_plan_campaign`
(Compose UI). Host: `/tmp/hiking_long_host.py` (ADB GPS + read-only monitoring).
Artifacts: `/tmp/hiking-fondsbu-hjelle-campaign/` and
`docs/evidence/hiking-long-test/`.

Instrument: **OK (1 test)** in ~314 s. Runner:
`no.navi.app.test/no.navi.app.NaviAndroidTestRunner`.

---

## Verdict

| Check | Result |
|---|---|
| UI wipe → empty leaves | **PASS** (0 leaves; World.mwm kept) |
| Østlandet first on SD | **PASS** (~77.2 s; release **261115**; 6 leaves) |
| Vestlandet Ready before plan | **PASS** — corridor Ready in **~11.1 s**; order local then along-route |
| Leaves on SD after plan | **PASS** — 7 leaves + World.mwm |
| Plan found | **PASS** — **244.9 km**, route on map |
| Ferries | **1** (`unnamed@29.22`) — Avoid ferries **OFF** (allowed by settings) |
| Tunnels | **PASS — 0** (`route_tunnel_count=0`, empty fp) |
| Wild camping | **PASS — 12** sites (`camping_plugin` ON) |
| Nearby attractions (cone) | **Noted — 0** — **no attractions along this route** (not a product FAIL) |
| Water sources | **0** |
| GPS replay + DNT screenshots | **PASS** — Fondsbu, Hjelle_motell, Nashaugsætra |
| Crashes / ANR / OOM / LMK | **PASS** — no app FATAL/ANR/OOM; 1 unrelated system `safetyhub` died |
| Overall | **PASS** (attractions absent on corridor; documented below) |

---

## Nearby attractions — none on this corridor

Nearby attractions was **ON**. Seed and cone both returned empty; this is **route data**, not a harness/tooling failure:

| Evidence | Value |
|---|---|
| Settings | `nearby_attractions=true` |
| Seed | `NaviPlan: mwm_attractions seed empty query=Severdigheter samples=48 max_dist_m=850.0` |
| Cone tally | `NaviAttractions: lookahead tally attraction_count=0 types=0 samples=48` |
| Lookahead JSON | `poi_lookahead_json` → `hits=[]`, `cone_m=850`, `half_width_deg=30` |
| By-type | `attraction_by_type={}` |
| Contrast | Wild camping still found **12** sites on the same plan (search path healthy) |

**Conclusion:** no Severdigheter / attraction POIs exist along the Nashaugsætra → Fondsbu → Hjelle corridor within the 850 m cone. Criteria that expected “several” attractions do not apply here; noted as **no attractions on route**, not a missing-POI product FAIL.

---

## Test setup

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G` / `emulator-5554` (8c / ~4 GB) — unchanged |
| Profile | **Hiking** |
| Avoid tunnels | **ON** |
| Avoid ferries | **OFF** |
| Wild camping | **ON** |
| Nearby attractions | **ON** |
| Long trip | **ON** |
| DATEX | **OFF** |
| From | **Nashaugsætra** — `60.8835, 9.9720` (search hit Oppland) |
| Via | **Fondsbu**, Tyinkrysset / Eidsbugarden — `61.37519, 8.29740` |
| To | **Hjelle motell** — `61.91460, 7.11161` |
| Departure | `2026-06-01T08:00:00` local |

---

## Regions downloaded (order, timing, paths)

All MWM leaves under SD:

`/storage/0000-0000/Android/data/no.navi.app/files/mwm/`  
(`releases/261115/leaves/` + mirrored). World.mwm at MWM root.

| Step | Region | Timing | Result |
|---|---|---|---|
| UI delete all except World | (prior leaves) | — | **0** leaves left |
| First download (UI) | **Østlandet** `europe_norway_ostlandet` | **77 198 ms** | OK; version 261115; 6 leaves |
| Corridor wait | Vestlandet downloading → Ready | **11 065 ms** to Ready-before-plan | Østlandet Ready → Vestlandet Ready |
| Plan missing-maps prompt | Vestlandet accepted | elapsed_ms=0 (already in flight / Ready) | Prompt accepted |
| After plan | — | — | **7** leaves on SD |

Final leaf set:

- Norway_Central, Norway_Hedmark, Norway_Hordaland, Norway_Oppland,
  Norway_Rogaland, Norway_Southern, Norway_Western

Expected order observed: **local (Østlandet) first**, then **next along route (Vestlandet)**.

---

## Route metrics

| Metric | Value |
|---|---|
| Total length | **244.9 km** |
| ETA | **3476.4 min** (~57.9 h) |
| Maneuvers | **107** |
| Soft-break interval | **11.3 km** / **2.67 h** (`chunked_soft_breaks`) |
| Estimated stops / rest places | **21** |
| Ferries | **1** — `unnamed@29.22` |
| Tunnels (path annotate) | **0** |
| Soft-avoid | Not required (already 0 tunnels; `tunnel_avoid=true`; enrich `route_tunnel_count=0`) |
| Plan build | `build_s=2.78`; `planning_done` **duration_ms=30053** (~30 s) |
| Wild camping sites | **12** (`OK`; `no-34` ×12) |
| Attractions (cone @ lookahead) | **0** — none along corridor (see section above) |
| Water sources | **0** |

### Rest places (name + coords)

All 21 soft rests labeled **Hiking rest**:

| # | lat | lon |
|---|---|---|
| 1 | 61.17858 | 9.67115 |
| 2 | 61.24672 | 9.58565 |
| 3 | 61.28958 | 9.41855 |
| 4 | 61.28476 | 9.24507 |
| 5 | 61.28503 | 9.07507 |
| 6 | 61.29282 | 8.90955 |
| 7 | 61.32158 | 8.76996 |
| 8 | 61.35262 | 8.58492 |
| 9 | 61.36475 | 8.38227 |
| 10 | 61.38402 | 8.19301 |
| 11 | 61.43921 | 8.07131 |
| 12 | 61.51009 | 8.02321 |
| 13 | 61.59053 | 7.97762 |
| 14 | 61.64550 | 7.91965 |
| 15 | 61.66260 | 7.75250 |
| 16 | 61.74457 | 7.78512 |
| 17 | 61.79783 | 7.66926 |
| 18 | 61.82256 | 7.48442 |
| 19 | 61.88061 | 7.51093 |
| 20 | 61.89180 | 7.37450 |
| 21 | 61.90354 | 7.21025 |

### km / day (soft 8 h hiking budget)

| Day | km | hike hours |
|---|---|---|
| 1–7 | 33.82 each | 8.0 |
| 8 | 8.20 | 1.94 |
| **Total** | **244.9** | ~57.9 |

### Nav instruction type counts

| kind | count |
|---|---|
| left | 54 |
| right | 49 |
| straight | 2 |
| destination | 2 |

---

## DNT / place screenshots

Host GPS replay along planned polyline (6870 pts → 1375 samples).
Screenshots named by place:

| Place | File |
|---|---|
| Fondsbu (via DNT cabin) | `docs/evidence/hiking-long-test/dnt_cabins/Fondsbu.png` |
| Hjelle motell (end) | `docs/evidence/hiking-long-test/dnt_cabins/Hjelle_motell.png` |
| Nashaugsætra (start) | `docs/evidence/hiking-long-test/dnt_cabins/Nashaugsætra.png` |

Also campaign UI frames `01_launch` … `12_after_gps_replay` under
`docs/evidence/hiking-long-test/screenshots/` (route visible on plan/GPS frames).

---

## Crashes / ANRs / memory

| Item | Value |
|---|---|
| Instrument | **OK (1 test)** |
| App FATAL / ANR / OOM / LMK | **None** for `no.navi.app` |
| Logcat alert (host) | 1× unrelated `com.google.android.apps.safetyhub` process died |
| Peak TOTAL_PSS | **549 326 kB** (~536 MiB) during GPS replay |
| RAM (test report) | before 314 MiB → post-plan 522 → final 523 MiB |

Excerpt (`docs/evidence/hiking-long-test/logs/crash_anr_excerpts.txt`):

```
11-21 11:44:36.981 ActivityManager: Process com.google.android.apps.safetyhub (pid 17960) has died: cch+65 CEM
```

Plan/tunnel excerpt:

```
NaviPlan: planner=mwm profile=HIKING ... tunnel_avoid=true/true
NaviPlan: mwm_enrich ... path=route_tunnel_count=0
NaviRouting: planning_done ... duration_ms=30053 distance_km=244.905
```

Attractions excerpt:

```
NaviPlan: mwm_attractions seed empty query=Severdigheter samples=48 max_dist_m=850.0
NaviAttractions: lookahead tally attraction_count=0 types=0 samples=48
```

---

## Evidence index

| Artifact | Path |
|---|---|
| This report | `docs/hiking-long-test.md` |
| Device report | `docs/evidence/hiking-long-test/report/final_device_report.json` |
| Host summary | `docs/evidence/hiking-long-test/report/host_summary.json` |
| Route result | `docs/evidence/hiking-long-test/report/route-result.json` |
| Polyline | `docs/evidence/hiking-long-test/route-polyline.txt` |
| Host / instrument logs | `docs/evidence/hiking-long-test/logs/` |
| Screenshots + DNT | `docs/evidence/hiking-long-test/screenshots/` + `dnt_cabins/` |
| Host tmp tree | `/tmp/hiking-fondsbu-hjelle-campaign/` |

---

## Notes

1. **Tunnels = 0** on this Fondsbu/Hjelle corridor (unlike prior Sandhaug/Lothepus run).
2. **Attractions = 0** because **none exist along this route** (`mwm_attractions seed empty` + cone tally 0 with nearby ON). Not a product FAIL for missing POIs.
3. **Ferry = 1** with Avoid ferries OFF — reported, not a settings violation.
4. androidTest OD constants adapted for Fondsbu/Hjelle and included with this campaign commit.
5. Dirty Leira radial WIP was kept out of this commit (stashed separately on the prior working branch).
