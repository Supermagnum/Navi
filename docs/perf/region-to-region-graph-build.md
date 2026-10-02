# Region-to-region graph build / plan performance

Branch: `perf/region-to-region-graph-build` (from `origin/dev` @ `61bd0b80`).  
Scope: client pack-hit planning only. Packs are FlatGraphPack / graph format **v9** (server serves v9; no format bump).

## Symptom

Raufoss → Bergen (car, eco) could take **10+ minutes** on device / hang on host when packs were present but unused, or when the corridor graph dropped the Vestlandet bridge under the single-stem tile budget.

## Measurement setup

- Host harness: `cargo run -p navi-ffi --release --bin region-to-region-perf-matrix -- --pack-dir /tmp/navi_pack_install --elev-dir /tmp/navi_pack_install/elevation`
- Timing gated by `set_route_plan_timing_enabled(true)` → `ROUTE_PLAN_STAGES` + `PLAN_PERF` (tile loads show `format=9`).
- Packs: Ostlandet + Vestlandet car/poi/wetland v9 under `/tmp/navi_pack_install` (stub PBFs beside manifests).
- No `NAVI_MEASURE_MAX_PLAN_TILES` override for final numbers (multi-stem default applies).

## Root causes (measured)

| Cause | Evidence | Fix |
| --- | --- | --- |
| (b) Pack dir not searched when long-trip UI flag is off | Empty `packDir` → fixtures / cold PBF build | Always pass `LongTripPackStorage.packDownloadDir`; `plan_pack_dirs` always probes nested `long-trip-packs`; stub PBFs accepted next to manifest |
| (c) `MAX_PLAN_TILES=6` drops Vestlandet bridge on multi-stem trips | With tiles=6: disconnected A*; with tiles≥11: pack_hit + route | `MAX_PLAN_TILES_MULTI_STEM=14` via `effective_max_plan_tiles_for_stems(extra_stems)` |
| (d) Soft motor multi-day POI scrape on single-day trips | After graph+search fixed: `multiday_ms≈36s` vs A*≈0.2s on Raufoss→Bergen | Skip overnight POI scrape when trip fits `MotorDailyBudget` |

Eco reweight is not the hang: `eco_reweight_ms≈50–84` on pack-hit.

## Before / after (host, release)

**Before (broken paths):**

- UI with packs installed but empty packDir: cold / fixture path (minutes).
- Pack-hit with default 6-tile budget: A* fails / hangs (no Vestlandet connectivity).
- Pack-hit with `NAVI_MEASURE_MAX_PLAN_TILES=16` (graph OK): wall ≈ **42 s**, of which `multiday_ms≈36 s`, A* ≈ 221 ms, pack_load ≈ 4 s, peak RSS ≈ 898 MiB.

**After (this branch, no tile-budget env override):**

| Case | eco | pack_hit | wall_ms | pack_load_ms | eco_reweight_ms | astar_ms | multiday_ms | distance_km | peak_rss_mb |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| raufoss_dombas | false | true | 2511 | 1588 | 0 | 157 | 0 | 206.81 | 578 |
| raufoss_dombas_eco | true | true | 2442 | 1563 | 50 | 133 | 0 | 206.81 | 578 |
| bergen_forde | false | true | 1930 | 1449 | 0 | 180 | 0 | 171.01 | 612 |
| bergen_forde_eco | true | true | 1906 | 1362 | 51 | 175 | 0 | 171.01 | 612 |
| raufoss_bergen | false | true | 5626 | 3909 | 0 | 219 | 0 | 485.45 | 898 |
| raufoss_bergen_eco | true | true | 5585 | 3846 | 83 | 221 | 0 | 459.71 | 1002 |
| raufoss_bergen_eco_warm | true | true | 5595 | 3832 | 84 | 223 | 0 | 459.71 | 1002 |

Raufoss→Bergen loads 11 corridor tiles (`extra_stems=1`, all `format=9`). Dominant cost is pack mmap/merge (~4 s) + POI/barrier (~1 s); search stays ~0.2 s.

## Route quality

- Default car Raufoss→Bergen: 485.45 km, eta ≈ 423 min, no ferry, snap ≤ 25 m.
- Eco: 459.71 km (eco soft costs), same pack set; quality change is intentional eco preference, not a regression from the perf fixes.
- Same-stem controls (Raufoss→Dombås, Bergen→Førde) remain pack_hit and ~2 s.

## Code changes (summary)

