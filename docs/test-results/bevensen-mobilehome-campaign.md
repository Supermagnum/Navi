# Bevensen → Dalsøren MobileHome UI campaign — test result

June 2026 open-roads retest (guest clock `2026-06-01T08:00` CEST). Source ART: `/tmp/bevensen-mobilehome-campaign/attempt_20261124_015755`.

**Verdict: PASS (accepted)** — distance **1425.907 km** with via **Vågå**. The published no-via band **1461.3–1648.6** does not apply under the via + correct-date (June open-roads) rule: the via shortens the corridor and the lower figure is expected and approved. Other campaign metrics PASS. This file records the accepted result for branch `tests`.

| Field | Value |
|---|---|
| Branch / base commit at run | `tests` @ `13f2469df` |
| Serial | `emulator-5554` |
| Via | Vågå / Ottadal `61.8691419, 9.1055130` |
| Planner | MWM (`PLANNER=mwm`, `comaps_route_code=OK`) |
| PASS_UI | `dist=1425.907 etaMin=1023.4 man=58 datex=false wild=114 attractions=22` |
| **Tunnels** | App enum **0**; OSM-along-route **29** named/carriageway (see §6) |

---

## 1. Bands (pass/fail)

| Metric | Measured | Band / rule | Result |
|---|---|---|---|
| Distance km | **1425.907** | Via Vågå + June open-roads (accepted; no-via band 1461.3–1648.6 is reference only) | **PASS (accepted)** |
| Duration h | **17.057** (1023.4 min) | 17–22 (test uses 17.0–22.5) | **PASS** |
| Maneuvers / instructions | **58** | 55–100 | **PASS** |
| Wild camping sites | **114** | several / >0 | **PASS** |
| Attractions | **22** | several / >0 | **PASS** |
| Tunnels (enumerated) | App **0** / OSM **29** | report required; names if any | **App counter gap**; geometry has tunnels (see §6) |

**Overall:** **PASS (accepted)**. Distance is below the historical no-via floor because the **Vågå via** shortens the corridor; that outcome is approved under the via + correct-date rule. Version/tag policy is separate from this campaign acceptance.

---

## 2. Regions / long-trip corridor

All 9 corridor regions reported **Indexed** before plan:

`Niedersachsen · Schleswig-Holstein · Denmark · Skåne · Halland · Västra Götaland · Ostlandet · Sorlandet · Vestlandet` (1–9 of 9).

`corridor_ready=true` at plan wait. Graph log: `graph=local-pbf` / pack_hit=false on maneuver dump (MWM-backed plan path with local country files).

UI wipe of Geofabrik paths failed (`field_geofabrik_path` missing) — packs policy then `ui_wipe_then_auto_download`; MWM path used.

---

## 3. Disk

From `logs/progress_watch.txt` during run:

| Path | Size |
|---|---|
| `files/long-trip-packs` (app) | **0 MB** (`packs_mb=0`) |
| SD `long-trip-packs` | **1 MB** (`sd_packs_mb=1`) |
| `files/mwm` | **2202 MB** (`mwm_mb=2202`) |

No Geofabrik PBF growth abort. MWM data dominated disk use.

---

## 4. RAM

`monitor/meminfo_*.txt` TOTAL PSS samples (21 non-empty):

| | Value |
|---|---|
| Max TOTAL PSS | **940977 KB ≈ 918.9 MB** |
| Min TOTAL PSS | **316416 KB ≈ 309.0 MB** |
| Late sample (`meminfo_0920`) | **939612 KB ≈ 917.6 MB** |

Several mid-run dumps were empty (34-byte stubs) while process was between instrumentation and resume; peak during active app ≈ 919 MB PSS.

---

## 5. Ferries

| Source | Value |
|---|---|
| Enumeration `route_ferry_legs` | **0** / null in dump footer (`route_uses_ferry=null`) |
| Maneuver text | **Yes** — Rodby–Puttgarden ferry present |

Evidence (maneuver dump):

- Street / text: `Rødby (DK) - Puttgarden (D)`
- English: `Take the ferry to Sydmotorvejen (E 47)` (4 EN / 4 NB ferry phrasings in dump)
- GPS town screenshots include **Puttgarden** and **Rodby**

**Note:** Ferry is on the route in instructions and GPS path, but structured ferry-leg enumeration stayed 0/null — report both facts; do not claim zero ferries.

Avoid-ferries: off (default). Eco UI ON; router log for finished plan: `eco=false`.

