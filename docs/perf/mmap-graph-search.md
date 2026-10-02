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

### Dig vs tip vs WIP (2b) — tablet SM-P613 (2026-10-02)

Logcats: dig `/tmp/r2r_dev_matrix_logcat.txt`, tip `/tmp/r2r_tip_matrix_logcat.txt`
(0b732602), WIP 2b `/tmp/r2r_2b_matrix_logcat.txt`
(`perf/mmap-graph-search` WIP: parallel tile load + directed ferry-hop gate +
`bergen_stavanger` longTrip densify). Device: **R52TB0JQEDE**.

#### Bergen→Stavanger (ferry check)

Dig/tip: corridor **disconnected** (no route; dig wall ~36 s / tip ~18 s).
WIP 2b: `longTrip=true` (this case only), densify hops=2. Root cause of leg2
fail was ferry overlay **skipped** via undirected UF (`skip_already_connected`)
while directed A* could not cross Sandvikvåg→Stavanger water. Fix: gate uses
directed reachability; disconnected → `disconnected_try_overlay` + ferry
sidecar (`vestlandet-latest.navi-ferry-overlay-car.rkyv`). First cold build of
that sidecar from the 245 MB region PBF once cost ~18 min (`pbf_build`); later
plans hit sidecar (~3–4 s pack_load).

| Build | eco | ok/uses | distance_km | ferry legs / fp | wall_ms |
| --- | --- | --- | --- | --- | --- |
| dig | default | false | 0 | 0 / - | 36218 |
| dig | eco | false | 0 | 0 / - | 27615 |
| tip | default | false | 0 | 0 / - | 18387 |
| tip | eco | false | 0 | 0 / - | 15934 |
| **2b + longTrip** | default | **true** | **228.21** | **2 / Halhjem–Sandvikvåg@21.32\|Arsvågen–Mortavika@9.15** | **9402** |
| **2b + longTrip** | eco | **true** | **228.21** | **same fp** | **8900** |

Default and eco share geom `1cf463d52af4c4ee…` and the same ferry fingerprint.

#### Geometry

Successful matrix routes share **identical** `PROFILE_GEOM` hashes across dig / tip / 2b
(except Stavanger, which only succeeds on 2b):

| Route | geom_sha256 (prefix) | notes |
| --- | --- | --- |
| raufoss_bergen eco | `8a8c6f7b…` | 459.71 km |
| raufoss_bergen default | `540bb2ad…` | 485.45 km |
| raufoss_dombas | `6a1360bc…` | 206.81 km |
| bergen_forde | `0bff0c85…` | 171.01 km; ferry Lavik–Oppedal@5.72 |
| raufoss_tromso | `e79679c2…` | 1766.89 km |
| **bergen_stavanger** | **`1cf463d5…`** | **228.21 km; dig/tip empty `e3b0c442…` (failed)** |

#### Tromsø hop breakdown (tip ~33.8 s / 2b ~32.6 s)

17 densify hops; final `motor_multi_day: days=4` (Treetop Ekne, Korgenfjellet
Fjellstue, Bardu Hotel) on dig/tip/2b.

| | dig | tip (2a) | **2b (final)** |
| --- | --- | --- | --- |
| wall_ms | 51687 | **33818** | **32566** |
| hop1 pack_load / astar | 6144 / 275 | 2456 / 293 | 1816 / 280 |
| peak_rss_mb (tromso row) | 1091.7 | 1077.1 | **1154.9** |

**Per-hop tile_cache (Tromsø plan only; tip / 2b):**

| | tip | 2b |
| --- | --- | --- |
| tile_cache hits | 8 | 7 |
| miss_clip_hydrate (fit-gate reject) | **33** | **33** |
| miss_full | 22 | 23 |
| hit share of tile loads | **12.7%** | **11.1%** |

Fit gate rejects oversized tiles (`file*4 >= cap` or `file >= 12 MiB`) →
clip-hydrate, never inserted into the full-tile LRU. Multiday overhead on hops
is ~0 ms (final multi-day scrape is after hops). Ferry stage per hop is the
connectivity/overlay gate (~50–370 ms), not route ferry count.

2b hop pack_load (ms): 1816, 1710, 1618, 435, 511, 1355, 1310, 545, 544, 648,
466, 326, 554, 375, 398, 392, 502. A*: 280, 229, 700, 35, 21, 138, 543, 258, 71,
31, 82, 37, 27, 60, 29, 119, (leg17 in report body).

