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

_(pending)_
