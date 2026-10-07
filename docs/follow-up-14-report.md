# Follow-up 14 report

Branch: `wip/fu3-plan-diag-index-snapshot`  
Sources: `geojson-routes/bevensen-vagaa-dalsoren-fu14-datex-none-clean-*`, `geojson-routes/bevensen-vagaa-dalsoren-fu11-datex-none-*`, `.tmp-fu14-*`, `.tmp-fu14-skeletons/`, `.tmp-place_index.db.fu14-host-bak`, FU14 agent interim (hop 4 / wall time).  
No Stage B. No new plan in Follow-up 15.

---

## a) Hop 4 water edge vs identical total km

### Why total km matches fu11

Distance is edge-length geometry, not ferry ETA. Hop 4 already used the same Puttgarden–Rødby water chord endpoints and the same hop length before and after the ferry-duration fix. The often-cited **13.919 km** figure is **one sparse shape segment inside** the ferry way (seg2: `54.50709,11.23183` → `54.62456,11.30643`), not a separate graph edge. Host probe of `.tmp-fu14-packs` finds **no** standalone ~13.9 km belt edge. The full ferry way is ~18.9 km; fu11 and fu14 hop 4 both sum to **122.471 km**, so the plan total stays **1688.345 km**.

| | FU11 DATEX-none | FU14 clean |
|---|---|---|
| Plan total | 1688.345 km | 1688.345 km |
| Hop 4 km | 122.471 | 122.471 |
| Hop 4 ETA | 109.6 min | 141.9 min |
| Hop 4 `route_ferry_legs` | 1 | 1 |

### Water edge used (hop 4)

| Field | Value |
|---|---|
| Source | **Pack** ferry (same geometry also present in ferry-overlay sidecar): Rødby (DK) – Puttgarden (D) |
| Endpoints | `54.50282,11.22822` → `54.65431,11.35081` |
| Length | **18935.3 m** (~18.9 km) |
| Shape points | **8** (6 intermediate + endpoints) |
| `is_ferry` | **true** |
| Duration tag | OSM **0:45** crossing; `base_weight` implies 45 min + 10 min boarding (`FERRY_CAR_BOARDING_PENALTY_MIN`) |

Overlay probe (`.tmp-fu14-ferry-probe.txt`) also lists matching ~18862–18935 m Fehmarn edges on `schleswig-holstein-latest` and `denmark-latest` sidecars.

### Hop 4 ETA 141.9 min breakdown

Same model as `ferry_eta_uses_overlay_duration_plus_boarding`:

| Part | Minutes |
|---|---|
| Driving (rest of hop) | **86.9** |
| Ferry crossing | **45.0** |
| Boarding | **10.0** |
| **Total** | **141.9** |

vs fu11 hop 4 **109.6** (+32.3 min): same path length; heavier ferry costing (45+10), not a length swap.

---

## b) Wall time 1122.9 s vs fu11 118.3 s

| | FU14 clean | FU11 DATEX-none |
|---|---|---|
| `planning_done` | **1122924 ms (1122.9 s)** (logcat `NaviRouting`, 10-07 04:43:27) | **118301 ms (118.3 s)** (FU14 interim / campaign figure) |
| Instrumented sum (pack+snap+search+DATEX) | **882.6 s** | **88.1 s** |
| Unaccounted (multiday / UI / etc.) | **~240 s** | **~30 s** |

Sidecar / overlay load has **no separate timer**; it is inside `pack_load_ms`. DATEX bind = **0** every hop on both plans (`datex_plan_mode=none`).

### Per-hop (ms): pack / snap / search