#### 2b — Bergen eco cold stage + Tromsø wall

`pack_stage_threads=tile_load_parallel=2` on 2b; tip was `single_threaded`.

| Metric | tip (2a) | **2b (final)** |
| --- | --- | --- |
| wall_ms | 7448 | **6606** |
| pack_load_ms | 3985 | **3383** |
| pagein_ms | 768 | 1136 |
| validate_ms | 47 | 41 |
| copy_ms | 1594 | 1810 |
| merge_hash_ms | 417 | 444 |
| merge_adj_ms | 765 | 754 |
| ferry_ms | 259 | 614 |
| astar_ms | 1250 | 1211 |
| distance_km | 459.71 | 459.71 |
| Tromsø wall_ms | 33818 | **32566** |
| Stavanger wall_ms (default / eco) | fail | **9402 / 8900** |
| peak_rss_mb (Bergen eco cold) | 827.8 | **881.8** |
| peak_rss_mb (matrix max) | 1077.1 | **1154.9** |

2b cuts Bergen cold ~0.8 s and Tromsø ~1.3 s vs tip under the same fit-gated
tile cache; RSS ceiling ~1.15 GB. Stavanger is a permanent matrix case with
matching default/eco ferry fingerprints.

### Production defaults after 2b (tablet-safe)

Parallel tile load and the full-tile LRU stay in tree for experiments, but
**device defaults** avoid the RSS cost that did not pay for itself on SM-P613:

| Knob | Default | Override |
| --- | --- | --- |
| Tile-load concurrency | **1** when `/proc/meminfo` MemTotal &lt; **6 GiB**; else **2** | `NAVI_TILE_LOAD_PARALLEL=N` (1–8) |
| Full-tile LRU (`tile_cache.rs`) | **off** | `NAVI_TILE_CACHE=1` (or `true`/`yes`/`on`) |

Rationale: parallel=2 on the tablet raised peak RSS ~50–80 MiB for ~0.3 s wall.
Tile cache hit share on Tromsø was ~11% with **33** fit-gate rejects per plan —
not enough to justify retaining full tiles beside the corridor Arc. Code paths
remain; PLAN_PERF notes `tile_load_parallel`, `mem_total_mb`, and
`tile_cache_enabled=0|1`.

#### Confirmation matrix (defaults: parallel=1, tile_cache off)

Device: SM-P613 `R52TB0JQEDE`, tip with the defaults above (2026-10-02).
`tile_load_parallel=1`, `tile_cache_enabled=0`, `mem_total_mb=3505`. No
`lowmemorykiller` / ANR for `no.navi.app`.

| case | eco | wall_ms | pack_load_ms | distance_km | peak_rss_mb | geom vs dig | ferry |
| --- | --- | --- | --- | --- | --- | --- | --- |
| raufoss_bergen | true (cold) | **7780** | 4357 | **459.71** | 869.3 | match `8a8c6f7b…` | — |
| raufoss_bergen | false | 2358 | 6 | **485.45** | 869.3 | match `540bb2ad…` | — |
| raufoss_bergen_warm | true | 2591 | 6 | **459.71** | 869.3 | match | — |
| raufoss_dombas | true | 6787 | 4399 | **206.81** | 1064.5 | match `6a1360bc…` | — |
| bergen_forde | true | 3949 | 2172 | **171.01** | 1064.5 | match `0bff0c85…` | Lavik–Oppedal@5.72 |
| bergen_stavanger | false | 10496 | 2503 | **228.21** | 1064.5 | dig empty; branch `1cf463d5…` | Halhjem–Sandvikvåg@21.32 \| Arsvågen–Mortavika@9.15 |
| bergen_stavanger | true | 9850 | 2296 | **228.21** | 1064.5 | same geom/fp as default | same |
| raufoss_tromso | false | **35220** | (per-hop) | **1766.89** | 1064.5 | match `e79679c2…` | 5 legs |

Cold pack_load cut vs Step 1: Bergen eco **15.6 s → ~7.8 s** wall (pack_load
~12.4 s → ~4.4 s); Tromsø **~50 s → ~35 s**. Peak RSS matrix max was **1064.5** MiB
before the RSS clamp below (below 2b’s ~1155 with parallel=2). Stavanger ferry
fingerprints match default/eco.