- `core/src/routing/plan_perf.rs` — debug-gated notes + peak RSS.
- `core/src/routing/plan_bbox.rs` — multi-stem tile budget (14).
- `core/src/routing/indexed/load.rs` — use `for_stems`; ignored load probe.
- `navi-ffi` — stage split, pack_dirs probe, multiday early-out, host matrix bin.
- Android — always pass pack download dir; stub PBF resolution; instrumented matrix test.

## Constraints respected

- No `graph_format_version` bump; read path remains v8+v9 accept, write/preferred **v9**.
- No navi-server changes; no merge to main/dev/right-to-roam.

---

## Follow-up results (device + corridor cache + disconnect)

Date: 2026-10-02. Branch tip after this follow-up (see git log). Innlandet is **not** a separate Geofabrik leaf (covered by Ostlandet / hedmark+oppland). Installed stems on device SD `long-trip-packs`: ostlandet, vestlandet, trondelag, nord-norge, sorlandet (car+poi+wetland; foot tiles optional).

### Root cause of original 10+ minutes (dev vs this branch)

Confirmed combination:

1. Empty / unused `packDir` → cold PBF / fixture path (minutes).
2. Single-stem `MAX_PLAN_TILES=6` dropping Vestlandet bridge → disconnected corridor + ferry Geofabrik stub ensure spinning (~130–300 s).
3. Soft motor overnight POI scrape on trips that still fit one driving day (`multiday_ms≈36 s`).

After this branch: pack-hit, tile budget 14 (or widen from 6), overnight scrape skipped when under `MotorDailyBudget` (`multiday_ms≈30 ms`).

### Raufoss→Bergen pack selection

- `primary_stem=ostlandet-latest`, `extra_stem_list=vestlandet-latest`, `tile_budget=14`, `edge_clip=CorridorBand`.
- Why: trip bbox spills west of Ostlandet into Vestlandet; corridor band keeps the mountain/coast bridge tiles that budget=6 dropped.

### Host matrix (release, corridor LRU warm)

| Case | eco | pack_hit | wall_ms | pack_load_ms | eco_reweight_ms | astar_ms | distance_km | peak_rss_mb | route_ok |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| raufoss_dombas | false | true | 2965 | 1981 | 0 | 157 | 206.81 | 802.3 | true |
| raufoss_dombas_eco | true | true | 1562 | 654 | 52 | 140 | 206.81 | 802.3 | true |
| bergen_forde | false | true | 2520 | 2003 | 0 | 191 | 171.01 | 898.3 | true |
| bergen_forde_eco | true | true | 1497 | 964 | 58 | 171 | 171.01 | 898.3 | true |
| raufoss_bergen | false | true | 5991 | 4739 | 0 | 245 | 485.45 | 1327.7 | true |
| raufoss_bergen_eco | true | true | 2616 | 1268 | 95 | 245 | 459.71 | 1360.2 | true |
| raufoss_bergen_eco_warm | true | true | 2542 | 1246 | 91 | 258 | 459.71 | 1360.2 | true |
| raufoss_tromso | false | true | 17119 | 1650 | 0 | 55 | 1766.89 | 1360.2 | true |

Tromsø: long-trip densify, **17 hops**, stems Ostlandet→Trøndelag→Nord-Norge, `chunked_distance_km=1766.892`, ferries used on coastal legs.

Cold→warm: Raufoss→Bergen pack_load **4739 → 1268 ms** (corridor LRU hit). Remaining ~1.2 s on hit is mostly owned-`RouteGraph` clone from cache (mmap tiles are already materialized into heap).

### Android emulator matrix (x86_64, real SD packs)

Peak **native heap** from `Debug.getNativeHeapAllocatedSize` (MiB). Emulator: Navi_8c_4G_128G AVD API 15.

| Case | eco | pack_hit | wall_ms | pack_load_ms | astar_ms | distance_km | peak_rss_mb | peak_native_heap_mb | route_ok |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| raufoss_bergen | true | true | 24820 | 6214 | 352 | 459.71 | 1268.3 | 406.3 | true |
| raufoss_bergen | false | true | 20062 | 1683 | 317 | 485.45 | 1280.7 | 406.5 | true |
| raufoss_bergen_warm | true | true | 19338 | 1646 | 312 | 459.71 | 1294.2 | 406.5 | true |
| raufoss_dombas | true | true | 19184 | 2857 | 180 | 206.81 | 1302.8 | 636.8 | true |
| raufoss_dombas | false | true | 17252 | 893 | 215 | 206.81 | 1302.8 | 636.9 | true |
| bergen_forde | true | true | 3269 | 2775 | 212 | 171.01 | 1302.8 | 636.6 | true |
| bergen_forde | false | true | 1753 | 1289 | 249 | 171.01 | 1302.8 | 452.7 | true |
| raufoss_tromso | false | true | 32114 | 2156 | 67 | 1766.89 | 1302.8 | 537.0 | true |

