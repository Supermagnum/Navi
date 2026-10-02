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