#### Peak RSS clamp (vs #137 matrix max 954.4 MiB)

Two retainers pushed matrix max ~110 MiB over post-#137 (`ba37869c`):

1. **Corridor LRU held Bergen while Dombås materialized** — soft-cap byte
   estimates under-count owned-graph RSS, so two corridors "fit". Peak jumped at
   `raufoss_dombas` (869 → 1064). **Fix:** single-MRU insert + clear cache on
   miss before load (eco→non-eco same key stays a hit and never hits evict).
2. **Ferry overlay cloned the corridor** — `load_tiled_graph_files` inserted the
   pre-ferry Arc into the LRU, then `arc_graph_owned` failed `try_unwrap` and
   cloned (~2×) while merging the Vestlandet sidecar (~65 MB / 72k edges). Showed
   up on `bergen_stavanger` eco after the single-MRU fix. **Fix:** defer LRU
   insert until after ferry supplement so merge unique-owns the corridor.

Tile cache disabled path was already zero-retention; not the regressor.

##### Post-fix matrix (defaults: parallel=1, tile_cache off)

Device: SM-P613 `R52TB0JQEDE` (2026-10-02). No LMK/ANR. Distances and geom hashes
unchanged vs dig / prior tip.

| case | eco | wall_ms | pack_load_ms | distance_km | peak_rss_mb | #137 rss | pre-fix tip |
| --- | --- | --- | --- | --- | --- | --- | --- |
| raufoss_bergen | true (cold) | **7142** | 4024 | **459.71** | 931.4 | 954.4 | 869.3 |
| raufoss_bergen | false | 2208 | 6 | **485.45** | 931.4 | 954.4 | 869.3 |
| raufoss_bergen_warm | true | 2466 | 6 | **459.71** | 931.4 | 954.4 | 869.3 |
| raufoss_dombas | true | 4534 | 2459 | **206.81** | **933.2** | 954.4 | **1064.5** |
| bergen_forde | true | 3703 | 2164 | **171.01** | 933.2 | 954.4 | 1064.5 |
| bergen_stavanger | false | 9859 | 2426 | **228.21** | 933.2 | (n/a) | 1064.5 |
| bergen_stavanger | true | 9074 | 2168 | **228.21** | 933.2 | (n/a) | 1064.5 |
| raufoss_tromso | false | **34329** | (per-hop) | **1766.89** | 933.2 | 954.4 | 1064.5 |