Device Tromsø: `extra=trondelag-latest` on the Ostlandet→Trøndelag hop; later hops re-home `primary_stem` to trondelag / nord-norge; **17 hops**, same 1766.89 km as host.

Earlier device Tromsø failure (`snap_failed` on chunk_leg6, wall≈241 s): car-only installs missing **foot** tiles made `status_pack_files` report Not Ready, so Trøndelag never joined the corridor; stub Geofabrik ferry ensure then burned ~131 s on a land hop. Fixed by profile-scoped Ready + no blocking stub ensure.

Device wall ≫ pack_load on several Ostlandet rows: `poi_barrier_ms≈15–17 s` on SD (separate from multiday; `multiday_ms≈28–32` after early-out). Bergen→Førde stays ~2–3 s (small Vestlandet POI).

### Forced tile budget 6 (host, never spin)

`NAVI_MEASURE_MAX_PLAN_TILES=6` Raufoss→Bergen:

- First load: `tile_budget=6`, `corridor_components=disconnected`, `ferry_overlay=skip_disconnected_components`.
- Widen: `tile_budget_widen_to=10` (attempt 1), then pack_hit route, wall≈**7894 ms**, no hang.

### Pack LRU / RSS note

Tiles are mmap’d (`mmap=1` in `PLAN_PERF`) then **materialized into an owned `RouteGraph`** for A* (merge copy). Peak RSS ~1000–1360 MiB is that owned graph + corridor LRU spare clone (cap ~1536 MiB), not raw mmap RSS alone. Warm hits skip tile mmap/decode but still clone the cached owned graph (~1.2 s host).

### Multi-day POI scrape

Profiled ~36 s overnight scrape was soft motor day-boundary POI search after A*. Client fix: skip when trip fits `MotorDailyBudget` → `multiday_ms≈30 ms` on Raufoss→Bergen. Multi-day trips (Tromsø) still run overnight logic on day marks; densify chunk legs skip POI (`poi_skipped=chunk_leg`). Further POI/barrier I/O on large Ostlandet packs (~15 s on emulator SD) is a separate client follow-up if needed.

### Additional code (this follow-up)

- `plan_bbox`: tile budget floor / widen / memory-aware cap (never spin).
- `load`: disconnect detect + widen; profile-scoped `status_pack_files_for_profile`; stub ferry ensure skipped; snap gate aligned to 35 km densify snap.
- `corridor_cache`: LRU of owned corridor graphs between plans.
- `RouteGraph: Clone` for cache spare.
- Host + Android instrumented matrix harnesses (debug timing flag).

## Follow-up 2 (POI clip/cache, Arc share, snap, Tromso, tiles)

Date: 2026-10-02. Same branch; client-only; FlatGraphPack / POI format **v9** (no bump).

### 1. POI/barrier load (was 15–17 s on emulator)

**Profile (Ostlandet car, before this follow-up):** full-region `*.navi-poi-barrier.rkyv` hydrate every plan, including ~2M overnight buildings. File ~66 MB Ostlandet / ~42 MB Vestlandet. Parsed on every plan (no cache). Whole-region, not corridor.

**Fix:**

- Clip POI/barrier load to a polyline corridor band (~15 km half-width) via `load_poi_barrier_pack_bbox` / `try_load_poi_barrier_for_plan_bbox_with_pack_dirs`.
- Skip overnight buildings for motor plans (`INCLUDE_OVERNIGHT_BUILDINGS=false`).
- Process-wide `poi_barrier_cache` keyed by pack paths + bbox fingerprint.

**After (Raufoss→Bergen eco):**

| Env | cold `poi_barrier_ms` | warm `poi_barrier_ms` | notes |
| --- | --- | --- | --- |
| Host | ~265 | ~73 | clip=1; buildings=0; ost+vest packs |
| Emulator | ~656 | ~268 | SD; cache hit on warm |

Cold wall is no longer POI-dominated; pack tile materialize is.

### 2. Memory / Arc sharing