---

## 6. Tunnels

| Source | Value |
|---|---|
| `report/route-result.json` → `enumerations.route_tunnel_count` | **0** |
| `enumerations.route_tunnel_fp` | empty |
| Maneuver dump footer `route_tunnel_count` | **null** (field not filled on dump path) |
| Instruction streets / `text_en` / `text_nb` | no “tunnel” / “Tunnel” in any of 58 maneuvers |
| Plan settings | `avoid_tunnels=false` (`NaviRouting plan_settings`) |
| **OSM-along-route (post-ART)** | **29** tunnels (~36.5 km approx.) — see table below |

**Why the app reported 0:** This run used `PLANNER=mwm` / `TEST_KIND=PLAN_MWM_ROUTE`. The MWM/CoMaps plan report string has **no** `route_tunnel_count` / `route_tunnel_fp` tokens (only distance, ETA, ascent, `comaps_route_code`, …). `RoutePlanStats` then defaults missing tokens to **0** / empty — the same class of instrumentation gap as ferry legs (ferry present in maneuvers, `route_ferry_legs=0`). Tunnel legs are produced by the **local PBF graph** path (`path_tunnel_legs` on `is_tunnel` edges), which this MWM plan did not populate. Maneuver text also never names tunnels, so the campaign counter stayed at 0 even though the polyline crosses many OSM `tunnel=*` carriageways.

**Independent re-derivation (docs only, no app code change):** ART polyline `report/route-polyline.txt` matched offline against Geofabrik PBFs (`ostlandet`, `vestlandet`, `sweden`, `denmark` under `navi-gate/packs-fu38-nogav`). Kept ways with `tunnel=*` and `highway` in motorway/trunk/primary (+ links) whose geometry runs **along** the route (not short underpass crossings). Named tunnels (`tunnel` / `lokket` / `tunneln` in `name`) if ≥25% sampled nodes within ~150 m of the polyline; long unnamed carriageway tunnels if length ≥500 m and tightly along-route. Dual-carriageway length ≈ summed way lengths / 2.

**OSM tunnel count: 29** (approx. total length **~36.5 km**).

| along_km | ~length_km | Name | Approx. location |
|---:|---:|---|---|
| 391.3 | 4.74 | Øresundsmotorvejen | Øresund fixed-link tunnel section (DK→SE) |
| 698.3 | 0.54 | Tingstadstunneln | Göteborg / E 6 |
| 826.0 | 0.25 | Gerumsbergstunneln | Bohuslän E 6 |
| 888.4 | 0.05 | Busgårdlokket | E 6 SE Norway |
| 903.7 | 0.10 | Galteryggtunnelen | E 6 |
| 904.2 | 0.86 | Eidettunnelen | E 6 |
| 939.7 | 0.89 | Follotunnelen | E 6 Follo |
| 958.7 | 0.92 | Smiehagen tunnel | E 6 |
| 960.2 | 0.12 | Vassumtunnelen | E 6 |
| 960.5 | 3.83 | Nordbytunnelen | E 6 |
| 965.9 | 3.62 | Nøstvettunnelen | E 6 |
| 984.0 | 0.16 | Ryentunnelen | Oslo approach |
| 985.6 | 0.28 | Brynstunnelen | Oslo / Bryn |
| 1029.7 | 0.06 | Hauerseterlokket | E 6 north of Oslo |
| 1033.5 | 0.04 | Elstadmolokket | E 6 |
| 1048.7 | 1.17 | Eidsvolltunnelen | E 6 Eidsvoll |
| 1060.2 | 0.63 | Korslundtunnelen | E 6 |
| 1063.6 | 2.33 | Morskogstunnelen | E 6 |
| 1076.4 | 0.68 | Espatunnelen | E 6 Espa |
| 1132.9 | 0.33 | Skarpsnotunnelen | E 6 toward Lillehammer |
| 1163.2 | 0.23 | Jernbanetorgtunnelen | Lillehammer area |
| 1163.4 | 0.29 | Sorgendalstunnelen | Lillehammer / E 6 |
| 1164.9 | 0.21 | Mosoddentunnelen | north of Lillehammer |
| 1187.1 | 3.85 | Øyertunnelen | Gudbrandsdalen E 6 |
| 1227.5 | 4.29 | Hundorptunnelen | Gudbrandsdalen E 6 |
| 1248.1 | 3.76 | Teigkamptunnelen | Gudbrandsdalen E 6 |
| 1314.1 | 0.52 | Byrebergtunnelen | Ottadalen Rv 15 (near via Vågå) |
| 1417.2 | 1.20 | Gullringtunnelen | Rv 55 toward Dalsøren |
| 1420.4 | 0.56 | Ottatunnelen | Rv 55 near destination |