Matrix max peak RSS **933.2 MiB** (≤ #137’s **954.4**). Run-to-run Bergen cold
alone varies ~870–980 MiB with identical node/edge counts; the important signal
is that Dombås/Stavanger no longer raise the matrix ceiling above Bergen.

### Remaining

- Optional: ship prebuilt `{stem}.navi-ferry-overlay-*.rkyv` with packs so first
  coastal overlay never pays full-PBF `pbf_build`
- **2c** search on mapped tiles (main path to &lt;5 s Bergen cold) — design note
  below; **not implemented on this branch**

## Step 2c design note — A* on mapped tiles (no merged owned copy)

Goal: keep Bergen eco cold pack_load near mmap/page-in + ferry gate only
(~1–2 s class on the tablet), and cut Tromsø rematerialize-per-hop, **without**
building the merged `RouteGraph` that today costs copy + full-node hash merge +
adjacency rebuild (~2–3 s of the remaining pack_load after 2-pre).

### Addressing nodes across tiles

Today A* expands integer OSM node ids on one owned adjacency list. Without a
merged copy, a plan would hold an ordered list of **mapped tile views**
(`Mmap` + archived `FlatGraphPack`) plus the same corridor clip boxes used now.

- **Node identity** stays the OSM id (stable across tiles in v9).
- **Local index** inside a tile is the position in that tile’s `node_ids` /
  coord arrays — not globally dense.
- Expansion for node `u`: for each mapped tile whose clip may contain `u`, look
  up `u` in that tile (see border resolution), walk archived outgoing edges,
  apply the clip predicate, emit neighbour `(v, cost)` with `v` still an OSM id.

No single dense `0..N` renumbering unless a later format adds one. Open-set /
g-score maps stay keyed by OSM id (same as today after merge).

### Shared border nodes without a full-node hash (v9 has no border marks)

v9 packs do **not** mark border nodes (`v9_border_marks=false`). Shared OSM
ids still appear in every tile that stores an incident edge.

Without the merge-time `HashMap`:

1. **Per-tile id → local index**: build a compact hashmap (or sorted id +
   binary search) **once per mapped tile** when the tile is opened for search —
   O(nodes_in_tile), not O(sum of all corridor nodes twice). Miss ⇒ node not in
   that tile.
2. **Cross-tile continuity**: if `u` is only needed as a Steiner point on a
   clipped edge in tile A, tile B that also stores `u` will find it via (1).
   A* does not need a separate stitch table as long as every clip-kept edge’s
   endpoints are present in at least one open tile’s node table (true for v9
   archives that store both ends with each edge).
3. **Duplicate edges** across overlapping clips: either accept duplicate
   relaxations (same `v`, same weight → idempotent) or keep a small
   `(source,target,length_mm,…)` bloom/set for the corridor — much cheaper than
   today’s full merge hash over every edge up front.

If (1)+(3) prove too slow or memory-heavy on Tromsø, **v10** should add
explicit border marks or a stem-level border id list so only border nodes are
indexed globally (see below).

### Eco / ferry / soft-cost overlays

| Overlay | Attach point under mapped search |
| --- | --- |
| Eco reweight | Keep today’s edge-cost function; read archived base attributes from the
  mapped edge and apply eco multipliers at expand time (no owned edge clone).
  Warm eco can still cache a **reweighted cost side table** keyed by
  `(tile_ix, edge_ix)` if profiling shows attribute decode dominates. |
| Ferry overlay | Same directed connectivity gate + sidecar as 2b. Inject ferry edges as
  a tiny owned adjacency delta keyed by OSM id (hundreds of edges), consulted
  after mapped-tile expand — do **not** rewrite tile files. |
| Soft costs / avoidances | Same as eco: evaluate in the cost fn from archived
  flags + live overlays; optional sparse side table for mutated weights. |

Corridor cache today stores a merged Arc; under 2c it would store **tile path
list + clip fingerprint + optional ferry delta**, and reopen mmaps (or keep a
small mmap LRU) instead of retaining owned nodes/edges.

### Expected tablet impact (order-of-magnitude)

| | Today (2b defaults: parallel=1, tile_cache off) | 2c target |
| --- | --- | --- |
| Bergen eco cold wall | ~6.6–7.5 s | **&lt;5 s** (pack_load ~mmap/page-in + ferry ≪ copy/merge) |
| Tromsø wall | ~33 s | hop pack_load drops toward page-in; wall toward A*+multiday |
| Peak RSS | ~0.9–1.1 GiB (owned corridor) | **lower**: mapped files + open-set; avoid ~hundreds of MiB owned edges |

Numbers are goals, not measurements — validate on SM-P613 with the same matrix
and LMK/ANR watch.

### When v9 makes 2c impractical — what v10 needs

Ship a format bump only if mapped expand cannot stay correct/fast:

1. **`node_is_border` bit** (or separate border-id array) per tile so cross-tile
   index is border-only.
2. Optional **dense local adjacency CSR** already aligned to archived order
   (avoid rebuilding adjacency from edge lists at open time).
3. Optional **corridor slice** or precomputed clip edge-index ranges to skip
   full edge scans on large tiles.
4. Stable **graph_format_version** gate; keep v9 readable for one release.

Until then: stay on owned merge for search; 2c remains the follow-up after this
PR’s ferry fix + pack_load cuts.

## Phase 1 PART A — ferry sidecar (gates merge of #138)

### A.1 Fast ferry sidecar build

**Prior cost:** `build_ferry_overlay_from_pbf` did ~10 full PBF scans (in-bbox
node set, ferry ways, 4 approach hops, terminal coords, near-ids, vacuum ways,
coord resolve) — first Vestlandet build ~18 min on SM-P613 from the 245 MB
region PBF.

**Rewrite:** ≤2 PBF reads — (1) collect `route=ferry` + pier/footway/platform
promotions only, (2) resolve those node coords; BFS (≤4 hops) + terminal-radius
vacuum run in memory. Pack car highways are not duplicated into the sidecar.

**Tablet SM-P613 (delete + rebuild all stems, 2026-10-02):**

| stem | build_ms | out_bytes |
| --- | ---: | ---: |
| ostlandet-latest | 89018 | 25741396 |
| vestlandet-latest | 50340 | 21215172 |
| trondelag-latest | 26528 | 3488160 |
| nord-norge-latest | 67203 | 7647148 |
| sorlandet-latest | 10958 | 3592872 |

Vestlandet ~50 s / ~21 MB (was ~18 min / ~62 MB with full residential vacuum).
Østlandet / Nord-Norge remain I/O-bound on two full region-PBF passes (~67–89 s);
still ≫10× faster than the multi-pass builder. Target “well under a minute”
holds for Vestlandet / Trøndelag / Sørlandet; largest stems need a compact
`*.ferry.osm.pbf` extract for a further cut.

### A.2 Build out of the plan path

`ensure_ferry_sidecar` runs at pack install (`emitInstalledForRouting`) and
pack refresh / usable (`markUsable`) via `FerrySidecarBackground`, with
download-progress labels (`Preparing ferry data for {region}…`).

Plan path loads the sidecar only (`ferry_overlay_for_plan`). Overlay clips use
the OD **corridor band** (not trip-AABB) so pack AABB fallback cannot pull a
coast-length ferry mesh. If the hop needs overlay and the sidecar is
missing/stale: return `PackLoadError::FerryPreparing` →
`search_terminate_reason=ferry_preparing` (no PBF parse). UI shows the status
and auto-retries (`planKick`) every 1.5 s until the background job finishes.

**First plan with no sidecar (Bergen→Stavanger):** polls saw
`ferry_preparing` for ~35 s, then route at **~45.5 s** wall
(`distance=228.21`, Halhjem+Arsvågen).

### A.3 Why packs lack some ferry links (finding only — no navi-server change)

**Observation:** Bergen→Førde uses **Lavik–Oppedal** from the published
Vestlandet pack alone. Bergen→Stavanger needs **Halhjem–Sandvikvåg** and
**Arsvågen–Mortavika** from the client PBF ferry overlay; without it, directed
A* reports disconnected (undirected UF could falsely `skip_already_connected`).

**Server bake (`navi-server-local` / `pack-convert-core`):** Pass 1 spill already
keeps `route=ferry` via `tags_indicate_ferry` (see `ferry_tunnel_v9` regression).
Car highways are ingested; **`man_made=pier` / footway boarding links are not**
promoted into the car graph. Ferry ways whose landfall only connects through
pier/footway stubs therefore sit as islands (or one-sided) in tiled packs —
directed reachability fails even when a long ferry edge exists elsewhere in the
stem. Tile clips can also drop pier tips a few hundred metres past the tile
pad, breaking OSM-id join to inland highways.

Lavik–Oppedal is highway-connected at both ends in OSM, so it survives pack
bake. Halhjem / Arsvågen depend on pier-class approaches the bake drops.

**Exact server-side fix (do not implement here):** In
`pack-convert-core` tiled Pass 1 (same rules as client
`ferry_approach_tags`), retain pier / footway / platform ways that share a
node with a `route=ferry` way (or lie within ~1.5 km of a ferry endpoint),
promote them to `highway=service` + motor access for topology, and keep them
in the tile that owns the ferry terminal (extra pad around ferry endpoints if
needed). Re-bake Vestlandet (and other coastal stems). After that, client
ferry sidecars become unnecessary for Halhjem/Arsvågen-class crossings.

### A.4 Long-trip off (Bergen→Stavanger)

Span Bergen→Stavanger is ~1.42° Chebyshev > `LONG_TRIP_CHUNK_DEG` (1.15°).
Single-shot + full ferry overlay admits a coast-chained water shortcut
(`unnamed@195`, ~212 km). Densify (`longTrip=true`, or same-stem **coastal**
ferry densify when a vestlandet/nord-norge/sørlandet sidecar is ready) splits
into hops ≤1.15° so each A* cannot see the coast-long chain.

Matrix permanently includes both: `bergen_stavanger` (longTrip off) and
`bergen_stavanger_lt` (longTrip on), each × default and eco. Both variants
 densify on this OD and return **228.21 km**, ferries
Halhjem–Sandvikvåg@21.32 | Arsvågen–Mortavika@9.15, geom `1cf463d5…`.
Cross-stem mid trips (Raufoss→Bergen) and inland same-stem (Raufoss→Dombås)
do **not** take the ferry densify path.

### A.5 Verify (tablet SM-P613 `R52TB0JQEDE`, 2026-10-02)

- Sidecar rebuild timings/sizes: see A.1 table. No LMK / ANR for `no.navi.app`.
- First plan before sidecar ready: `ferry_preparing` until ~45.5 s, then 228.21.
- Cold/warm with sidecars present: Stavanger pack_load ~2.2–2.4 s; Bergen eco
  cold **7611 ms**, warm **2447 ms**.
- Full matrix (peak RSS max **932.5 MiB** ≤933):

| case | eco | wall_ms | distance_km | geom | ferry |
| --- | --- | ---: | ---: | --- | --- |
| raufoss_bergen cold | true | 7611 | **459.71** | `8a8c6f7b…` | — |
| raufoss_bergen | false | 2202 | **485.45** | `540bb2ad…` | — |
| raufoss_bergen_warm | true | 2447 | **459.71** | same | — |
| raufoss_dombas | true | 4835 | **206.81** | `6a1360bc…` | — |
| bergen_forde | true | 3936 | **171.01** | `0bff0c85…` | Lavik–Oppedal@5.72 |
| bergen_stavanger (lt off) | false/true | ~9.3/8.8 s | **228.21** | `1cf463d5…` | Halhjem\|Arsvågen |
| bergen_stavanger_lt | false/true | ~8.8/8.8 s | **228.21** | same | same |
| raufoss_tromso | false | 34020 | **1766.89** | `e79679c2…` | 5 legs; **4 days / 3 overnight** |

**STOP — wait for merge of #138. Do not start Phase 2.**

## Phase 2 PART B — time outside pack_load (branch `perf/tile-index-sidecar`)

Merged #138 at `1f5a60b9`. Device SM-P613 `R52TB0JQEDE`.

### B.1 Residual breakdown (Bergen eco cold)

Source: post-RSS-clamp matrix logcat (`/tmp/r2r_rss_fix3_matrix_logcat.txt`),
tip that became #138. **wall_ms=7142**, **pack_load_ms=4024** → residual
**~3118 ms** outside pack_load.

`ROUTE_PLAN_STAGES` (sums to ~3066 ms; ~52 ms timer/unaccounted):

| Stage | ms | Notes |
| --- | ---: | --- |
| profile_map_ms | 203 | travel-profile / settings map |
| eco_reweight_ms | 238 | eco edge weights on owned corridor |
| snap_ms | 233 | start+end nearest_routable |
| network_pref_ms | 0 | |
| **astar_ms** | **1222** | pathfinding IndexMap keyed by OSM id (+ surface, edge) |
| **polyline_ms** | **388** | overlay string from edge shapes |
| **poi_barrier_ms** | **743** | cold POI pack miss (polyline-clipped) |
| rest_branch_ms | 1 | |
| multiday_ms | 0 | single-day |
| pause_pins_ms | 26 | |
| report_addons_ms | 12 | |
| **Sum (excl. pack_load)** | **~3066** | |

Nothing material left unexplained (~1.7% of residual).

### B.2 Dense A* (OSM HashMap → dense CSR + flat arrays)

**Before:** `pathfinding::astar` with `FxIndexMap` states keyed by OSM `NodeId`
(plus surface + incoming edge on car/truck); `RouteGraph.adjacency` was
`HashMap<NodeId, Vec<usize>>`; heuristic did `nodes.get` HashMap lookups.

**After:** at adjacency rebuild, build dense `id→u32`, CSR `adj_off`/`adj_edge`,
flat lat/lon/blocked. Custom A* keys by `node_idx` (×4 surfaces for car/truck);
parent edge stored in a parallel array (not in the open-set key).

Tablet before/after `astar_ms` (same matrix; distances + geom hashes must match):

| case | eco | astar before | astar after | distance | geom |
| --- | --- | ---: | ---: | ---: | --- |
| *(fill after tablet run)* | | | | | |

### B.3 Other stages >300 ms

- **polyline_ms (~388):** pre-size the overlay string + `write!` into it (no
  per-point `format!` realloc storm).
- **poi_barrier_ms (~743):** cold miss after the path; needs polyline/corridor
  clip. Warm is ~155 ms (cache hit). Overlap-with-A* / corridor-clip preload
  left for a follow-up if residual still blocks &lt;5 s after dense A*.
