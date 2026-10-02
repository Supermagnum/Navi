# Directed waypoint snap + ferry overlay reassessment

Branch: `fix/directed-snap` (from `dev`). Client only. No `graph_format_version` bump. No navi-server changes. No merge.

## STEP 1 — Reproduce (published v9 Vestlandet, overlay/sidecar disabled)

Host packs: published v9 `vestlandet-latest` car tiles. Stub `vestlandet-latest.osm.pbf` so ferry overlay cannot build (`MIN_FERRY_OVERLAY_PBF_BYTES`).

Coordinates:

| Point | Lat | Lon |
| --- | ---: | ---: |
| Bergen | 60.388144 | 5.3347434 |
| Stavanger city centre (matrix) | 58.97 | 5.733 |
| Stavanger station | 58.9670 | 5.7315 |

### City-centre destination snap

| Role | Node | Dist m | can_reach_main | reachable_from_main | out_deg |
| --- | ---: | ---: | --- | --- | ---: |
| Any (legacy) | **11335393456** | 40.9 | true | **false** | 1 |
| Destination (fixed) | 264565258 | 61.6 | true | true | 1 |

- Same weak component as Bergen: yes.
- Directed path Bergen → Any snap: **no**.
- Directed path Bergen → Destination snap: **yes**.
- Source-stub population in the same weak component (can reach main, not reachable from it): **~174 nodes**.

Verdict: the city-centre matrix coordinate snaps to a **directed source stub** (leave-only into the network). Weak UF treats it as connected; directed A* cannot arrive. This is a **destination snap problem**, not missing Halhjem/Arsvågen pack ferries.

### Stavanger station (no overlay)

Routes with pack ferries only: `ok=true`, `distance_km≈205.41`, `route_uses_ferry=true`, `ferry_overlay=skip_*`.

### Bergen→Stavanger centre after snap fix (no overlay)

| Case | ok | wall_ms | distance_km | ferry |
| --- | --- | ---: | ---: | --- |
| longTrip off | true | ~560 | **206.78** | pack ferries (`route_uses_ferry=true`) |
| longTrip on | true | ~5400 | **230.10** (2 hops) | pack ferries |

navi-server observation (Halhjem–Sandvikvåg + Arsvågen–Mortavika present on v9 without overlay because landings already touch `highway=service`/`trunk`) matches the client once the destination is directed-reachable. Earlier client diagnosis (pier links dropped / directed islands needing overlay) was wrong for this OD.

## STEP 2 — Fix

- Precompute on `rebuild_adjacency`: largest SCC inside the giant weak component, then
  - `can_reach_main` (reverse BFS from that SCC) — valid **origins**
  - `reachable_from_main` (forward BFS) — valid **destinations**
- `RouteOptions.snap_role`: `Any` | `Origin` | `Destination` | `Via`
- `nearest_routable_*` rejects directed-unusable nodes and takes the next nearest within the existing 750 m gate
- Plan path sets Origin / Destination / Via roles
- Ferry-overlay connectivity probe uses directed snaps + directed BFS after weak UF, so one-way dead-end destination snaps neither falsely skip nor falsely force overlay

Tests:

- `destination_skips_one_way_dead_end_stub`
- `origin_rejects_tiny_isolated_sink`
- `ferry_base_weight_matches_server_formula`
- `ferry_costing_rejects_long_unnamed_chain_vs_short_tagged`

## STEP 3 — Ferry overlay reassessment + costing

### Costing: client overlay vs server `ferry_base_weight_m`

On `origin/dev`, client PBF / overlay ferry edges used `base_weight = length_m`. Pack rkyv edges already carry server-baked weights. That mismatch under-priced slow water hops and admitted coast-chained shortcuts.

Aligned with navi-server `pack-convert-core::ferry_base_weight_m` (client: `core/src/routing/graph/builder.rs`):

| Input | Weight |
| --- | --- |
| Tagged OSM `duration` | seconds × 80 km/h drive-equivalent |
| Else (no duration) | `length_m × (80 / 10)` (10 km/h fallback) |
| Car/truck boarding | +10 minutes at 80 km/h drive-equivalent |
| Foot/bike boarding | 0 |
| Geometry | `length_m` unchanged |

`duration` is retained in overlay tag filtering (`keep_way_tag`). Both `bbox_edge` (overlay/PBF build) and pack `push_directed_edge` call `ferry_base_weight_m`.

