# Directed waypoint snap + ferry overlay reassessment

Branch: `fix/directed-snap` (from `dev` / dig tip after #138). Client only. No `graph_format_version` bump. No navi-server. No merge. PR: https://github.com/Supermagnum/Navi/pull/140

## STEPs 1–2 (recap)

Bergen→Stavanger **city centre** on a single-shot Vestlandet corridor failed because dig `SnapRole::Any` snapped to one-way **source stub** OSM `11335393456`. Destination-role snap (after deferred labels) picks `264565258`. With same-stem coastal densify (span > CHUNK), densify hop joints avoid that stub — tablet matrix matches dig geom `1cf463d5…` / **228.21 km** without ever building labels.

Labels: compact `origin_reject` / `dest_reject` stub sets; Kosaraju **only** if Any-snap A* misses (Stavanger-class). Dig-matching ODs never allocate the label working set.

## Gaps 1–3 (2026-10-03 tablet SM-P613 `R52TB0JQEDE`)

Force-stop before cold. Dig = `origin/dev` after #138.

### 1. Route changes vs dig

Restored dig **edge-distance** snap (nearest polyline, then closer endpoint). Directed filter applies only when labels are ready and only rejects endpoints that fail the role gate — dig’s node is kept whenever it is directed-usable.

| Case | dig km / geom | this branch | O/D snap | Diverge |
| --- | --- | --- | --- | --- |
| Raufoss→Bergen eco | 459.71 / `8a8c6f7b…` | **match** | Same as dig (Any; labels deferred) | — |
| Raufoss→Bergen default | 485.45 / `540bb2ad…` | **match** | Same | — |
| Raufoss→Dombås | 206.81 / `6a1360bc…` | **match** | Same | — |
| Bergen→Førde | 171.01 / `0bff0c85…` | **match** | Same | — |
| Raufoss→Tromsø | 1766.89 / `e79679c2…` | **match** | Same | — |
| Bergen→Stavanger (lt off/on, eco/default) | 228.21 / `1cf463d5…` | **match** | Densify hops; Any snaps directed-OK | Allowed exception was centre single-shot; densify restores dig |

Earlier 0.1–0.3 km / hash diffs were from a **node-haversine** snap rewrite that reordered candidates ahead of dig’s edge-nearest node. Fixed.

**Intended remaining difference:** none vs dig on the tablet densify matrix. Host single-shot Vestlandet centre without densify still uses Destination snap (`11335393456` → `264565258`) when Any A* misses. Pack ferry edges keep baked weights; `ferry_base_weight_m` applies only to client overlay/PBF edges.

### 2. Peak RSS ≤933

| Case | peak_rss_mb | nodes | What is held |
| --- | ---: | ---: | --- |
| raufoss_bergen eco cold | **843.5** | 238063 | Owned corridor (same size as dig) |
| raufoss_bergen warm / default | 843.5 | 238063 | Corridor cache Arc (no second clone) |
| raufoss_dombas | 843.5 | 135096 | Evict-before-miss; does not raise HWM |
| bergen_forde | 843.5 | 136152 | Same |
| bergen_stavanger (+ lt) | 843.5 | **167015** | Densify hops — dig never held 259k single-shot |
| raufoss_tromso | 843.5 | (per hop) | Same |

**Matrix max 843.5 MiB ≤933.**

What dig did **not** hold on the regressing tip: a **259180-node / 537935-edge** single-shot Vestlandet owned `RouteGraph` (Bergen→Stavanger longTrip off) that pushed VmHWM to ~985 MiB. Fix: same-stem coastal densify when span > CHUNK (nodes ≤~167k/hop, dig’s residency). Directed labels are **not** built on dig-matching plans (`directed_label_ms=0`); no Kosaraju HashSets on the matrix path.

### 3. Snap check cost

| Metric | Cold | Warm |
| --- | --- | --- |
| `directed_label_ms` | **0** (deferred; densify path never ensure) | **0** (labels not on corridor) |
| `directed_label_bytes` | 0 | 0 |
| `snap_ms` (dig pad scan) | ~230–720 (graph size) | ~120–227 |
| Added directed filter | n/a until ensure | n/a |

Labels computed once only after Any-snap A* miss, then kept on the corridor Arc. Target met: label build not on dig-matching warm plans; when ensure runs, filter lookups are O(1).

### Logcat

**No** `lowmemorykiller` for `no.navi.app`. **No** ANR / `am_anr`.

## Ferry overlay status

| Path | When |
| --- | --- |
| Overlay | Connectivity gate `Disconnected` / `SnapFailed` after snaps |
| Skip | Connected / corridor cache hit / stub PBF |
| Sidecar | Lazy on plan miss (`ferry_preparing`); not at pack install |
| Costing | Overlay edges: `ferry_base_weight_m`; pack edges: baked weights |

## Host harness

```bash
cargo run -p navi-ffi --release --bin directed-snap-diag -- \
  --pack-dir .packs/long-trip-packs
```