| hop | fu11 pack / snap / search | fu14 pack / snap / search |
|---:|---|---|
| 1 | 5463 / 1748 / 232 | 55912 / 24641 / 1559 |
| 2 | 6194 / 2055 / 910 | 73401 / 18482 / 5105 |
| 3 | 2904 / 658 / 340 | 25537 / 6335 / 2207 |
| 4 | 4457 / 1027 / 394 | 31709 / 7696 / 2626 |
| 5 | 6011 / 2185 / 303 | 71345 / 25981 / 1685 |
| 6 | 4938 / 2017 / 1220 | 60587 / 24548 / 6397 |
| 7 | 3339 / 863 / 277 | 41407 / 7601 / 1595 |
| 8 | 2317 / 372 / 293 | 19541 / 3825 / 1775 |
| 9 | 4275 / 1341 / 558 | 48544 / 13857 / 3385 |
| 10 | 4343 / 564 / 114 | 24685 / 5244 / 582 |
| 11 | 3645 / 1016 / 237 | 40914 / 8723 / 1335 |
| 12 | 4784 / 2121 / 294 | 54624 / 23918 / 1820 |
| 13 | 3465 / 1426 / 687 | 47439 / 14157 / 4139 |
| 14 | 2222 / 741 / 232 | 27845 / 6343 / 1346 |
| 15 | 1588 / 417 / 108 | 13173 / 3899 / 655 |
| 16 | 1717 / 108 / 43 | 6427 / 1389 / 379 |
| 17 | 1424 / 61 / 11 | 5053 / 1035 / 233 |
| **sum** | **63.1 / 18.7 / 6.3 s** | **648.1 / 197.7 / 36.8 s** |

**Cause named in FU14 interim:** ferry-sidecar merge work folded into every hop’s `pack_load_ms`, plus heavier snap on larger merged graphs (~10.3× pack, ~10.6× snap, ~5.9× search). DATEX bind did not contribute. No concurrent skeleton/pack-pull during the clean plan window (~04:25–04:43); those logs start after 04:43.

---

## c) Spikes (unrepaired FU14 clean path) vs ORS

Export: `geojson-routes/bevensen-vagaa-dalsoren-fu14-datex-none-clean-spike-ors.json`

| | Navi (polyline / app) | ORS |
|---|---|---|
| Distance | app 1688.345 / poly 1686.466 km | 1440.985 km |
| Gap app vs ORS | **+247.36 km** | |

### Per-country km

| | Navi | ORS | Delta |
|---|---:|---:|---:|
| DE | 328.00 | 241.09 | +86.92 |
| DK | 183.23 | 161.65 | +21.58 |
| SE | 570.65 | 468.22 | +102.43 |
| NO | 604.58 | 570.03 | +34.55 |

### Spike windows (path/chord ≥ ~2.5, extra ≥ ~1.5 km)

| km along route | lat, lon | path km | chord km | extra km | ratio | on ORS corridor |
|---:|---|---:|---:|---:|---:|---|
| 287.0 | 54.52297, 11.05102 | 20.02 | 4.64 | 15.38 | 4.32 | false |
| 408.0 | 55.16737, 11.71811 | 20.01 | 6.69 | 13.32 | 2.99 | false |
| 815.2 | 57.6063, 12.25005 | 20.03 | 5.58 | 14.45 | 3.59 | false |
| 916.0 | 58.24466, 11.95129 | 20.22 | 7.06 | 13.15 | 2.86 | true |
| 989.0 | 58.41201, 11.27137 | 20.03 | 7.30 | 12.73 | 2.74 | false |
| 1330.9 | 60.67754, 10.55937 | 20.01 | 6.31 | 13.69 | 3.17 | false |

Spike extra sum: **82.72 km**. (Road names per spike window were not stored in the spike export.)

Landmark hits on Navi unrepaired path: Puttgarden, Rødby, E47 Zealand, Öresund E20, E6 Gothenburg, Otta, Vågå, Lom, Dalsøren; **not** A1 near Lübeck.

---

## d) Coarse Bevensen → Vågåvegen 80 → Dalsøren

Source: `.tmp-fu14-coarse-results.json` → `bevensen`  
Note in artifact: `undirected coarse merge of persistent skeletons (border_osm empty at build)`.

| | Coarse | ORS |
|---|---:|---:|
| Total km | **1532.564** | **1440.985** |
| Search time | 0.513 s | — |
| Peak RSS (process) | 272.8 MB | — |
| Graph nodes (merged skel) | 260852 | — |
| Ferry legs | 1 (5.249 km) | — |