**Note:** Same pattern as §5 Ferries — do **not** claim “zero tunnels on the corridor” from the app counter alone. The polyline crosses E 6 / Øresund / Rv 55 tunnel infrastructure; the MWM enumeration path failed to report them.

---

## 6b. Route corridor (where the route went)

Bevensen → Dalsøren with via **Vågå** `61.86914, 9.10551`. Major corridor from instructions + polyline:

1. **Germany:** Bad Bevensen → Lüneburg (B 4) → B 404 / Lübeck approach → A 1 / B 207 toward Fehmarn → **Puttgarden**.
2. **Ferry:** **Rødby (DK) – Puttgarden (D)** onto **Sydmotorvejen (E 47)**.
3. **Denmark:** E 47 → Køge Bugt Motorvejen (E 20/E 47/E 55) → **E 20** toward Sweden.
4. **Sweden:** E 20 / **E 6** via Halland / Göteborg corridor (GPS towns Halmstad, Göteborg) north on **E 6**.
5. **Norway (Ostlandet):** Svinesund (~along 873 km) → past Oslo (~3.2 km nearest, along ~987) on **E 6** → **past Hamar** → Lillehammer → Gudbrandsdalen on **E 6** to **Otta**.
6. **Via Vågå:** leave E 6 at Otta onto **Ottadalsvegen (Rv 15)** to via (~along 1301 km).
7. **Vestlandet finish:** **Sognefjellsvegen / Lustravegen (Rv 55)** to **Dalsøren**.

### Hamar

**Yes — passes near Hamar.** Polyline nearest point `(60.8088, 11.1061)` is **2.61 km** from Hamar center `(60.7945, 11.0680)` at ~along **1107 km** (154 polyline samples within 5 km). Instructions also use street name **Hamarvegen (213)** later near Lillehammer (along ~1158), which is the Rv 213 link, not Hamar city itself.

### E 6 in Norway

**Yes — E 6 for most of the Norwegian leg** (~**399 km / ~72%** of Norway distance from Svinesund entry ~873 km to Otta leave ~1272 km). Maneuver evidence: repeated `E 6` / `E6` / `Mesnadalsvegen (E 6)` / `Hjalmar Brantings vei (E 6)` from SE border through Mjøsa to north of Lillehammer; leave E 6 at Otta for Rv 15 (via) then Rv 55.

### Lillehammer detour (Rv 213)

Brief local leave of E 6 onto **Storgata / Hamarvegen (213)** ~along **1135–1164 km**, then rejoin E 6 at Lillehammer (`Mesnadalsvegen (E 6)`).

**User hypothesis:** likely road works near Lillehammer. **ART evidence does not confirm that.** Synthetic DATEX sits include `syn-no-e6` (Ostlandet ~60.15, 11.15 — south of this detour, along ~962 km), not a Lillehammer-local sit; DATEX parse failed (`datex=false`), so soft penalties did not drive the plan. Detour cause remains **unproven** in this ART (hypothesis only).

---

## 7. Camping / rest places

| Field | Value | Result |
|---|---|---|
| `wild_camping_site_count` | **114** | PASS (>0) |
| `wild_camping_status` | OK / ok | — |
| By country | se=53, no=61 | — |
| By subdivision | se=53; no-31=7; no-32=13; no-03=2; no-34=34; no-46=5 | — |
| `rest_place_count` | **0** | enumeration empty at campaign time (see gap note) |
| `break_poi_count` / overnight_stops | 0 / `[]` | no overnight stop objects in plan JSON |
| Camping plugin | UI toggle visible, pref ON | — |

**Why `rest_place_count` was 0 (do not invent names):** This Bevensen ART ran **before** `MwmBreakPois` (`aec359ec3`). `MwmProductionPlanner` hard-coded `breakPoisJson="[]"` on the MWM path even when Ostlandet/corridor leaves contain cabin / shelter / picnic POIs. Soft rest interval fields (1.5 h / 15 min) were seeded, but no named break POIs or coordinates were emitted. **No rest-place names are listed here** because none were produced by that ART.