### Before / after: unnamed ~195 km chain (single-shot)

| Mode | Short ferry (20 km, `duration=0:40`) | Unnamed chain (195 km, no duration) | A* preference |
| --- | ---: | ---: | --- |
| **BEFORE** length-only | 20 km weight | 195 km weight | Chain competitive with land+ferry OD (~206 km); single-shot + full overlay could report `unnamed@195` |
| **AFTER** `ferry_base_weight_m` | 40 min × 80 km/h + 10 min boarding ≈ **80.0 km** equiv (66.7 + 13.3) | 195 × 8 + boarding ≈ **1573 km** equiv | Chain loses by ~23×; pack Halhjem/Arsvågen-class crossings win |

Host re-check (this branch, stub PBF → overlay cannot build; directed snap only):

| Case | overlay | ok | distance_km | ferry_fp |
| --- | --- | --- | ---: | --- |
| Bergen→Stavanger centre, longTrip off | `skip_*` | true | **206.78** | Halhjem–Sandvikvåg + Arsvågen–Mortavika — **no** `unnamed@195` |
| Bergen→Stavanger centre, longTrip on | `skip_already_connected` | true | **230.10** | same named pack ferries — **no** `unnamed@195` |
| Bergen→Stavanger station | `skip_already_connected` | true | **205.41** | same named pack ferries |

With a real Vestlandet PBF and overlay forced on after costing alignment, the unnamed 195 km chain still should not appear in single-shot because its A* weight is ~1.5 Mm drive-equivalent vs ~80 km for a tagged 40-minute hop. The historical `unnamed@195` failure required **both** length-only overlay weights **and** trip-AABB / full-coast overlay clips (corridor-band clips are already preferred on `dev`).

### Overlay need (post snap fix)

Vestlandet host cases route without overlay. Overlay remains gated: build only when directed connectivity still fails after directed snap (`ferry_hop_connectivity_gate` → `disconnected_try_overlay` / `snap_failed_try_overlay`).

### Lazy ferry sidecar (install no longer builds)

`origin/dev` started `FerrySidecarBackground` on every `emitInstalledForRouting` / `markUsable`. This branch:

- Removes install/usable sidecar kicks (hundreds of MB / multi-minute coastal builds).
- Keeps plan-path lazy kick: when overlay is needed and sidecar is stale/missing, return `ferry_preparing` and spawn `ensure_ferry_sidecar` off-thread.
- `FerrySidecarBackground` remains for explicit/manual ensure and instrumented tests.

## STEP 4 — Tablet (SM-P613 `R52TB0JQEDE`)

Device: `R52TB0JQEDE` (SM-P613). Build: `./scripts/build-android-native.sh aarch64-linux-android release` + `installDebug` + `RegionToRegionPerfMatrixInstrumentedTest` (2026-10-02).

| Case | eco | ok | wall_ms | distance_km | ferry_fp |
| --- | --- | --- | ---: | ---: | --- |
| bergen_stavanger | false | true | 10497 | **230.10** | Halhjem–Sandvikvåg + Arsvågen–Mortavika |
| bergen_stavanger | true | true | 10246 | **229.48** | same |
| bergen_stavanger_lt | false | true | 10111 | **230.10** | same |
| bergen_stavanger_lt | true | true | 10193 | **229.48** | same |

- **No** `unnamed@195` in any Bergen→Stavanger ferry fingerprint.
- Peak RSS ~951 MB; MemAvailable before coastal cases ~800–960 MB; no ANR observed during the matrix run.
- Install-time ferry sidecar kick removed on this branch; pre-existing `*.navi-ferry-overlay-*.rkyv` on the tablet (from prior mmap/sidecar builds) remain on disk but are not required for these Vestlandet OD rows (pack ferries + directed snap suffice).

Expected storage win vs Phase-2 install-time sidecars on a clean device: no per-stem overlay rkyv at install (Vestlandet/Nord-Norge/Sørlandet sidecars were hundreds of MB).

## Host harness

```bash
cargo run -p navi-ffi --release --bin directed-snap-diag -- \
  --pack-dir .packs/long-trip-packs

cargo test -p driver-break-core ferry_base_weight ferry_costing destination_skips origin_rejects -- --nocapture
```

Matrix cases: `bergen_stavanger`, `bergen_stavanger_eco`, `bergen_stavanger_lt`, `bergen_stavanger_station`.
