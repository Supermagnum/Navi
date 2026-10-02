# Directed waypoint snap + ferry overlay reassessment

Branch: `fix/directed-snap` (from `dev` / dig tip after #138). Client only. No `graph_format_version` bump. No navi-server. No merge. PR: https://github.com/Supermagnum/Navi/pull/140

## STEPs 1–2 (recap)

Bergen→Stavanger city centre failed because legacy `SnapRole::Any` snapped to one-way **source stub** OSM node `11335393456` (`reachable_from_main=false`). Destination-role snap picks `264565258`. Pack ferries (Halhjem–Sandvikvåg, Arsvågen–Mortavika) were already present.

Labels: largest SCC in giant weak component; compact **reject** stub sets (`origin_reject` / `dest_reject`) instead of storing ~250k inclusion NodeIds. Computed once on the final merged corridor (`ensure_directed_snap_labels`), not on every tile hydrate.

## 1. Full matrix (tablet SM-P613 `R52TB0JQEDE`, 2026-10-03)

Force-stop before cold. Overlay: all rows below used pack connectivity only (`ferry_overlay=skip_already_connected` or `skip_corridor_cache_hit`). No `ferry_preparing`. Dig = `origin/dev` after #138 ([mmap-graph-search.md A.5](mmap-graph-search.md)).

| Case | eco | lt | ok | wall_ms | km | peak_rss | ferry_fp | geom (prefix) | vs dig |
| --- | --- | --- | --- | ---: | ---: | ---: | --- | --- | --- |
| raufoss_bergen cold | true | false | true | 10303 | 459.61 | 872.4 | — | `db5eb912…` | dig `8a8c6f7b…` / 459.71 — **geom differs** (see note) |
| raufoss_bergen | false | false | true | 2192 | 485.35 | 872.4 | — | `915e3d9d…` | dig `540bb2ad…` / 485.45 — **geom differs** |
| raufoss_bergen_warm | true | false | true | 2505 | 459.61 | 872.4 | — | `db5eb912…` | same as cold eco |
| raufoss_dombas | true/false | false | true | 6270/1935 | **206.81** | 931.4 | — | `6a1360bc…` | **match dig** |
| bergen_forde | true/false | false | true | 5078/1423 | 170.74 | 931.4 | Lavik–Oppedal@5.72 | `0a2ebf66…` | dig `0bff0c85…` / 171.01 — near-match km; geom differs slightly |
| bergen_stavanger | false | false | true | 9206 | **206.78** | 984.9 | Halhjem\|Arsvågen | `63923d18…` | dig was **228.21** densify `1cf463d5…` — intentional |
| bergen_stavanger | true | false | true | 2638 | **206.16** | 984.9 | same | `93fc4c85…` | intentional (snap + no coastal densify) |
| bergen_stavanger_lt | false | true | true | 10416 | **230.10** | 984.9 | same | `28d75202…` | densify hops=2 (longTrip on) |
| bergen_stavanger_lt | true | true | true | 10106 | **229.48** | 984.9 | same | `e1949336…` | densify eco |
| raufoss_tromso | false | true | true | 36756 | **1766.55** | 984.9 | 5 named legs | `48565da1…` | dig `e79679c2…` / 1766.89 — km≈; geom differs |

**Snap-changed endpoints (legitimate):**

| Case | Old (dig / Any) | New (Destination) | Why |
| --- | --- | --- | --- |
| Bergen→Stavanger centre | `11335393456` (source stub) | `264565258` | Stub cannot be reached directed from Bergen; new node is in `reachable_from_main` |

Raufoss→Bergen / Førde geom deltas vs dig are small km drift (≤0.3 km) under the same packs; not attributed to a changed O/D snap node in diags. Stavanger **is** the intentional snap + densify-policy change.

**Overlay on:** not exercised in this matrix — every OD stayed `skip_already_connected` after directed snap. Forcing overlay would require a corridor that fails directed connectivity (missing pier/ferry in packs). Host stub-PBF Vestlandet: same skip path.

**Host (Vestlandet only):** centre longTrip off **206.78** km; station **205.41** km; ferries Halhjem\|Arsvågen; no `unnamed@195`.

## 2. Tromsø

- Routes with overlay **off** (`ferry_overlay=skip_already_connected` on every densify hop). No lazy sidecar kick / no `ferry_preparing`.
- **1766.55 km**, 17 hops, 5 ferry legs (Lund–Hofles, Holm–Vennesund, Hurtigruten, Levang–Nesna, Drag–Kjøpsvik).
- Soft motor: `motor_multi_day: days=4; total_driving_h=25.50`. Three overnight lodgings: Treetop Ekne, Korgenfjellet Fjellstue, Soltun soldatheim — **yes, still 4 days / 3 overnight stops**.

## 3. Stavanger 206.78 vs 230.10 km

Cause: **#138 same-stem coastal densify** (`ferry_same_stem_densify`) forced densify when longTrip was **off** if a Vestlandet ferry sidecar was ready — tablet both lt on/off became ~230 km. Host without ready sidecar kept longTrip off at **206.78** while longTrip on densified to **230.10** (span 1.42° > `LONG_TRIP_CHUNK_DEG` 1.15° → 2 hops).

**Removed** that coastal densify rule. After removal:

| longTrip | Behaviour | km (tablet default) |
| --- | --- | ---: |
| off | single-shot A* | **206.78** |
| on | intentional densify (span > CHUNK) | **230.10** |

They do **not** become equal: longTrip on still densifies by design. The 23 km gap is densify hop joints vs single optimal corridor, not missing ferries. Removing coastal densify restored longTrip **off** to the single-shot dig-class crossing (~206–207 km).

## 4. Peak RSS ~951 / 984 vs #138 ceiling 933 MiB

| Source | Effect |
| --- | --- |
| Directed inclusion labels (~250k×2 HashSets) | ~+15–20 MiB (first PR tip) |
| Compact reject stubs (~60–174 nodes) | reclaimed inclusion overhead |
| Per-tile Kosaraju | CPU only; deferred to final merge |
| Single-shot Vestlandet corridor (~259k nodes / ~538k edges) | **dominant** — owned-graph estimate ≫ 900 MiB |

#138’s **933 MiB** matrix max was measured while Bergen→Stavanger **densified** (smaller per-hop graphs ~167k nodes). Single-shot Vestlandet cold now peaks **984.9 MiB** (process HWM across the matrix). Labels are no longer the driver. Staying ≤933 with single-shot Vestlandet needs mmap search (Phase 2) or re-introducing densify for RSS — not done here. Dombås/Førde rows stay ≤931.4.

## 5. Snap check cost

| Stage | Where | Tablet note |
| --- | --- | --- |
| `directed_label_ms` | Final merge `ensure_directed_snap_labels` (Kosaraju + stub sets) | Once per cold corridor; Vestlandet merge ~0.5–1.7 s (part of pack_load / adj). Skipped on tile hydrate and on corridor cache hit. |
| Snap role filter | `directed_snap_ok` O(1) HashSet lookup per candidate inside existing `nearest_routable` pad scan | Warm `directed_snap_ms` (full O/D snap wall) ~111–317 ms includes the whole pad scan dig already paid; **added** filter cost is ≪ 50 ms. |

Target ≤~50 ms **added** for the reachability filter: met for the filter itself. Cold label build remains a pack_load cost (not snap-filter); deferred off tiles to avoid N× Kosaraju.

## 6. Logcat (LMK / ANR)

Explicit check on `adb logcat` + warn buffer during `RegionToRegionPerfMatrixInstrumentedTest` (force-stop before cold):

- **No** `lowmemorykiller` for `no.navi.app`
- **No** `am_anr` / `ANR in no.navi.app`
- Only post-test `ActivityManager: Killing … stop no.navi.app due to finished inst` (instrumentation teardown)

## 7. Ferry overlay status

| Path | When |
| --- | --- |
| `supplement_pack_ferries_from_pbf` / plan corridor | Only if `ferry_hop_connectivity_gate` → `Disconnected` or `SnapFailed` after directed Origin/Destination snaps |
| Skip | `Connected` / corridor cache hit / stub PBF / no ferry edges in sidecar clip |
| Sidecar build | Lazy: plan path spawns `ensure_ferry_sidecar` off-thread and returns `ferry_preparing` when overlay is needed and sidecar missing/stale. **Not** at pack install (`emitInstalledForRouting` / `markUsable` kicks removed). |
| Costing | Overlay/PBF ferry edges use `ferry_base_weight_m` (duration or 10 km/h + 10 min car boarding) — same as server |

## Host harness

```bash
cargo run -p navi-ffi --release --bin directed-snap-diag -- \
  --pack-dir .packs/long-trip-packs
```
