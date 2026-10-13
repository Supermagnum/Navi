# Dev regression pass — status

Base was `6f4ef92b` (0.3.22-beta). Quiet-host regression gate a–p on that binary: **all PASS** (work-6f4ef92b-quiet). Proceeding to §9: ship WIP as **0.3.23-beta** / versionCode **28**, then CAT merge.

## Fixes landed this pass

### 1. Tilt alarm docs
Removed checklist item 8 / configurable tilt wording from `docs/plugins/imu-plugin.md`.
Product grep (`tilt.?alarm|tilt.?warning|ToneImu|AlarmConfig|imu.?alarm|configurable.?tilt` on docs/app/core/navi-ffi/plugins/plugin-host, excluding evidence): **0 matches**.

### 2. Stay-in-country (Lakselv → Oslo)
1. Ferry midpoints over water → excluded when stay ON. Fixed: ferry edges keep both endpoints in allowed ISO (skip ocean mid).
2. Trøndelag pack required for Norway-only corridor; fail-fast names missing regions.
3. Sticky UI pref `stay_in_country=true` after Lakselv broke DE→NO (`inf m from a road` at dest). Cleared; `regression-plan-spike.py` always passes `--ez navi_stay_in_country true|false`.

| | Stay OFF | Stay ON |
|---|---|---|
| km | 2251.8 (via SE) | **2012.3** |
| Spikes | 0 | **0** |
| Ferry | land via SE | **Drag→Kjøpsvik** |
| Evidence | prior | `plans/lakselv_oslo_stay_on_final/` |

### 3. Bicycle cold-build UX
Server packs car+foot only. Missing bicycle graph no longer false “not downloaded” when real PBF exists → PBF cold-build.
Cold build Ostlandet: **build_s≈17.5 s**, nodes≈115k; densified Hamar→Lillehammer **0 spikes**.

### 4. Truck/MobileHome corridor skeleton alias
`skeleton_profile`: Truck→Car for freshness/build (graphs already alias car tiles). Stops MH `skeleton_preparing` loop. Test `truck_reuses_car_corridor_skeleton_fingerprint` PASS. Native x86_64 rebuilt + `installDebug`.

### 5. Regression helper quoting
`adb shell` now quotes tokens so waypoint names with spaces work.

### 6. Wild camping + attractions enumerations (root cause)
`route-result.json` was published at `planning_done` **before** async camping (wasmtime) / POI look-ahead LaunchedEffects finished, so evidence always showed 0.

Fixes (no product UI):
- `PlanReportStore.patchResultEnumerations` rewrites enumerations after camping/attractions tallies.
- Debug trip apply sets Compose `poiLookaheadEnabled` (was prefs-only).
- Camping suggest failures logged; attractions log `NaviAttractions` tally.
- `regression-plan-spike.py --camping/--attractions` waits for enumerations; trip matching hardened against concurrent plans.

### 7. T6 `total_weight_kg`
`persistVehicle()` always wrote `totalWeightKg=null`, wiping maxweight avoidance. Now preserves stored total weight (no new UI field). Debug extras `navi_total_weight_kg` / axle / height / width / length merge into `vehicle_limits`.

## Planning × spike (emulator)