### Per-country km (coarse vs ORS)

| | Coarse | ORS | Delta |
|---|---:|---:|---:|
| DE | 203.02 | 241.09 | −38.07 |
| DK | 384.85 | 161.65 | +223.20 |
| SE | 409.80 | 468.22 | −58.42 |
| NO | 534.90 | 570.03 | −35.13 |
| **Total** | **1532.564** | **1440.985** | **+91.58** |

### “Prefers Öresund over Fehmarn” — exact route

Coarse path **does not** use Puttgarden–Rødby (`Puttgarden=false`, `Rodby=false`, `E47_Zealand=false`, `Oresund_E20=false`).

It uses the **Helsingør–Helsingborg** ferry (Scandlines HH):

| Field | Value |
|---|---|
| From | 56.032946, 12.616005 |
| To | 56.043311, 12.691520 |
| Length | 5.249 km |
| Region in artifact | `europe/denmark` |

Joints place the Denmark spine through **Jutland** then **Funen / Storebælt** then **Zealand**, then the HH ferry into Sweden, then **E6** north (`E6_Gothenburg=true`).

### Does it avoid Puttgarden–Rødby via Jutland + Storebælt?

**Yes**, by landmarks and joints (high DK km vs ORS Fehmarn shortcut).

### Coarse cost of Fehmarn alternative vs chosen path

**Not present in FU14 artifacts.** `.tmp-fu14-coarse-results.json` records only the chosen path. No second coarse search cost (km / minutes / ferry duration+boarding) for a forced Fehmarn alternative was exported. Cannot invent those numbers.

Chosen ferry cost available: HH 5.249 km; duration tag + boarding for that edge were **not** recorded in the coarse JSON (only length).

### Roads in order per country

OSM `ref` / name sequence along the coarse path was **not exported**. Verified corridor signals only:

| Country | What sources support |
|---|---|
| DE | Not A1/Lübeck. Start Bevensen → joint ~54.27,9.78 (SH west / E45 approach to DK). |
| DK | Jutland (~55.43,9.46) → Funen/Storebælt area (~55.35,10.61) → Zealand (~55.65,11.76) → Helsingør–Helsingborg ferry. Not E47 after Fehmarn. |
| SE | E6 corridor (`E6_Gothenburg=true`). |
| NO | Otta, Vågå, Lom, Rv15 Ottadal, Rv55 Sognefjellet landmarks true (see e). |

### Joints (coordinates and type)

| lat | lon | type |
|---:|---:|---|
| 53.079367 | 10.531892 | start |
| 54.27225 | 9.78081 | hop_deg |
| 55.42712 | 9.46011 | hop_deg |
| 55.3472 | 10.61155 | hop_deg |
| 55.64668 | 11.76177 | hop_deg |
| 56.04331 | 12.69152 | ferry (HH arrival) |
| 57.2199 | 12.23613 | hop_deg |
| 58.37744 | 11.77752 | hop_deg |
| 59.53528 | 10.72799 | hop_deg |
| 60.68598 | 10.61012 | hop_deg |
| 61.78985 | 9.45791 | hop_deg |
| 61.70665 | 8.29863 | hop_deg |
| 61.44365 | 7.46052 | end |

---

## e) Otta → Rv15 → Vågå → Lom → Rv55?

From coarse landmarks in `.tmp-fu14-coarse-results.json`:

| Landmark | Hit |
|---|---|
| Otta | true |
| Vågå | true |
| Lom | true |
| Rv15_Ottadal | true |
| Rv55_Sognefjellet | true |
| Dalsøren | true |

**Yes** — coarse flags leave E6 at Otta onto Rv15 Ottadalsvegen, pass the Vågå via, continue to Lom, and take Rv55 toward Luster/Dalsøren. (No per-edge ref dump beyond these landmark flags.)

---

## f) Aga coarse ferry

Source: `.tmp-fu14-coarse-results.json` → `aga`  
Total 390.425 km; ferry_legs=1; ferry_km=5.658.