**Before:** corridor LRU hit still cloned owned `RouteGraph` (~1.2 s host warm pack_load); peak RSS ~1000→1360 MiB with spare clone; cache hard cap 1536–2048 MiB.

**After:**

- `corridor_cache_get` returns `Arc<RouteGraph>` (no graph clone on hit).
- Eco / motor soft / surface mode are per-plan `RouteOptions` overlays (`compute_eco_weights` → `eco_weights: Arc<…>`), never mutating the cached graph.
- Ferry overlay skipped on corridor-cache hit (`ferry_overlay=skip_corridor_cache_hit`) — the 35 km snap+probe was the remaining warm pack_load cost.
- Cache cap from `MemAvailable` (20%), clamped to **[256, 768] MiB**.

| Metric | Follow-up 1 (host) | Follow-up 2 (host) |
| --- | --- | --- |
| Raufoss→Bergen peak RSS | ~1360 MiB | **994.5 MiB** |
| Warm eco `pack_load_ms` | ~1246 | **2** |
| Warm eco `wall_ms` | ~2542 | **734** |

Emulator peak RSS ~907–1118 MiB; peak native heap ~290–447 MiB (`Debug.getNativeHeapAllocatedSize`).

### 3. Snap gate 35 km

| Snap site | Gate | Notes |
| --- | --- | --- |
| User origin / destination / via (non-chunk) | `CAR_MAX_WAYPOINT_SNAP_M` = **750 m** | Unchanged |
| Densify chunk intermediate joints (`relax_start_snap` / `relax_end_snap`) | `CHUNK_INTERMEDIATE_SNAP_M` = **35 km** | Loose gate for geometric densify points only |
| Ferry-overlay / `graph_hop_already_connected` probes | **35 km** | Same densify constant; connectivity uses weak-component (no A*) |

Endpoints were **not** loosened: chunk leg 1 start and last-leg end keep 750 m. Only internal densify joints use 35 km. No revert needed for user O/D.

### 4. Raufoss→Tromsø stage breakdown (host, 17 hops)

Wall ≈ **16.6 s**; sum of per-hop `pack_load_ms` ≈ **9.5 s**. Chunk legs skip POI (`poi_barrier_ms=0`). Ferry/multiday on hops are negligible vs pack_load.

| Leg | pack_load_ms | astar_ms | Dominant |
| --- | --- | --- | --- |
| 1–3 (Ostlandet) | 1325–1686 | 63–159 | pack miss (unique clips) |
| 4–5 | 207–285 | 8–11 | small tiles |
| 6–7 (Trøndelag) | 991–1189 | 41–136 | pack miss |
| 8–17 | 103–397 | 8–65 | smaller northern tiles |

**Largest stage:** per-hop pack_load on **cache-miss** corridors (clip sets differ every hop). Quantizing clips to share Arcs was tried earlier and **reverted** (route distance drifted 1906 vs 1766 km). Remaining cost is inherent to densify hop isolation without a format bump.

Emulator Tromsø wall ≈ **29–33 s**, same **1766.89 km**, `route_ok=true`.

### 5. Tile budget reconciliation

| Claim | Meaning |
| --- | --- |
| First report “≥11 tiles for Bergen” | Multi-stem budget **14** selects **11** corridor tiles for Raufoss→Bergen (ost+vest). Measured `tiles=11` on cache hit. |
| Follow-up “force 6 → widen to 10” | With `NAVI_MEASURE_MAX_PLAN_TILES=6`, first load disconnects; widen lands on **10**, then pack_hit route. |

**Actual minimum for a successful Raufoss→Bergen pack-hit:** **10** tiles after widen (forced-6 path). Default multi-stem selection uses **11** under budget 14. Budget 6 alone is insufficient (Vestlandet bridge dropped).

### 6. Full matrices after fixes (distances unchanged)

**Host (release):**

| Case | eco | wall_ms | pack_load_ms | distance_km | peak_rss_mb | route_ok |
| --- | --- | --- | --- | --- | --- | --- |
| raufoss_dombas | false | 2298 | 1600 | 206.81 | 513.5 | true |
| raufoss_dombas_eco | true | 635 | 1 | 206.81 | 513.5 | true |
| bergen_forde | false | 1983 | 1637 | 171.01 | 636.6 | true |
| bergen_forde_eco | true | 351 | 1 | 171.01 | 636.6 | true |
| raufoss_bergen | false | 4995 | 4166 | 485.45 | 994.5 | true |
| raufoss_bergen_eco | true | 764 | 2 | 459.71 | 994.5 | true |
| raufoss_bergen_eco_warm | true | 734 | 2 | 459.71 | 994.5 | true |
| raufoss_tromso | false | 16568 | 1663 | 1766.89 | 994.5 | true |