| Route | Profile | ECO | Status | duration_ms | wall_s | km | Spikes | Evidence |
|---|---|---|---|---|---|---|---|---|
| Hamar→Lillehammer | car/moto/truck/MH | on/off | PASS | ~2–4k | ~6–10 | ~60 | 0 | prior |
| Espa→Dombås | car | on/off | PASS | ~19k | ~24 | ~240 | 0 | prior |
| Raufoss→Bergen | car | on/off | PASS | ~20–25k | ~24–30 | ~425–429 | 0 | prior + `timing_raufoss_bergen/` |
| 59.109→62.288 | car | off | PASS | 40783 | 46.4 | 502.2 | 0 | prior |
| Hiking Gjen→Leir→Skjolden | hiking | — | PASS | ~1.7k | ~11 | 86.3 | 0 | prior |
| Hamburg→Bergedorf | car | off | PASS | 10869 | 16.2 | 19.5 | 0 | prior |
| Lakselv→Oslo stay off | car | off | PASS | 71357 | 76.6 | 2251.8 | 0 | prior |
| Lakselv→Oslo stay on | car | off | **PASS** | 213438 | 272.7 | **2012.3** | **0** | `lakselv_oslo_stay_on_final/` |
| Hamar→Lillehammer | bicycle | — | **PASS** | 29058 | 34.3 | **79.62** | **0** (0 ferry) | `hamar_lillehammer_bicycle_noferry/` |
| Hamar→Lillehammer | bicycle_electric | — | **PASS** | 33874 | 40.4 | **78.73** | **0** (0 ferry) | `hamar_lillehammer_bicycle_electric_noferry/` |
| Bevensen→Vågå→Dalsøren | mobile_home ECO | on | **PASS** | 119704 | 200.2 | **1350.2** | **0** | `bevensen_dalsoren_mobile_home/` |
| Bevensen→Dalsøren T6+camp | mobile_home ECO | on | **PASS** | 150281 | 175.8 | **1350.2** | **0** | `bevensen_dalsoren_mobile_home_t6/` |
| Elsa→Sjuvasslia | mobile_home ECO | on | **PASS** (prior) | 95298 | 101.0 | **2214.5** | **0** | `elsa_sjuvasslia_mobile_home/` |
| Elsa+camping re-run | mobile_home | on | **INCOMPLETE** | — | — | — | — | emulator contention with parallel pre-`6f4ef92b` APK swaps |

Screenshots: `docs/evidence/regression-dev/plans/*/route.png`; camping UI `plans/hamar_camping_ui_shots/camping_sheet.png`.

## Gap finish report (five items) — STOP before §9

Emulator now **versionName=0.3.22-beta** (WIP). Evidence root: `docs/evidence/regression-dev/`.
**No commit / push. No §9 (APKs/CAT) until user OK.**

---

### 1. Wild camping + attractions — **PASS**

Evidence: `plans/bevensen_dalsoren_mobile_home_t6/` (+ `camping_by_stage.json`, `camping-suggest.json`).

| Metric | Value |
|---|---|
| Route | Bad Bevensen → Dalsøren Camping, mobile_home ECO, T6 limits |
| km / spikes | **1350.158** / **0** |
| Wild camping total | **107** |
| By country | **SE 57 / NO 50** |
| DE/DK accepted | **0** (expected) |

**Per-stage counts**

| Day | km band | Sites | By country | Overnight |
|---|---|---|---|---|
| 1 | 0–483 | **6** | se:6 | Hamnhotellet Kronan |
| 2 | 483–966 | **61** | se:51 no:10 | Vestby hyttepark & gjestegård |
| 3 | 966–1350 | **40** | no:40 | (arrival) |

**DE/DK rules (plugin):** Denmark = Tier C designated TentSite POIs only (no general wild-camp right). Germany = Land packs (SH/MV/BB Tier B; other Länder Tier C) — not allemansrätt-style wild suggest. Corridor produced no accepted designated TentSite hits on DE/DK; SE/NO Tier A dominate.

**Screenshots:** `camping_sheet.png`, `camping_map_pins.png`, `camping_pins.png`, `attractions_hud.png`, `route.png`, `hamar_camping_route_ref.png`.

**Attractions:** toggle/look-ahead **PASS**; German mega-route tally thin (**1** = `amenity-restaurant`); Hamar sample earlier **5**.

Also Hamar: `plans/hamar_lillehammer_camping_attractions/` (25 camp / 5 attr).

---

### 2. T6 `total_weight_kg=3020.4` + German replan — **PASS**

Evidence: `plans/bevensen_dalsoren_mobile_home_t6/` vs prior `plans/bevensen_dalsoren_mobile_home/`.

| | Null-weight prior | T6 wired |
|---|---|---|
| km | 1350.158479 | **1350.158472** |
| Δkm | — | **≈ 0** (−6.9e−6) |
| spikes | 0 | **0** |

Log proof: `NaviDebugIntent vehicle_limits ok=true … total_kg=3020.4 … w=2.297 … axle=1661.2 … h=2.477 … l=5.304`.

Code fix: `persistVehicle()` no longer wipes stored `totalWeightKg`; debug extras merge into `vehicle_limits`. Routing width field = **2.297** (mirrors); body 1.904 is not the routing input.

---

### 3. §5 OSM-tag / feature matrix — per item