| Field | Value |
|---|---|
| From | 60.4718109, 6.6123366 |
| To | 60.4240901, 6.6217554 |
| Length | 5.658 km |
| Region | `europe/norway/vestlandet` |

Landmark flags: `Kinsarvik=true`, `Utne=true`.

**Kinsarvik–Utne?** **No** (by terminal coordinates).  
South terminal matches Utne (~60.424, 6.622). North terminal (~60.472, 6.612) is **not** the usual Kinsarvik terminal (~60.375, 6.72; ~11–12 km away). Path proximity flags both landmarks, but the ferry edge terminals are not the Kinsarvik–Utne pair. FU14 did not store an OSM ferry name string on this edge (`hw` empty).

---

## g) Skeleton per region

From `.tmp-fu14-skel-*.txt` / `.tmp-fu14-skeletons/`.  
`build_ms` = skeleton build; `load_ms` = pack load before build; `peak_rss_mb` = process RSS (later regions may show cumulative RSS with `delta_rss_mb=0`).

| region | nodes | edges | file bytes | build_ms | peak_rss_mb |
|---|---:|---:|---:|---:|---:|
| hamburg | 7146 | 9701 | 884833 | 11 | 301 |
| schleswig-holstein | 19051 | 32369 | 2802706 | 47 | 1122 |
| mecklenburg-vorpommern | 16233 | 28893 | 2485360 | 34 | 1121 |
| niedersachsen | 60453 | 100590 | 8837638 | 165 | 3952 |
| denmark | 31035 | 50056 | 4428850 | 138 | 4023 |
| skane | 7952 | 11120 | 1020877 | 19 | 542 |
| halland | 2374 | 3129 | 289109 | 7 | 256 |
| vastra_gotaland | 12185 | 15999 | 1502033 | 32 | 1147 |
| ostlandet | 64383 | 115278 | 9906658 | 444 | 4021 |
| vestlandet | 41298 | 76513 | 6497604 | 59 | 1421 |

---

## h) Is commit `08e696aa` still needed once tiles come from the coarse path?

Commit `08e696aa` (*Stop after one trip-AABB dest-hop reload and fill tiles to budget*): corridor-band + chord samples can drop off-chord valley tiles; one trip-AABB reload with fill-to-budget replaces repeated pad widenings.

**Still needed as a safety net**, not as the primary tile source, once Stage B densify uses coarse-path tiles:

- Coarse joints improve which corridor is requested, but hop graphs can still disconnect (border gaps, missing secondary, clip bugs).
- `should_fallback_to_trip_aabb` remains the documented escape when corridor band stays disconnected.
- Without it, a single bad band can fail the dest hop instead of one bounded AABB reload.

It should become **rarer**, not obsolete, when coarse-driven tiles are correct.

---

## FU14 commits (local, not pushed) relevant to Stage A / clean plan

Including later Follow-up 15 gate commit:

- `e67e6da4` Fix ferry ETA from overlay duration and discard stale hop resume  
- `aeb2dcd6` Unit test for long-trip pack fingerprint resume key  
- `bbc172fe` Harden place-index migrate restore and start corridor skeleton Stage A  
- `eb68ed57` Fix place-index gate after PK migrate rename (Follow-up 15)

---

## Protected index counts (host bak, readonly)

File: `.tmp-place_index.db.fu14-host-bak` (3379351552 bytes, schema v6)

| region_id | COUNT(*) |
|---|---:|
| europe/germany/hamburg | **1719762** |
| europe/germany/niedersachsen | **2903125** |
| europe/norway/ostlandet | **1483135** |
| europe/denmark | **2851971** |

---

## Gaps left for decisions (see Follow-up 15 Steps 2–3)

1. Stage B approval (depends on this report).  
2. Sweden option A implementation details / approval to change rows.  
3. Hamburg H1 vs H2 (leave alone until answered).  
4. Host disk cleanup choices (inventory only; nothing deleted).  
5. Coarse Fehmarn alternative costing was not exported in FU14 — re-run only if approved.
