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