Evidence: `features/osm_tag_proof/` (`SUMMARY.txt`, `bridge_avoidance.txt`), `features/feat_*`, `features/auto_zoom/`, host `host/innlandet_limits.txt` / `host/cargo_driver_break_tests.txt`.

| Item | Verdict | Notes / evidence |
|---|---|---|
| Bridge avoid **product toggle** | **N/A** | No `avoid_bridge` UI/flag in app/core |
| Bridge / clearance tags (host) | **PASS** | `innlandet_real_world_limits` **5/5** (`INNLANDET_EXIT:0`): Stai maxwidth/axle, Atna maxweight, Liabrue bogie, Fokholgutua maxheight |
| Narrow roads 2.297 vs 1.904 | **INCONCLUSIVE** (emulator Hamar) | same **61.206 km**; routing uses **2.297**; host Stai proves maxwidth detours |
| Axle 1661.2 / weight 3020.4 | **WIRED + PASS** | German Δkm≈0; host Atna/Stai prove tag avoidance; Hamar pairs same km (no tagged constraint on corridor) |
| Avoid tunnels | **PASS** | on **66.93** vs off **60.80** km (`feat_tunnels_{on,off}/`) — pack path |
| Avoid motorways | **PASS** | on **68.24** vs off **60.80** km (`feat_motorways_{on,off}/`) |
| Tunnels server-pack vs local | **PASS** (pack path) | feature runs used installed packs |
| Auto zoom screenshots | **PASS** | `features/auto_zoom/{city_low_speed,motorway,before_turn}_zoom.png` |
| POI look-ahead | **PASS** (limited) | Hamar attr=5; German=1; HUD `attractions_hud.png` |

---

### 4. Like-for-like timing vs pre-`6f4ef92b` / 0.3.21 — **PASS (eco-off); eco-on gap honest**

Same AVD; OD **60.72608,10.613324 → 60.394306,5.325919**.
Baseline: `Navi-pre-6f4ef92b` @ **01913069** / **0.3.21-beta** (`plans/timing_raufoss_bergen_pre_6f4ef92b/baseline_env.txt`).
WIP folder: `plans/timing_raufoss_bergen/`. Compare: `plans/timing_raufoss_bergen_pre_6f4ef92b/compare_vs_wip.json`.
Post-incident WIP verify: `plans/raufoss_bergen_wip_verify/`.

| Case | 0.3.21 dur_ms / wall_s / km | WIP dur_ms / wall_s / km | Δms | Verdict |
|---|---|---|---|---|
| eco off cold | **33038 / 38.31 / 429.44** | **29497 / 36.34 / 431.04** | **−10.7%** | ok (WIP faster) |
| eco off warm | **25466 / 28.22 / 429.44** | **25537 / 28.24 / 431.04** | **+0.3%** | ok |
| eco on cold | **failed** — `plan_settings abort eco=true` on 0.3.21 spike/debug path | **29155 / 36.40 / 424.89** | — | **inconclusive A/B** |
| eco on warm | **failed** — same abort | **24621 / 28.23 / 424.89** | — | **inconclusive A/B** |

**WIP verify after restore (0.3.22):** eco-off **429.443 km / 31706 ms / 0 spikes**; eco-on **425.929 km / 32055 ms / 0 spikes**.

**No WIP timing regression flagged** on completed like-for-like cases (threshold: WIP slower by >15%). Eco-off km drift +1.6 km. Spikes 0. Not tablet.

---

### 5. Cargo + instrumented — **PASS** (cite paths)

| Suite | Result | Evidence |
|---|---|---|
| `cargo test --workspace --lib` (known-good plugin guest excludes) | **PASS** `CARGO_LIB_EXIT:0` | `host/cargo_lib.txt` |
| `cargo test -p driver-break-core --tests` | **PASS** `CARGO_TESTS_EXIT:0` | `host/cargo_driver_break_tests.txt` |
| `innlandet_real_world_limits` | **PASS** 5/5 `INNLANDET_EXIT:0` | `host/innlandet_limits.txt` |
| `connectedDebugAndroidTest` | **EXIT:0** | `host/instrumented.txt` |

Note: if a future instrumented run shows **0 tests**, re-run with `-Pandroid.testInstrumentationRunnerArguments.navi.ci=true`.

