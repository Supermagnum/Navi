# mmap graph search (remove cold pack materialize cost)

Branch: `perf/mmap-graph-search` (from `dev` after PR #137 merge `ba37869c`).
Client only; no navi-server changes; no `graph_format_version` bump; no merge.

## Targets (tablet SM-P613, v9 packs)

| Case | Target |
| --- | --- |
| Raufoss → Bergen eco cold | wall &lt; 5 s (was ~15.8 s; pack_load ~12.5 s) |
| Raufoss → Bergen eco warm | stay &lt; 3 s |
| Raufoss → Tromsø | wall &lt; 20 s (was ~50 s) |
| Peak RSS Bergen | ≤ 954 MiB (lower if mapped search lands) |

Distances must stay exact: 459.71 / 485.45 / 206.81 / 171.01 / 1766.89 km. Tromsø: 4 days / 3 overnight stops.

## Step 1 — pack_load breakdown (before behaviour changes)

Device: SM-P613 `R52TB0JQEDE`, Android 14, arm64, MemTotal ~3.5 GiB.
Build: tip of this branch with plan-perf stage timers; `set_route_plan_timing_enabled(true)`.
When timing is on, each tile mmap is page-touched before validate/copy so I/O and CPU separate; production cold mixes page-in into copy.

### Raufoss → Bergen eco cold (11 tiles, ~496k edges)

| Metric | Value |
| --- | --- |
| wall_ms | **15639** (repeat; prior 15691) |
| pack_load_ms | **12400** |
| eco_reweight_ms | 252 |
| astar_ms | 1213 |
| distance_km | 459.71 |
| peak_rss_mb | 957 |
| MemAvailable before | ~1311 MB |
| pack_hit | true |

**Stage totals** (summed from per-tile `tile_stage` / `merge_stage` notes on the cold run; ferry = residual of `pack_load_ms`):

| Step | ms | Bytes / notes | Threads |
| --- | --- | --- | --- |
| File open / mmap | **0** | ~325 MB across 11 tile files (lazy map) | 1 |
| Page-in I/O (forced touch) | **727** | first-touch of tile file bytes | 1 |
| Decode / validation (`rkyv::access` + preamble) | **41** | archived view check | 1 |
| Per-tile copy into owned `RouteGraph` (`to_route_graph_clips`) | **1438** | clip + allocate nodes/edges/strings/shapes | 1 |
| Cross-tile merge hash / dedupe | **1861** | OSM node-id `HashMap` + edge-key set | 1 |
| Adjacency rebuild (inside each incremental `from_parts`) | **3568** | rebuilds after **every** tile merge | 1 |
| Border stitching | *(in merge hash)* | no separate stitch table; shared OSM node ids | 1 |
| Spatial / snap index build | **0** in pack_load | snap is later (`snap_ms` ~258) | — |
| Ferry overlay (PBF supplement) | **~4765** | residual; same path as other corridors (~1.5–2.8 s when shorter) | 1 |
| **Sum** | **~12400** | | **single-threaded tile loop** |

Warm eco (corridor cache hit): pack_load **6** ms, wall **2498** ms.

### What v9 forces vs client choices

**v9 / on-disk layout forces**

- Per-stem tiled `.rkyv` graph packs with rkyv archived `FlatGraphPack` (v9 adds `edge_is_tunnel`).
- Stable OSM node ids across tiles (merge is hash-by-id, not a remapping table).
- Corridor clip must scan archived edges to keep a band (no precomputed corridor slices).

**Client choices (this branch’s cost)**

- Materialize an **owned** `RouteGraph` (copy out of mmap) before A*.
- Cache **merged corridors** (clip fingerprint keyed), not individual tiles → Tromsø hop clip changes miss every time.
- **Sequential** tile load; incremental merge that **rebuilds adjacency on every tile**.
- Optional ferry overlay from on-disk PBF when packs lack usable ferry connectivity.

### Baseline matrix (same run; locale commas in log)

| case | eco | c/w | pack_hit | wall_ms | pack_load_ms | astar_ms | distance_km | peak_rss_mb | route_ok |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| raufoss_bergen | true | cold | true | 15639 | 12400 | 1213 | 459.71 | 957 | true |
| raufoss_bergen | false | warm* | true | 2242 | 6 | 1152 | 485.45 | 957 | true |
| raufoss_bergen_warm | true | warm | true | 2498 | 6 | 1211 | 459.71 | 957 | true |
| raufoss_dombas | true | cold | true | ~8.2k | ~5.5k | ~777 | 206.81 | ~1051 | true |
| bergen_forde | true | cold | true | ~7.4k | ~5.7k | ~935 | 171.01 | ~1051 | true |
| raufoss_tromso | false | cold | true | ~50k | (per-hop) | — | 1766.89 | ~1051 | true |

\*non-eco immediately after eco cold; corridor cache hit.

No `lowmemorykiller` / ANR for `no.navi.app` during these runs.

### Step 1 conclusion

~70% of Bergen cold pack_load is **owned-graph assembly**: copy (~1.4 s) + merge hash (~1.9 s) + repeated adjacency rebuild (~3.6 s) + ferry overlay (~4.8 s). Search is already ~1.2 s. Tromsø pays rematerialize every densify hop because the corridor cache keys on clip set.

Next: Step 2a per-tile cache (same edges/clips, reuse materialized tiles across hop clips).

## Step 2 — fixes (cheapest first)

### 2-pre-1 — Ferry overlay cheap skip (+ stem sidecar)

**Instrument (Bergen eco cold, before this fix on WIP):** residual was not PBF parse.
`ferry_overlay=skip_already_connected` with `ferry_snap_probe_ms≈1790` +
`ferry_skip_probe_ms≈1733` — duplicate 35 km (`CHUNK_INTERMEDIATE_SNAP_M`)
`nearest_routable` probes (first block + `graph_hop_already_connected`).

**Fix:** one snap + weak-component check. Prefer
`max_waypoint_snap_m(profile)` (car **750 m**); fall back to 35 km only when
tight snap fails (densify joints). If connected → return immediately (no second
probe). Stem ferry sidecar (`*.navi-ferry-overlay-*.rkyv`) still used when an
overlay must build (Tromsø coastal) — no PBF re-parse once cached.

| Metric | Before (WIP) | After 2-pre-1 |
| --- | --- | --- |
| ferry_ms | ~3526 | **263** |
| ferry_snap_probe_ms | ~1790 | **263** |
| ferry_skip_probe_ms | ~1733 | **0** |
| ferry_snap_m | 35000 | **750** |
| ferry_overlay | skip_already_connected | skip_already_connected |
| ferry_pbf_parse_ms | (n/a; skip path) | (n/a; skip path) |
| wall_ms (Bergen eco cold) | ~10326 | see post-pre table |
| distance_km | 459.71 | **459.71** |

Per-plan: skip/overlay runs once on corridor-cache miss; warm uses
`ferry_overlay=skip_corridor_cache_hit`. Tromsø densify joints that miss the
750 m gate correctly fall back to `ferry_snap_m=35000` (observed on several hops).

### 2-pre-2 — Single-pass adjacency

Stop rebuilding adjacency after every tile merge; append all tiles, then
`RouteGraph::from_parts` once (`pack_merge_mode=single_pass_one_adjacency`).

| Metric | Step 1 baseline | After 2-pre-2 |
| --- | --- | --- |
| merge_adj_ms (Bergen eco cold) | **3568** (per-tile rebuilds) | **766** |
| merge_hash_ms | 1861 | 425 (same run; hash also cheaper once) |
| pack_merge_wall_ms | (incremental) | **1191** |

### 2-pre-3 — Border-only dedupe

Inspected `FlatGraphPack` v9: nodes are `node_ids` + coords +
`node_access_blocked` only — **no border-node marks**.
`v9_border_marks=false`. Merge keeps full OSM-id node `HashMap` + edge-key
dedupe (`border_stitch=full_node_hash`). No format bump / invented fields.

Measured Bergen eco cold hash (with single-pass): **425 ms** (was 1861 ms at
Step 1; drop is mostly from not rebuilding adjacency between hash passes, not
from border-only dedupe).

### Post-pre Bergen eco cold breakdown (before 2a)

Device: SM-P613 `R52TB0JQEDE`, tip with 2-pre-1..3. Instrumented
`RegionToRegionPerfMatrixInstrumentedTest` (2026-10-02).

| Metric | Value |
| --- | --- |
| wall_ms | **6942** |
| pack_load_ms | **3872** |
| eco_reweight_ms | 216 |
| astar_ms | 1205 |
| distance_km | **459.71** |
| peak_rss_mb | 821.9 |
| pack_hit | true |
| tiles | 11 (~496k edges) |

| Step | ms | Notes |
| --- | --- | --- |
| File open / mmap | 2 | lazy map |
| Page-in I/O | 795 | forced touch when timing on |
| Decode / validation | 37 | |
| Per-tile copy | 1530 | clip + allocate |
| Cross-tile merge hash | **425** | full-node hash; `v9_border_marks=false` |
| Adjacency rebuild | **766** | **once** after all tiles |
| Ferry overlay | **263** | snap 750 m; skip_already_connected; skip_probe=0 |
| **pack_load sum** | **~3872** | |

Warm eco: pack_load **6** ms, wall **2447** ms. Distances: 459.71 / 485.45 /
206.81 / 171.01 / **1766.89** km. Tromsø: **days=4**, **3 overnight** lodging
stops (Treetop Ekne, Korgenfjellet Fjellstue, Bardu Hotel).

Next: **2a** per-tile cache (full tile materialize + in-memory clip across hop
clip changes), then 2b parallel tile load, 2c search on mapped tiles.

### 2a — Per-tile cache (full tile + in-memory clip)

Module `tile_cache.rs`: process-wide LRU of **full** materialized tiles keyed by
path+profile. On hit, re-clip in memory (same edge predicate as pack hydrate).

**Tablet constraint:** tile LRU soft cap ~64–90 MiB. Ostlandet/Vestlandet tiles
(often 20–117 MB on disk → hundreds of k edges owned) do **not** fit; full
materialize then discard regresses Bergen cold (observed wall **9813** /
pack_load **6601** / copy **2606** before the fit gate).

**Policy now:** if corridor clips are set and the tile is unlikely to fit
(`file*4 >= cap` or `file >= 12 MiB`), **clip-hydrate** (no full copy). Small
tiles miss_full + insert for cross-hop reuse.

| Metric | Post-pre | 2a (fit-gated) |
| --- | --- | --- |
| wall_ms Bergen eco cold | 6942 | **7247** |
| pack_load_ms | 3872 | **3998** |
| copy_ms | 1530 | **1583** |
| merge_adj_ms | 766 | **749** |
| ferry_ms | 263 | **251** |
| distance_km | 459.71 | **459.71** |
| Tromsø wall_ms | 31806 | **33168** |
| Tromsø distance / days | 1766.89 / 4 | **1766.89 / 4** |

2a does not move Bergen or Tromsø much under the current RSS budget; real win
needs larger tile retention or **2c** (search without owned full-tile copy).
Infrastructure + fit gate kept for small-tile hits.

### Remaining

- **2b** parallel tile load
- **2c** search on mapped tiles (main path to &lt;5 s Bergen cold)