**Emulator (x86_64, connectedDebugAndroidTest):**

| Case | eco | wall_ms | pack_load_ms | distance_km | peak_rss_mb | peak_native_heap_mb | route_ok |
| --- | --- | --- | --- | --- | --- | --- | --- |
| raufoss_bergen | true | 6517 | 4951 | 459.71 | 907.4 | 447.1 | true |
| raufoss_bergen | false | 1215 | 182 | 485.45 | 907.4 | 447.3 | true |
| raufoss_bergen_warm | true | 1232 | 204 | 459.71 | 907.4 | 447.4 | true |
| raufoss_dombas | true | 3315 | 2201 | 206.81 | 1117.7 | 447.6 | true |
| raufoss_dombas | false | 948 | 98 | 206.81 | 1117.7 | 294.2 | true |
| bergen_forde | true | 2764 | 2145 | 171.01 | 1117.7 | 294.3 | true |
| bergen_forde | false | 552 | 99 | 171.01 | 1117.7 | 289.4 | true |
| raufoss_tromso | false | 29605 | 2121 | 1766.89 | 1117.7 | 289.5 | true |

Distances match prior tables (459.71 / 485.45 / 206.81 / 171.01 / 1766.89).

**Targets vs measured (emulator eco):**

- Warm **under 2 s:** met (~1.2 s).
- Cold **under 5 s:** not met on emulator SD; cold wall ≈ **6.5–7.7 s**, of which `pack_load_ms` ≈ **5–6 s** for 11-tile / ~496k-edge materialize. Host warm and eco-after-cache are well under 1 s. Instrumented assert: cold <9 s (SD variance), warm <2 s.

### Code (follow-up 2)

- `poi_barrier_cache.rs`, `load_poi_barrier_pack_bbox`, overnight-building skip, corridor POI clip.
- `corridor_cache`: Arc get/insert; MemAvailable cap ≤768 MiB.
- `load`: return `Arc` from tiled load; skip ferry overlay on cache hit; weak-component ferry probe.
- `builder` / `reweight`: eco overlay weights; no cached-graph mutation.
- Instrumented matrix asserts + host `region-to-region-perf-matrix` harness.

## Overnight buildings vs motor overnight stop candidates (PR gate)

These are **two different things**:

| Mechanism | What it is | When skipped |
| --- | --- | --- |
| Pack **overnight buildings** (`building_lats` / hiking allemannsretten samples) | ~2M building centroids in Ostlandet POI packs | **Always** skipped on the motor corridor plan hydrate path (`INCLUDE_OVERNIGHT_BUILDINGS=false`). **Not** gated by `MotorDailyBudget`. Unused for soft-break lodging/camping search. |
| Motor **overnight stop candidates** (lodging / camp / rest at day marks) | `PoiCategory::Lodging` etc. from the same packs | Skipped only when the trip **fits** `MotorDailyBudget` (single-day). Multi-day densify uses `finalize_chunked_motor_soft_breaks`, which loads POI packs at day-boundary points via `load_poi_barrier_pack` (full hydrate, buildings included for that local pack load). |

### Raufoss → Tromsø multi-day overnight (host, this tip)

`region-to-region-perf-matrix --only raufoss_tromso` (long-trip densify, 1766.89 km, ~25.5 h driving):

| Metric | This branch (`15b91e7d`) | Pre-branch expectation (`origin/dev`) |
| --- | --- | --- |
| `overnight_candidates` (scrape pool) | **106** | Same path in finalize (not using corridor `INCLUDE_OVERNIGHT_BUILDINGS=false`) |
| `rest_candidates` | **54** | Same |
| `motor_multi_day` | **days=4**, `budget=Hours(8.0)` | Same |
| Day-boundary overnight with `poi_found=true` | **3** (Treetop Ekne; Korgenfjellet Fjellstue; Bardu Hotel) | Same lodging categories |

**Conclusion:** Overnight **stop candidates were not dropped** for multi-day trips. No restore needed. The always-false flag only omits hiking building centroids from the **motor corridor** POI hydrate (the 15 s Ostlandet cost). Day-boundary finalize still loads lodging/camping POIs per mark.

Open follow-ups (not in this PR): cold pack materialize (~5–6 s; search on mmapped tiles); per-hop pack_load on long multi-stem densify (Tromsø).