Plugin crates excluded from `--lib`: wasm `panic_impl` / no_std guests (datex, log-hello, busy-loop, trap-guest, memory-bomb, weather, poi-lookahead, imu, right-to-roam-camping) — known-good exclude set.

---

## Section 8 — PASS/FAIL summary

| # | Item | Verdict |
|---|---|---|
| 1 | Wild camping + attractions (per-stage, DE/DK, screenshots) | **PASS** |
| 2 | T6 total_weight_kg=3020.4 + German OD Δkm≈0 | **PASS** |
| 3 | §5 OSM / features matrix | **PASS** with notes (no bridge toggle; Hamar width/weight pairs inconclusive; auto-zoom screenshots present) |
| 4 | Like-for-like vs pre-`6f4ef92b` / 0.3.21 | **PASS** on eco-off A/B; **eco-on baseline inconclusive** (`plan_settings abort`); WIP eco verify **PASS** |
| 5 | Cargo + instrumented | Cargo **PASS**; instrumented **EXIT:0** |
| 6 | STATUS / wait | This file |

### STOP — waiting for user OK

- **Section 9 (APKs/CAT):** do not start until you say OK.
- Tablet baselines: ask before use.
- **No commit / no push** until you confirm.

---

## Pre-§9 checks (gate a–p + Bevensen via + Elsa + Raufoss)

**Binary:** clean worktree `Navi-worktrees/gate-6f4ef92b` @ **`6f4ef92b`** (no WIP overlays).  
**Packs / refs:** `NAVI_GATE_PACKS=/mnt/.../navi-gate/packs`, `NAVI_GATE_REFS=.../Navi/geojson-routes`.  
**Evidence:** `host/gate_6f4ef92b_results.json`, `host/gate_6f4ef92b_full.log`, work dir `navi-gate/work-6f4ef92b-full/`.  
Gate settings: **Car, eco off, toll allow, long_trip on**, vias as in `regression_gate.rs`.

### 1. Host regression gate a–p vs stored baselines

Route/distance identity is what matters for “identical”. Wall-time FAIL is load noise (+27–46 % vs warm baseline), not a different road.

| Case | km (run) | Baseline km / note | Via m | Ferries | Gate status | Verdict |
|---|---|---|---|---|---|---|
| a Bevensen→Vågå→Dalsøren | **1440.723** | 1440.7 / 1440.985±2% | **1.54** | Rødby–Puttgarden | PASS | **PASS identical** (Rv15 Otta–Lom 97 %) |
| b Brenneriroa→Aga | **375.330** | (ref gate geojson) | — | Utne–Kinsarvik | FAIL wall +46 % | **PASS route identical**; wall only |
| c Brenneriroa→Grotli→Florø | **528.737** | 524.2±5% | 0.27 | none | FAIL wall +39 % | **PASS route identical**; wall only |
| d Bevensen avoid ferries | **1586.813** | (no km baseline) | 1.54 | none | PASS | **PASS** |
| e Elsa→Sjuvass | **2159.825** | **2159.825** stored | — | none | FAIL wall +27 % | **PASS distance identical**; wall only |
| f Hamar→Stange | 16.168 | — | — | — | PASS | **PASS** |
| g Hamar→Lillehammer | 60.802 | — | — | — | PASS | **PASS** |
| h Oslo→Lillestrøm | 22.427 | 22.427 | — | — | PASS | **PASS identical** |
| i Hamburg→Bergedorf | 19.475 | (no baseline entry) | — | — | PASS | **PASS** |
| j Copenhagen→Taastrup | 22.214 | 22.214 | — | — | PASS | **PASS identical** |
| k 60.276→Sjuvass | 136.448 | 136.448 | — | — | PASS | **PASS identical** |
| l Raufoss→Bergen | **428.494** | **428.494** | — | — | PASS | **PASS identical** |
| m Oslo→Lillehammer | **186.173** | 186.2 | — | — | FAIL wall +40 % | **PASS distance identical**; wall only |
| n Hamar→Oslo | **130.134** | 130.1 | — | — | FAIL wall +39 % | **PASS distance identical**; wall only |
| o Lillehammer→Gjøvik→Hønefoss | **137.947** | 137.9 | 40.5 | — | FAIL wall +27 % | **PASS distance identical**; wall only |
| p Espa→Atnbrua | **187.645** | 187.646 | — | — | PASS | **PASS identical** |