**Post-fix note (2026-11-24, after `MwmBreakPois`):** Full Bevensen MobileHome corridor was not re-planned. Ostlandet MWM smoke on device (`MwmBreakPoisSmokeInstrumentedTest` Hamar→Lillehammer Hiking) returned **`rest_place_count=6`** with named picnic pins + coords. Full Pilgrim Hiking re-run (`docs/test-results/pilgrim-long-test.md`, ART `attempt_20261124_114827`) returned **`rest_place_count=29`**. A dedicated MobileHome Bevensen corridor re-plan for named rest places was not executed in this update.

---

## 8. Instructions (maneuvers)

**Count: 58** (PASS, band 55–100).

Kind histogram:

| kind | n |
|---|---|
| roundabout | 11 |
| straight | 10 |
| left | 8 |
| slight_left | 7 |
| right | 6 |
| exit_right | 6 |
| slight_right | 5 |
| destination | 2 |
| u_turn | 1 |
| sharp_right | 1 |
| exit_left | 1 |

Sample bilingual instructions include U-turn at start, Rodby–Puttgarden ferry, E6/E20 corridors, destination “You have arrived” / “Du er fremme”. Full dump in `logs/host_logcat.txt` (`NaviManeuverDump` 1/7–7/7).

---

## 9. Attractions

| Field | Value | Result |
|---|---|---|
| `attraction_count` | **22** | PASS |
| By type | tourism-attraction=20, tourism-viewpoint=2 | — |
| POI lookahead plugin | UI visible, pref ON | — |

---

## 10. Fuel estimate (report-only)

Vehicle seed: tank **70 L** full; campaign helper notes fuel-stop planning unimplemented.

| Assumption | Range | Usable (−100 km margin) | Est. refuels | Trip liters |
|---|---|---|---|---|
| 11 L/100 km | 636.4 km | 536.4 km | **2** | ~156.8 L |
| 14 L/100 km | 500.0 km | 400.0 km | **3** | ~199.6 L |

Also `estimateFuelStops` style (500/600 mi ranges): stops_at_500mi / stops_at_600mi computable from same distance; not emitted as a separate ART JSON this run. **Not planned by the app.**

---

## 11. DATEX

| Item | Detail |
|---|---|
| Synthetic sits seeded | **6** (`datex_sits.json` / host replay): syn-de-a7, syn-dk-e45, syn-se-e6, syn-se-gbg, syn-no-e6, syn-no-vaga |
| Replay events along polyline | **6** (`report/gps_replay.json`) |
| Cache | `datex-cache.json` — source `server-duckdns` / `navi-synth`, fingerprint ends `syn-no-vaga` |
| XML | `datex-GetSituation.xml` (synthetic SituationPublication) |
| App parse | **FAIL soft:** `parse_failed:datex xml: expected 'severity' tag, not 'validity'` (`NaviDatex` refresh) |
| PASS_UI `datex=` | **false** |
| Plan impact counters | not applied (parse failed → no severity/validity overlay) |

DATEX soft-penalize path did **not** mutate the chosen geometry this run.

---

## 12. Town screenshots (GPS replay)

Host GPS replay OK; 10 towns; elapsed ~383 s; replay distance ~1424.3 km. PNG paths below are relative to the ART directory (not checked into git).

| Town | File |
|---|---|
| Bad_Bevensen | `screenshots/towns/Bad_Bevensen.png` |
| Luneburg | `screenshots/towns/Luneburg.png` |
| Lubeck | `screenshots/towns/Lubeck.png` |
| Puttgarden | `screenshots/towns/Puttgarden.png` |
| Rodby | `screenshots/towns/Rodby.png` |
| Halmstad | `screenshots/towns/Halmstad.png` |
| Goteborg | `screenshots/towns/Goteborg.png` |
| Otta | `screenshots/towns/Otta.png` |
| Vaga | `screenshots/towns/Vaga.png` |
| Dalsoren | `screenshots/towns/Dalsoren.png` |

---

## 13. CAT / hamlib Dummy / repeaters

| Item | Evidence |
|---|---|
| Prefs | `cat_backend=hamlib`, `cat_rig_model=1` (Dummy), `cat_plugin_enabled=true` — `logs/cat_hamlib_dummy_evidence.txt` |
| UI | Disconnect visible after Connect (implies connected); set `2026-11-24T02:09:39Z` |
| Follow | `NaviCat follow_tick ... follow=auto OK: Ok` at Dalsøren and Bevensen coords |
| Repeater candidate | `navi::cat_plugin: cat: candidate LA5MR` (logcat) |