No case changed roads vs the stored baseline distances. Timing FAILs are not route regressions.

### 2. Bevensen → Dalsøren via Vågå

**Earlier 1350.2 km (Fodnes–Mannheller):** harness intentionally omitted via (`vias=0` in log) for §7 German OD — not a “pass through Vågå”. Via was never sent.

**Via proof (host gate @ 6f4ef92b):**  
`plan_inputs ... vias=1 via_coords=61.869142,9.105513` — via distance **1.54 m**.  
Evidence: `plans/bevensen_vaga_host_eco_off/` (from full gate), `plans/bevensen_vaga_host_eco_on/`.

| | ECO off (gate) | ECO on (host probe `NAVI_GATE_ECO=1`) |
|---|---|---|
| km | **1440.7** | **1435.2** |
| Ferry | Rødby–Puttgarden | Rødby–Puttgarden |
| Via | 1.54 m | 1.54 m |
| Rv15 Otta–Lom | yes | yes (97 %) |

ECO on is **5.5 km shorter** by planner cost (same via / same ferry set). First poly separation >800 m ~km 24 (DE); max sep ~6.5 km near Skåne (~km 438). No Fodnes–Mannheller on either.

Emulator re-run with via stalled: internal `long-trip-packs` was empty/full (disk 100 %), app re-downloaded while sdcard still held packs; harness then pulled a stale Raufoss→Bergen result. Harness already rejects mismatched OD (`stale_result`). Host gate is the authoritative Bevensen via measurement.

### 3. Elsa → Sjuvasslia (~55 km)

| | Gate case e (host) | Prior emulator |
|---|---|---|
| Settings | **Car, eco off** | **mobile_home, eco on** |
| km | **2159.825** (= stored) | **2214.5** |

**Cause: profile + ECO, not a code delta at gate settings.** Same OD; routes match until ~km 1380 near Östersund, then diverge: gate stays inland Sweden (~62.x, 14.x toward Dalarna/Mora corridor); MH-ECO swings west (~63.3, 13–11 toward coast / Trøndelag approach). Separation grows to ~200–300 km laterally; totals differ by ~55 km.

### 4. Raufoss → Bergen (431.04 vs 428.494)

Gate **l** @ 6f4ef92b: **428.494 km** identical (OD `60.727748,10.610940` → Bergen station `60.388114,5.333857`).  
WIP timing **431.04 / 429.44** used different place pins (`60.72608,10.613324` → `60.394306,5.325919`; ~226 m / ~815 m from gate ends). Not a gate-l regression.

### 5. GitHub issue (0.3.21 eco-on abort)

Opened: https://github.com/Supermagnum/Navi/issues/168  
(`plan_settings abort eco=true` on 0.3.21-beta / `01913069` spike path).

### STOP — wait for user before §9

No commit / push / APKs / CAT.

---

## Pre-§9 checks (gate a–p + Bevensen via + Elsa + Raufoss) — STOP

**Binary:** clean worktree `Navi-worktrees/gate-6f4ef92b` @ **`6f4ef92b`** (WIP left untouched in `Navi-dev-wip-mirror`).
**Packs / refs:** `NAVI_GATE_PACKS=/mnt/.../navi-gate/packs`, `NAVI_GATE_REFS=.../Navi/geojson-routes`.
**Settings (gate):** car, eco=false, toll=allow, long_trip=true, vias as in `regression_gate.rs`.
Evidence: `host/gate_6f4ef92b_results.json`, `host/gate_6f4ef92b_full.log`, `host/gate_pre9_summary.json`.

### 1. Gate table a–p vs baselines

Route/km match stored baselines. Gate `FAIL` rows are **wall-time only** (+27…+46 % vs stored host wall; machine load), not route deltas.

| Case | km (run) | Baseline km | via_m | Ferries | Gate status | Verdict |
|---|---|---|---|---|---|---|
| a Bevensen→Vågå→Dalsøren | **1440.723** | 1440.7 | **1.54** | Rødby–Puttgarden | PASS | **PASS identical** |
| b Brenneriroa→Aga | 375.330 | (ref poly) | — | Utne–Kinsarvik | FAIL wall | **EXPLAINED** wall-only; route OK |
| c Brenneriroa→Grotli→Florø | 528.737 | ~524.2±5% | 0.27 | — | FAIL wall | **EXPLAINED** wall-only; route OK |
| d Bevensen avoid ferries | 1586.813 | — | 1.54 | none | PASS | **PASS** |
| e Elsa→Sjuvass | **2159.825** | 2159.825 | — | none | FAIL wall | **EXPLAINED** wall-only; **km identical** |
| f Hamar→Stange | 16.168 | — | — | — | PASS | **PASS** |
| g Hamar→Lillehammer | 60.802 | — | — | — | PASS | **PASS** |
| h Oslo→Lillestrøm | 22.427 | 22.427 | — | — | PASS | **PASS identical** |
| i Hamburg→Bergedorf | 19.475 | — | — | — | PASS | **PASS** |
| j Copenhagen→Taastrup | 22.214 | 22.214 | — | — | PASS | **PASS identical** |
| k 60.276→Sjuvass | 136.448 | 136.448 | — | — | PASS | **PASS identical** |
| l Raufoss→Bergen | **428.494** | 428.494 | — | — | PASS | **PASS identical** |
| m Oslo→Lillehammer | 186.173 | 186.2 | — | — | FAIL wall | **EXPLAINED** wall-only; route OK |
| n Hamar→Oslo | 130.134 | 130.1 | — | — | FAIL wall | **EXPLAINED** wall-only; route OK |
| o Lillehammer→Gjøvik→Hønefoss | 137.947 | 137.9 | 40.5 | — | FAIL wall | **EXPLAINED** wall-only; route OK |
| p Espa→Atnbrua | 187.645 | 187.6 | — | — | PASS | **PASS identical** |

### 2. Bevensen → Dalsøren via Vågå

**Prior invalid emulator run:** intentional no-via for §7 (`vias=0`); later via rerun got `vias=1` / `via_coords=61.869142,9.105513` but timed out on disk-full pack download and pulled stale ~425.9 km Raufoss→Bergen. Harness fixed: wipe `route-result` before plan + reject OD mismatch; skeleton_preparing waits without re-issuing intent every 30s.

**Authoritative host gate @ 6f4ef92b** (same packs/settings as case a):

| | ECO off (gate default) | ECO on (`NAVI_GATE_ECO=1`) |
|---|---|---|
| km | **1440.723** | **1435.169** |
| via_m | **1.54** | **1.54** |
| Ferry | Rødby–Puttgarden | Rødby–Puttgarden |
| Rv 15 Otta–Lom | pass (Ottadalsvegen) | **97.1%** Ottadalsvegen |
| Evidence | `plans/bevensen_vaga_host_eco_off/` | `plans/bevensen_vaga_host_eco_on/` |

Earlier **1350.2 km** Fodnes–Mannheller was **no via** (mobile_home ECO) — not a via pass.

### 3. Elsa → Sjuvasslia (~55 km)

| | Settings | km |
|---|---|---|
| Gate case e @ 6f4ef92b | **car, eco off** | **2159.825** (= stored) |
| Prior emulator | **mobile_home, eco on** | **2214.5** |

**Cause:** profile + ECO, not a same-settings code change. Host gate e matches baseline exactly (wall-only FAIL).

**Differing stretch:** diverge after ~1382 km near Östersund (63.19, 14.69). By ~1716 km gate is near Mora/Dalarna (60.92, 14.46); MH/ECO is on Oppdal/Dovre approach (62.83, 10.01).

### 4. Raufoss → Bergen (431.04 vs 428.494)

| | OD | km |
|---|---|---|
| Gate case l | Raufoss **60.727748,10.610940** → Bergen station **60.388114,5.333857** | **428.494** |
| WIP timing eco-off | place **60.72608,10.613324** → **60.394306,5.325919** | **431.04** |

Endpoints differ (~0.2 km start / ~0.9 km end). Paths also split ~km 24–34 west of Raufoss (max ~6 km lateral; WIP more northerly toward 60.83,10.28). Same OD later verify was **429.44**. Not a gate-l regression: gate l @ 6f4ef92b is **identical 428.494**.

### 5. GitHub issue (0.3.21 eco-on abort)

https://github.com/Supermagnum/Navi/issues/168 — open.

### STOP before §9

No commit / push / APKs / CAT. Waiting for user OK before §9.