### Rule-12 tension (document only)

Hard rule 12 prefers ADB side-effects limited to **GPS + DATEX**. CAT was brought up / verified with UI prefs and an **`am start` intent to `no.navi.app/.MainActivity`** (beyond GPS/DATEX). That is a documented rule-12 tension; no remediation in this ART (report-only). Instrumentation `am instrument` also triggers ActivityManager force-stop lifecycle (see Errors / rule 14).

---

## 14. Errors / rule notes

1. **Distance vs no-via band** — measured **1425.907 km** is below the historical no-via floor 1461.3; **accepted** under via Vågå + June open-roads (shorter corridor expected). Not a campaign fail.
2. **DATEX XML parse_failed** — severity/validity tag mismatch; `datex=false` on PASS_UI.
3. **Ferry enumeration null/0** vs ferry instructions present (Rodby–Puttgarden).
3b. **Tunnel enumeration 0** on MWM plan vs OSM-along-route **29** tunnels on the polyline (see §6).
4. **Geofabrik path wipe UI** — `delete_region_fail` / missing `field_geofabrik_path` (non-fatal; MWM corridor still Indexed).
5. **Eco:** UI eco ON; finished plan log `eco=false`.
6. **Rule 14** (`logs/rule14_violation.txt`): `am instrument` start/finish caused AM “Force stopping no.navi.app” (runner lifecycle). Harness did not call `am force-stop`. Pre-run install can also close the package.
7. **Rule 12:** CAT `am start` / UI beyond GPS+DATEX (see §13).
8. Host note: `host_exited_before_gps` then resume-replay still produced GPS/DATEX artifacts.

---

## 15. Comparison vs `geojson-routes/bad-luster.kml` / `.json`

| | Value |
|---|---|
| Reference LineString (`bad-luster.json`) | **1440.985 km**, 13679 pts, start≈Bevensen end≈Dalsøren |
| This plan (via Vågå) | **1425.907 km** |
| Delta vs reference | **−15.078 km** |
| vs no-via band floor 1461.3 | **−35.393 km** (informational; via-shortened corridor **accepted**) |

Hop ends (~every 100 km) vs reference polyline (coarse nearest):

| along_km | lat | lon | approx. deviation_km |
|---|---|---|---|
| 0.0 | 53.07969 | 10.58720 | 0.00 |
| 100.3 | 53.76085 | 10.38048 | 0.49 |
| 200.1 | 54.41597 | 11.12208 | 0.00 |
| 300.0 | 55.04735 | 11.98514 | 0.00 |
| 400.4 | 55.60985 | 12.72944 | 0.00 |
| 500.0 | 56.15062 | 12.84443 | 0.55 |
| 600.1 | 56.93036 | 12.51929 | 0.22 |
| 700.0 | 57.73271 | 11.98470 | 0.12 |
| 800.0 | 58.50710 | 11.56049 | 0.27 |
| 900.0 | 59.29364 | 11.07192 | 0.14 |
| 1000.0 | 59.97077 | 10.98978 | 0.18 |
| 1100.1 | 60.76265 | 11.18617 | 0.17 |
| 1200.0 | 61.38059 | 10.26862 | 0.08 |
| 1300.2 | 61.87385 | 9.12505 | 0.13 (near Vågå via) |
| 1400.1 | 61.49949 | 7.76177 | 0.09 |
| 1424.3 | 61.44338 | 7.46140 | 0.06 |

Geometry tracks the reference corridor closely. Total length is slightly under the JSON reference and under the historical no-via band floor; with the **Vågå via** that shorter total is **accepted**. Polyline artifact: `report/route-polyline.txt` (~918253 chars).

---

## 16. Clock / via / ascent

- Clock policy: `logs/clock_policy.txt` — `CLOCK_SET=2026-06-01T08:00:00 local`; guest before plan and at instrument = Mon Jun 1 08:00:00 CEST 2026; replay clock pinned.
- Via set in UI: `61.8691419, 9.1055130`.
- Ascent / descent (plan report): ascent_m≈15059, descent_m≈15080; build_s≈8.55.

---

## 17. Acceptance

**PASS (accepted)** under via Vågå + June open-roads. Distance below the no-via band is expected with the via and is not treated as a fail. Version bump / tag / release remain a separate product decision; this write-up documents the accepted campaign result under `docs/test-results/`.
